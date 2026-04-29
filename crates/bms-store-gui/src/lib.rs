//! bms-store desktop GUI — library entry for tests and the binary.
//!
//! See `main.rs` for the desktop launch path.

pub mod gui;

/// Modules copied verbatim from opencrate that don't yet have an equivalent
/// in any bms-store-* crate. Phase B rewires references; Task 14 may move
/// platform.rs into bms-store-storage or bms-store-runtime once stable.
pub mod extracted {
    pub mod platform;
}

// Re-export `platform` at the crate root so copied component code that
// uses `crate::platform::…` keeps compiling. The other opencrate top-level
// modules (auth, config, project) live in bms-store-storage and will be
// reached via `bms_store_storage::auth::…` etc. — Phase B handles the
// path rewires.
pub use extracted::platform;
