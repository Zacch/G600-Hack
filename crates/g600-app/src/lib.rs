//! Shared by the tray switcher and the profile editor.

pub mod apps;
pub mod config;
pub mod slots;

use std::path::PathBuf;

/// %APPDATA%\g600cfg
pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("g600cfg"))
}
