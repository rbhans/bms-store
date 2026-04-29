use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::store::report_store::{compute_next_run_ms, ExecutionStatus, ReportStore};

use super::engine::ReportEngine;

/// Periodically checks for due report schedules and generates + delivers reports.
pub struct ReportScheduler {
    report_store: ReportStore,
    engine: Arc<ReportEngine>,
    #[allow(dead_code)]
    project_name: String,
    shutdown: CancellationToken,
}

impl ReportScheduler {
    pub fn new(
        report_store: ReportStore,
        engine: Arc<ReportEngine>,
        project_name: String,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            report_store,
            engine,
            project_name,
            shutdown,
        }
    }

    /// Start the scheduler loop (spawns a tokio task).
    /// The task runs until the tokio runtime shuts down.
    /// Same fire-and-forget pattern as AlarmRouter and MqttPublisher.
    pub fn start(self) {
        let token = self.shutdown.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(60));
            ticker.tick().await; // skip immediate tick
            tracing::info!("Report scheduler started");

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        self.check_due_schedules().await;
                    }
                    _ = token.cancelled() => {
                        tracing::info!("Report scheduler stopped");
                        break;
                    }
                }
            }
        });
    }

    async fn check_due_schedules(&self) {
        let current_ms = now_ms();
        let due = self.report_store.get_due_schedules(current_ms).await;

        for schedule in due {
            let report_id = schedule.report_id;

            // Use shared run_report for execution lifecycle
            let (exec_id, status) = match self
                .engine
                .run_report(&self.report_store, report_id, Some(schedule.id), "schedule")
                .await
            {
                Ok(result) => result,
                Err(e) => {
                    tracing::error!(report_id, "Scheduled report failed: {e}");
                    continue;
                }
            };

            // Deliver via email if generation succeeded and recipients exist
            if status == ExecutionStatus::Completed && !schedule.recipients.is_empty() {
                let definition = match self.report_store.get_definition(report_id).await {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                let exec = self.report_store.get_execution(exec_id).await.ok();
                let html = exec.as_ref().and_then(|e| e.report_html.as_deref());
                if let Some(html) = html {
                    let delivery = match self
                        .deliver_email(
                            &definition.name,
                            definition.config.time_range.label(),
                            html,
                            &schedule.recipients,
                        )
                        .await
                    {
                        Ok(()) => "sent".to_string(),
                        Err(e) => {
                            tracing::error!(report_id, "Report email delivery failed: {e}");
                            format!("failed: {e}")
                        }
                    };
                    let _ = self
                        .report_store
                        .update_execution(
                            exec_id,
                            ExecutionStatus::Completed,
                            None,
                            None,
                            None,
                            Some(delivery),
                        )
                        .await;
                }
            }

            // Compute next run and update schedule
            let completed_ms = now_ms();
            let next_run = compute_next_run_ms(
                &schedule.frequency,
                schedule.day_of_week,
                schedule.day_of_month,
                schedule.hour,
                schedule.minute,
                schedule.timezone_offset_mins,
                completed_ms,
            );
            let _ = self
                .report_store
                .update_schedule_last_run(schedule.id, completed_ms, next_run)
                .await;

            tracing::info!(
                report_id,
                schedule_id = schedule.id,
                exec_id,
                "Scheduled report executed"
            );
        }
    }

    /// Send the report HTML via email using lettre (SMTP).
    /// Reads SMTP config from the ReportStore's config table (key: "smtp_config").
    async fn deliver_email(
        &self,
        report_name: &str,
        time_range_label: &str,
        html: &str,
        recipients: &[crate::store::report_store::ReportRecipient],
    ) -> Result<(), String> {
        let smtp_json = self
            .report_store
            .get_config("smtp_config")
            .await
            .unwrap_or_default();
        if smtp_json.is_empty() {
            return Err(
                "No SMTP configuration found. Configure email settings in Reports → Schedules tab."
                    .to_string(),
            );
        }

        // Parse SMTP config from store
        let config: SmtpConfig =
            serde_json::from_str(&smtp_json).map_err(|e| format!("Invalid SMTP config: {e}"))?;

        if config.smtp_host.is_empty() {
            return Err("SMTP host not configured".to_string());
        }

        let from_address = if config.from_address.is_empty() {
            format!("opencrate@{}", config.smtp_host)
        } else {
            config.from_address.clone()
        };

        let subject = format!("[OpenCrate] {} — {}", report_name, time_range_label);

        // Build and send via lettre
        for recipient in recipients {
            let to = if recipient.name.is_empty() {
                recipient.email.clone()
            } else {
                format!("{} <{}>", recipient.name, recipient.email)
            };

            let email = lettre::Message::builder()
                .from(
                    from_address
                        .parse()
                        .map_err(|e| format!("Invalid from address: {e}"))?,
                )
                .to(to
                    .parse()
                    .map_err(|e| format!("Invalid recipient '{}': {e}", recipient.email))?)
                .subject(&subject)
                .header(lettre::message::header::ContentType::TEXT_HTML)
                .body(html.to_string())
                .map_err(|e| format!("Failed to build email: {e}"))?;

            let transport = if config.smtp_port == 465 {
                lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::builder_dangerous(
                    &config.smtp_host,
                )
                .port(config.smtp_port)
                .timeout(Some(Duration::from_secs(15)))
                .build()
            } else if !config.smtp_user.is_empty() {
                lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::starttls_relay(
                    &config.smtp_host,
                )
                .map_err(|e| format!("SMTP TLS error: {e}"))?
                .port(config.smtp_port)
                .credentials(lettre::transport::smtp::authentication::Credentials::new(
                    config.smtp_user.clone(),
                    config.smtp_password.clone(),
                ))
                .timeout(Some(Duration::from_secs(15)))
                .build()
            } else {
                lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::builder_dangerous(
                    &config.smtp_host,
                )
                .port(config.smtp_port)
                .timeout(Some(Duration::from_secs(15)))
                .build()
            };

            use lettre::AsyncTransport;
            transport
                .send(email)
                .await
                .map_err(|e| format!("SMTP send failed for {}: {e}", recipient.email))?;
        }

        Ok(())
    }
}

/// SMTP configuration stored in data/report_email_config.json.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct SmtpConfig {
    #[serde(default)]
    pub smtp_host: String,
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,
    #[serde(default)]
    pub smtp_user: String,
    #[serde(default)]
    pub smtp_password: String,
    #[serde(default)]
    pub from_address: String,
}

fn default_smtp_port() -> u16 {
    587
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
