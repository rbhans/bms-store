use std::net::SocketAddr;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "bms_stored=info,bms_store_server=info,bms_store_bridges=info".into()
            }),
        )
        .init();

    let addr = std::env::var("BMS_STORE_ADDR")
        .ok()
        .and_then(|value| value.parse::<SocketAddr>().ok())
        .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 8080)));

    let project = parse_project_arg();
    if let Some(project) = project {
        let paths = bms_store_storage::project::ProjectPaths::from_root(project.clone());
        let storage = bms_store_storage::boot::boot_project(project).await?;
        let (bridges, report) = bms_store_bridges::boot::boot_bridges(&storage).await?;
        if !report.all_ok() {
            for (label, error) in report.failures() {
                tracing::warn!(bridge = %label, error = %error, "Bridge failed to start");
            }
        }

        let jwt_secret = bms_store_server::api::load_or_create_jwt_secret(
            &paths.data_dir.join("api_secret.key"),
        );
        let project_id = bms_store_storage::project::load_project_meta(&paths.root)
            .ok()
            .map(|meta| meta.id)
            .unwrap_or_else(|| "default".to_string());
        let backup_scheduler =
            bms_store_storage::backup::BackupScheduler::new(&project_id, &paths.data_dir);
        backup_scheduler.start();

        let api_state = bms_store_server::api::ApiState::from_runtimes(
            &storage,
            &bridges,
            backup_scheduler,
            bms_store_server::api::api_keys::ApiKeyStore::new(paths.data_dir.join("api_keys.json")),
            jwt_secret,
        );
        bms_store_server::serve_api(addr, api_state).await?;
    } else {
        tracing::info!("No --project supplied; starting health server without storage runtime");
        bms_store_server::serve(addr).await?;
    }

    Ok(())
}

fn parse_project_arg() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--project" {
            return args.next().map(PathBuf::from);
        }
        if let Some(value) = arg.strip_prefix("--project=") {
            return Some(PathBuf::from(value));
        }
    }
    None
}
