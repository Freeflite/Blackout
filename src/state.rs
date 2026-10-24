//! Original brightness levels, written to disk before dimming so they can be put back even if
//! Blackout is killed, or the PC restarts, in the middle of a blackout.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::logfile;

#[derive(Serialize, Deserialize, Default)]
struct StateFile {
    #[serde(default)]
    levels: BTreeMap<String, u32>,
}

fn path() -> PathBuf {
    crate::paths::local_dir().join("brightness.toml")
}

/// Key for one physical monitor: its stable monitor ID plus its position behind that monitor.
pub fn key(monitor_id: &str, index: usize) -> String {
    format!("{monitor_id}#{index}")
}

/// Levels still waiting to be put back, by `key`.
pub fn load() -> BTreeMap<String, u32> {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|text| toml::from_str::<StateFile>(&text).ok())
        .map(|file| file.levels)
        .unwrap_or_default()
}

/// Records the levels still waiting to be put back; an empty set removes the file.
pub fn save(levels: &BTreeMap<String, u32>) {
    let path = path();
    if levels.is_empty() {
        let _ = std::fs::remove_file(&path);
        return;
    }
    let text = toml::to_string(&StateFile { levels: levels.clone() }).unwrap_or_default();
    let written = std::fs::create_dir_all(crate::paths::local_dir()).and_then(|()| std::fs::write(&path, text));
    if let Err(e) = written {
        logfile::write(&format!("can't save brightness levels to {}: {e}", path.display()));
    }
}
