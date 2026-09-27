//! Host-side cartridge services; the programmer firmware remains unchanged.
pub mod error;
pub mod rom;
pub mod storage;
pub use error::{Error, Result};
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub mod famicom;
pub mod games;
pub mod gb;
pub mod gba;
pub mod gbflash;
pub mod gbxcart;
pub mod inl_gba;
pub mod operations;
pub mod operator;
pub mod readers;
pub mod service;
pub mod usb;
