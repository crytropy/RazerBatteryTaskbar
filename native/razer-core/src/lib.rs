pub mod device_db;
pub mod diagnostics;
pub mod events;
pub mod frontend;
pub mod integration;
pub mod manager;
pub mod notifications;
pub mod protocol;
pub mod service;
pub mod settings;
pub mod state;
pub mod transport;

#[cfg(all(windows, feature = "windows-hid"))]
pub mod windows_hid;

pub const RAZER_VENDOR_ID: u16 = 0x1532;
