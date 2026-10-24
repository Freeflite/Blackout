//! Where Blackout keeps its files.

use std::path::PathBuf;

/// %APPDATA%\Blackout: settings, which roam with the user profile.
pub fn config_dir() -> PathBuf {
    app_dir("APPDATA")
}

/// %LOCALAPPDATA%\Blackout: saved brightness levels and the log, which belong to this PC.
pub fn local_dir() -> PathBuf {
    app_dir("LOCALAPPDATA")
}

fn app_dir(var: &str) -> PathBuf {
    std::env::var_os(var).map_or_else(std::env::temp_dir, PathBuf::from).join("Blackout")
}
