//! tagjoe core: scan folders, check every tag field, propose and apply fixes.
#![forbid(unsafe_code)]

pub mod classify;
pub mod config;
pub mod containers;
pub mod model;
pub mod naming;
pub mod propose;
pub mod scan;
pub mod schema;

pub use config::Config;
pub use model::{Field, FileInfo, Kind, Unit};
pub use propose::{FilePlan, Plan, ReleaseInfo};
pub use schema::Status;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const USER_AGENT: &str = concat!("tagjoe/", env!("CARGO_PKG_VERSION"), " (https://github.com/joeMJ/tagjoe)");
