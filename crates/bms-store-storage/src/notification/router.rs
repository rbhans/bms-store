use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::broadcast;

use crate::event::bus::{Event, EventBus};
use crate::event::durable_sub::DurableSubscription;
use crate::event::journal::EventJournal;
use crate::store::alarm_store::{AlarmSeverity, AlarmStore};
use crate::store::notification_store::{
    ChannelType, DeliveryStatus, NotificationStore, RoutingRule,
};
use crate::store::schedule_store::ScheduleStore;

use super::channel::{NotificationChannel, NotificationEventType, NotificationPayload};
use super::email::EmailChannel;
use super::sms::SmsChannel;
use super::webhook::WebhookChannel;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Exponential backoff for notification retries.
fn retry_delay_ms(attempt: u32) -> i64 {
    let base_ms: i64 = 30_000; // 30 seconds
    let max_ms: i64 = 3_600_000; // 1 hour
    let delay = base_ms * (1i64 << attempt.min(10));
    delay.min(max_ms)
}

const MAX_ATTEMPTS: u32 = 8;

pub struct AlarmRouter {
    notification_store: NotificationStore,
    alarm_store: AlarmStore,
    #[allow(dead_code)]
    schedule_store: ScheduleStore,
    channels: HashMap<ChannelType, Arc<dyn NotificationChannel>>,
    project_name: String,
}

impl AlarmRouter {
    pub fn new(
        notification_store: NotificationStore,
        alarm_store: AlarmStore,
        schedule_store: ScheduleStore,
        project_name: String,
    ) -> Self {
        let mut channels: HashMap<ChannelType, Arc<dyn NotificationChannel>> = HashMap::new();
        channels.insert(ChannelType::Webhook, Arc::new(WebhookChannel::new()));
        channels.insert(ChannelType::Email, Arc::new(EmailChannel::new()));
        channels.insert(ChannelType::Sms, Arc::new(SmsChannel::new()));

        Self {
            notification_store,
            alarm_store,
            schedule_store,
            channels,
            project_name,
        }
    }

    /// Start the router loop. Spawns a tokio task that runs until shutdown or EventBus closes.
    ///
    /// When `journal` is provided, uses a [`DurableSubscription`] to replay
    /// missed events from the journal on startup.
    pub fn start(
        self,
        event_bus: &EventBus,
        shutdown: Option<tokio_util::sync::CancellationToken>,
        journal: Option<&EventJournal>,
    ) {
        let live_rx = event_bus.subscribe();
        let journal_clone = journal.cloned();

        tokio::spawn(async move {
            let mut retry_ticker = tokio::time::interval(tokio::time::Duration::from_secs(30));
            let mut escalation_ticker = tokio::time::interval(tokio::time::Duration::from_secs(60));
            let mut shelving_ticker = tokio::time::interval(tokio::time::Duration::from_secs(300));
            retry_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            escalation_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            shelving_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            if let Some(ref j) = journal_clone {
                let mut durable = DurableSubscription::new("alarm-router", j, live_rx).await;
                loop {
                    tokio::select! {
                        result = durable.recv() => {
                            match result {
                                Ok(event) => self.handle_event(&event).await,
                                Err(broadcast::error::RecvError::Closed) => break,
                                _ => continue,
                            }
                        }
                        _ = retry_ticker.tick() => self.process_retries().await,
                        _ = escalation_ticker.tick() => self.process_escalations().await,
                        _ = shelving_ticker.tick() => {
                            let cleaned = self.notification_store.clean_expired_shelving().await;
                            if cleaned > 0 {
                                tracing::debug!(cleaned, "cleaned expired alarm shelving entries");
                            }
                            durable.commit_latest();
                        }
                        _ = async { match &shutdown { Some(t) => t.cancelled().await, None => std::future::pending().await } } => {
                            tracing::info!("Alarm router shutting down");
                            durable.commit_latest();
                            break;
                        }
                    }
                }
            } else {
                let mut rx = live_rx;
                loop {
                    tokio::select! {
                        result = rx.recv() => {
                            match result {
                                Ok(event) => self.handle_event(&event).await,
                                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                                Err(broadcast::error::RecvError::Closed) => break,
                            }
                        }
                        _ = retry_ticker.tick() => self.process_retries().await,
                        _ = escalation_ticker.tick() => self.process_escalations().await,
                        _ = shelving_ticker.tick() => {
                            let cleaned = self.notification_store.clean_expired_shelving().await;
                            if cleaned > 0 {
                                tracing::debug!(cleaned, "cleaned expired alarm shelving entries");
                            }
                        }
                        _ = async { match &shutdown { Some(t) => t.cancelled().await, None => std::future::pending().await } } => {
                            tracing::info!("Alarm router shutting down");
                            break;
                        }
                    }
                }
            }
            tracing::info!("alarm router stopped");
        });
    }

    async fn handle_event(&self, event: &Event) {
        match event {
            Event::AlarmRaised { alarm_id, node_id } => {
                self.on_alarm_raised(*alarm_id, node_id).await;
            }
            Event::AlarmCleared { alarm_id, node_id } => {
                self.on_alarm_cleared(*alarm_id, node_id).await;
            }
            Event::AlarmAcknowledged { .. } => {
                // Acknowledgement is not a clear — do not send "cleared" notifications.
                // Notifications only fire on actual AlarmCleared (return-to-normal).
            }
            Event::FddFaultRaised {
                fault_id,
                equip_id,
                severity,
                rule_id,
            } => {
                self.on_fdd_fault_raised(*fault_id, *rule_id, equip_id, severity)
                    .await;
            }
            Event::FddFaultCleared { equip_id, .. } => {
                tracing::debug!(equip = %equip_id, "FDD fault cleared");
            }
            _ => {}
        }
    }

    async fn on_alarm_raised(&self, alarm_id: i64, _node_id: &str) {
        let active = self.alarm_store.get_active_alarms().await;
        let alarm = match active.iter().find(|a| a.config_id == alarm_id) {
            Some(a) => a,
            None => return,
        };

        // Check shelving
        if self
            .notification_store
            .is_shelved(Some(alarm_id), Some(alarm.device_id.clone()))
            .await
        {
            tracing::debug!(
                alarm_id,
                device_id = alarm.device_id,
                "alarm shelved — skipping notification"
            );
            return;
        }

        // Load rules and filter
        let rules = self.notification_store.list_rules().await;
        let recipients = self.notification_store.list_recipients().await;
        let recipient_map: HashMap<i64, _> = recipients.into_iter().map(|r| (r.id, r)).collect();

        for rule in &rules {
            if !rule.enabled {
                continue;
            }
            // Only dispatch tier-0 rules immediately
            if rule.escalation_tier > 0 {
                continue;
            }
            if !self.rule_matches(
                rule,
                &alarm.severity,
                &alarm.device_id,
                alarm.alarm_type.as_str(),
            ) {
                continue;
            }

            let recipient = match recipient_map.get(&rule.recipient_id) {
                Some(r) if r.enabled => r,
                _ => continue,
            };

            let payload = NotificationPayload {
                alarm_id,
                alarm_config_id: alarm.config_id,
                device_id: alarm.device_id.clone(),
                point_id: alarm.point_id.clone(),
                alarm_type: alarm.alarm_type.as_str().to_string(),
                severity: alarm.severity.as_str().to_string(),
                trigger_value: alarm.trigger_value,
                trigger_time_ms: alarm.trigger_time_ms,
                context_snapshot: alarm.context_snapshot.clone(),
                event_type: NotificationEventType::Raised,
                recipient_name: recipient.name.clone(),
                project_name: self.project_name.clone(),
            };

            self.dispatch(
                &payload,
                recipient.channel_type.clone(),
                &recipient.address,
                &recipient.channel_config,
                recipient.id,
                rule.id,
            )
            .await;
        }
    }

    async fn on_alarm_cleared(&self, alarm_id: i64, node_id: &str) {
        // Parse device_id and point_id from node_id (format: "device_id/point_id")
        let (device_id, point_id) = match node_id.split_once('/') {
            Some((d, p)) => (d.to_string(), p.to_string()),
            None => {
                tracing::warn!(
                    alarm_id,
                    node_id,
                    "cannot parse node_id for clear notification"
                );
                return;
            }
        };

        // Look up the alarm config to get severity and type for rule matching.
        // The alarm is already removed from active by the time AlarmCleared fires,
        // so we query alarm_history for the most recent offnormal→normal transition.
        let history = self
            .alarm_store
            .query_history(crate::store::alarm_store::AlarmHistoryQuery {
                device_id: Some(device_id.clone()),
                point_id: Some(point_id.clone()),
                to_state: Some("normal".to_string()),
                limit: Some(1),
                ..Default::default()
            })
            .await
            .unwrap_or_default();

        let (alarm_type_str, severity, value, time, context) = if let Some(evt) = history.first() {
            (
                String::new(), // alarm_type not stored in history, use empty for now
                evt.severity,
                evt.value,
                evt.timestamp_ms,
                evt.context_snapshot.clone().unwrap_or_default(),
            )
        } else {
            // Fallback — we don't have history, but we still know the device/point from the event
            (
                String::new(),
                crate::store::alarm_store::AlarmSeverity::Info,
                0.0,
                now_ms(),
                String::new(),
            )
        };

        let rules = self.notification_store.list_rules().await;
        let recipients = self.notification_store.list_recipients().await;
        let recipient_map: HashMap<i64, _> = recipients.into_iter().map(|r| (r.id, r)).collect();

        for rule in &rules {
            if !rule.enabled || !rule.notify_on_clear {
                continue;
            }
            if !self.rule_matches(rule, &severity, &device_id, &alarm_type_str) {
                continue;
            }

            let recipient = match recipient_map.get(&rule.recipient_id) {
                Some(r) if r.enabled => r,
                _ => continue,
            };

            let payload = NotificationPayload {
                alarm_id,
                alarm_config_id: alarm_id,
                device_id: device_id.clone(),
                point_id: point_id.clone(),
                alarm_type: alarm_type_str.clone(),
                severity: severity.as_str().to_string(),
                trigger_value: value,
                trigger_time_ms: time,
                context_snapshot: context.clone(),
                event_type: NotificationEventType::Cleared,
                recipient_name: recipient.name.clone(),
                project_name: self.project_name.clone(),
            };

            self.dispatch(
                &payload,
                recipient.channel_type.clone(),
                &recipient.address,
                &recipient.channel_config,
                recipient.id,
                rule.id,
            )
            .await;
        }
    }

    async fn on_fdd_fault_raised(
        &self,
        fault_id: i64,
        _rule_id: i64,
        equip_id: &str,
        severity: &str,
    ) {
        let fdd_severity = match AlarmSeverity::from_str(severity) {
            Some(s) => s,
            None => AlarmSeverity::from_str("warning").unwrap_or(AlarmSeverity::Info),
        };

        let rules = self.notification_store.list_rules().await;
        let recipients = self.notification_store.list_recipients().await;
        let recipient_map: HashMap<i64, _> = recipients.into_iter().map(|r| (r.id, r)).collect();

        for rule in &rules {
            if !rule.enabled {
                continue;
            }
            if rule.escalation_tier > 0 {
                continue;
            }
            // Match FDD faults using severity + alarm_type "fdd"
            if !self.rule_matches(rule, &fdd_severity, equip_id, "fdd") {
                continue;
            }

            let recipient = match recipient_map.get(&rule.recipient_id) {
                Some(r) if r.enabled => r,
                _ => continue,
            };

            let payload = NotificationPayload {
                alarm_id: fault_id,
                alarm_config_id: fault_id,
                device_id: equip_id.to_string(),
                point_id: String::new(),
                alarm_type: "fdd".to_string(),
                severity: severity.to_string(),
                trigger_value: 0.0,
                trigger_time_ms: now_ms(),
                context_snapshot: String::new(),
                event_type: NotificationEventType::Raised,
                recipient_name: recipient.name.clone(),
                project_name: self.project_name.clone(),
            };

            self.dispatch(
                &payload,
                recipient.channel_type.clone(),
                &recipient.address,
                &recipient.channel_config,
                recipient.id,
                rule.id,
            )
            .await;
        }
    }

    fn rule_matches(
        &self,
        rule: &RoutingRule,
        severity: &AlarmSeverity,
        device_id: &str,
        alarm_type: &str,
    ) -> bool {
        // Check min severity
        if let Some(min_sev) = AlarmSeverity::from_str(&rule.min_severity) {
            if *severity < min_sev {
                return false;
            }
        }

        // Check device filter
        if !rule.device_filter.is_empty() {
            let devices: Vec<&str> = rule.device_filter.split(',').map(|s| s.trim()).collect();
            if !devices.contains(&device_id) {
                return false;
            }
        }

        // Check alarm type filter
        if !rule.alarm_type_filter.is_empty() {
            let types: Vec<&str> = rule
                .alarm_type_filter
                .split(',')
                .map(|s| s.trim())
                .collect();
            if !types.contains(&alarm_type) {
                return false;
            }
        }

        true
    }

    async fn dispatch(
        &self,
        payload: &NotificationPayload,
        channel_type: ChannelType,
        address: &str,
        config: &str,
        recipient_id: i64,
        rule_id: i64,
    ) {
        // Create notification log entry
        let notif_id = match self
            .notification_store
            .insert_notification(
                payload.alarm_id,
                recipient_id,
                rule_id,
                channel_type.clone(),
                address,
            )
            .await
        {
            Ok(id) => id,
            Err(e) => {
                tracing::error!("failed to create notification log: {e}");
                return;
            }
        };

        // Send via channel
        let channel: Arc<dyn NotificationChannel> = match self.channels.get(&channel_type) {
            Some(ch) => ch.clone(),
            None => {
                tracing::error!(?channel_type, "no channel implementation");
                let _ = self
                    .notification_store
                    .update_notification_status(
                        notif_id,
                        DeliveryStatus::Failed,
                        Some("no channel impl".to_string()),
                        None,
                    )
                    .await;
                return;
            }
        };

        match channel.send(address, config, payload).await {
            Ok(()) => {
                tracing::info!(notif_id, ?channel_type, address, "notification delivered");
                let _ = self
                    .notification_store
                    .update_notification_status(notif_id, DeliveryStatus::Delivered, None, None)
                    .await;
            }
            Err(e) => {
                tracing::warn!(notif_id, ?channel_type, address, "notification failed: {e}");
                let next_retry = now_ms() + retry_delay_ms(1);
                let _ = self
                    .notification_store
                    .update_notification_status(
                        notif_id,
                        DeliveryStatus::Retrying,
                        Some(e.to_string()),
                        Some(next_retry),
                    )
                    .await;
            }
        }
    }

    async fn process_retries(&self) {
        let pending = self.notification_store.get_pending_retries().await;
        for record in pending {
            if record.attempt_count >= MAX_ATTEMPTS {
                let _ = self
                    .notification_store
                    .update_notification_status(
                        record.id,
                        DeliveryStatus::Failed,
                        record.last_error.clone(),
                        None,
                    )
                    .await;
                continue;
            }

            let channel: Arc<dyn NotificationChannel> =
                match self.channels.get(&record.channel_type) {
                    Some(ch) => ch.clone(),
                    None => continue,
                };

            // Increment attempt count before the retry send
            self.notification_store.increment_attempt_count(record.id);

            // Reconstruct a minimal payload for retry
            let payload = NotificationPayload {
                alarm_id: record.alarm_id,
                alarm_config_id: record.alarm_id,
                device_id: String::new(),
                point_id: String::new(),
                alarm_type: String::new(),
                severity: String::new(),
                trigger_value: 0.0,
                trigger_time_ms: 0,
                context_snapshot: String::new(),
                event_type: NotificationEventType::Raised,
                recipient_name: String::new(),
                project_name: self.project_name.clone(),
            };

            // Try to enrich from active alarm
            let active = self.alarm_store.get_active_alarms().await;
            let enriched =
                if let Some(alarm) = active.iter().find(|a| a.config_id == record.alarm_id) {
                    NotificationPayload {
                        alarm_config_id: alarm.config_id,
                        device_id: alarm.device_id.clone(),
                        point_id: alarm.point_id.clone(),
                        alarm_type: alarm.alarm_type.as_str().to_string(),
                        severity: alarm.severity.as_str().to_string(),
                        trigger_value: alarm.trigger_value,
                        trigger_time_ms: alarm.trigger_time_ms,
                        context_snapshot: alarm.context_snapshot.clone(),
                        ..payload
                    }
                } else {
                    payload
                };

            // Find recipient config
            let recipients = self.notification_store.list_recipients().await;
            let config = recipients
                .iter()
                .find(|r| r.id == record.recipient_id)
                .map(|r| r.channel_config.as_str())
                .unwrap_or("{}");

            match channel.send(&record.address, config, &enriched).await {
                Ok(()) => {
                    tracing::info!(id = record.id, "retry succeeded");
                    let _ = self
                        .notification_store
                        .update_notification_status(
                            record.id,
                            DeliveryStatus::Delivered,
                            None,
                            None,
                        )
                        .await;
                }
                Err(e) => {
                    let next_attempt = record.attempt_count + 1;
                    let status = if next_attempt >= MAX_ATTEMPTS {
                        DeliveryStatus::Failed
                    } else {
                        DeliveryStatus::Retrying
                    };
                    let next_retry = if status == DeliveryStatus::Retrying {
                        Some(now_ms() + retry_delay_ms(next_attempt))
                    } else {
                        None
                    };
                    let _ = self
                        .notification_store
                        .update_notification_status(
                            record.id,
                            status,
                            Some(e.to_string()),
                            next_retry,
                        )
                        .await;
                }
            }
        }
    }

    async fn process_escalations(&self) {
        let now = now_ms();
        let active_alarms = self.alarm_store.get_active_alarms().await;
        let rules = self.notification_store.list_rules().await;
        let recipients = self.notification_store.list_recipients().await;
        let recipient_map: HashMap<i64, _> = recipients.into_iter().map(|r| (r.id, r)).collect();

        // Get existing notifications to avoid duplicates
        let existing_log = self.notification_store.query_notification_log(10000).await;

        for alarm in &active_alarms {
            // Only escalate unacknowledged alarms
            if alarm.state != crate::store::alarm_store::AlarmState::Offnormal {
                continue;
            }

            let minutes_since = (now - alarm.trigger_time_ms) / 60_000;

            for rule in &rules {
                if !rule.enabled || rule.escalation_tier == 0 {
                    continue;
                }
                if (minutes_since as u32) < rule.escalation_delay_mins {
                    continue;
                }
                if !self.rule_matches(
                    rule,
                    &alarm.severity,
                    &alarm.device_id,
                    alarm.alarm_type.as_str(),
                ) {
                    continue;
                }

                // Check if we already notified for this alarm + rule
                let already_sent = existing_log
                    .iter()
                    .any(|n| n.alarm_id == alarm.config_id && n.rule_id == rule.id);
                if already_sent {
                    continue;
                }

                let recipient = match recipient_map.get(&rule.recipient_id) {
                    Some(r) if r.enabled => r,
                    _ => continue,
                };

                let payload = NotificationPayload {
                    alarm_id: alarm.config_id,
                    alarm_config_id: alarm.config_id,
                    device_id: alarm.device_id.clone(),
                    point_id: alarm.point_id.clone(),
                    alarm_type: alarm.alarm_type.as_str().to_string(),
                    severity: alarm.severity.as_str().to_string(),
                    trigger_value: alarm.trigger_value,
                    trigger_time_ms: alarm.trigger_time_ms,
                    context_snapshot: alarm.context_snapshot.clone(),
                    event_type: NotificationEventType::Escalated,
                    recipient_name: recipient.name.clone(),
                    project_name: self.project_name.clone(),
                };

                tracing::info!(
                    alarm_id = alarm.config_id,
                    tier = rule.escalation_tier,
                    recipient = recipient.name,
                    "escalating alarm notification"
                );

                self.dispatch(
                    &payload,
                    recipient.channel_type.clone(),
                    &recipient.address,
                    &recipient.channel_config,
                    recipient.id,
                    rule.id,
                )
                .await;
            }
        }
    }
}
