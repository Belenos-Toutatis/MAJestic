//! MAJestic engine.
//!
//! * [`winget`], [`pkgmgr`]: talk to winget, Chocolatey, Scoop and the Store.
//! * [`system`]: registry inventory, processes, icons, elevation.
//! * [`intel`]: security catalog, release notes, recommendations, lookups.
//! * [`scan`]: puts everything together into a [`model::ScanResult`].
//! * [`jobs`]: runs installations and reports [`model::JobEvent`]s.

pub mod eta;
pub mod exec;
pub mod history;
pub mod intel;
pub mod jobs;
pub mod model;
pub mod paths;
pub mod pkgmgr;
pub mod scan;
pub mod scheduler;
pub mod settings;
pub mod system;
pub mod version;
pub mod winget;

pub use model::*;

pub const APP_NAME: &str = "MAJestic";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const USER_AGENT: &str = concat!("MAJestic/", env!("CARGO_PKG_VERSION"), " (Windows update assistant)");
