//! Shared presentation state and isolated process transport. No widget owns USB.
pub mod files;
pub mod model;
pub mod worker;
pub use cartridge_core::readers::Kind as ReaderKind;
pub use cartridge_core::{Error, Result, VERSION};
pub use model::*;
