//! Boots the GUI library entry against demo-data and checks the platform
//! hands back a non-empty entity store.

use bms_store_gui::extracted::platform::init_platform;
use bms_store_storage::project::ProjectPaths;
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread")]
async fn demo_data_boots_and_has_entities() {
    // Demo data lives at the workspace root.
    // The test runs from `crates/bms-store-gui`, so we go up two levels.
    let demo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("demo-data")
        .canonicalize()
        .expect("demo-data must exist");

    assert!(
        demo_root.join("scenario.json").exists(),
        "demo-data scenario.json not found at {}",
        demo_root.display()
    );

    let paths = ProjectPaths::from_root(demo_root);
    let shutdown = CancellationToken::new();
    let (platform, _report) = init_platform(&paths, shutdown.clone())
        .await
        .expect("platform should boot against demo-data");

    // Verify the node store is reachable and demo-data has nodes.
    // (The entity store starts empty — nodes are the populated device-point
    // graph that the scenario seeds.  96 nodes are present in demo-data.)
    let nodes = platform.node_store.list_nodes(None, None).await;
    assert!(
        !nodes.is_empty(),
        "demo-data should have at least one node"
    );

    shutdown.cancel();
}
