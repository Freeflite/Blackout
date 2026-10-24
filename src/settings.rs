//! User settings, kept in %APPDATA%\Blackout\settings.toml.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::logfile;
use crate::monitors::Monitor;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Black out every monitor, including ones connected later. When off, only `selected`.
    pub all_monitors: bool,
    /// IDs of the monitors to black out when `all_monitors` is off.
    pub selected: Vec<String>,
    pub hotkey: String,
    /// A full blackout wakes after this many Enter taps within `wake_window_secs`.
    pub wake_taps: u32,
    pub wake_window_secs: f32,
    /// Turn the backlight down over DDC/CI while blacked out.
    pub dim_backlight: bool,
    /// Keep the PC and displays awake during a full blackout (every monitor).
    pub keep_awake_full: bool,
    /// Keep the PC and displays awake during a partial blackout (some monitors stay on).
    pub keep_awake_partial: bool,
    /// Every monitor the settings window has seen, so choices and nicknames outlive unplugging.
    pub known: BTreeMap<String, KnownMonitor>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct KnownMonitor {
    pub label: String,
    pub nickname: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            all_monitors: true,
            selected: Vec::new(),
            hotkey: "Ctrl+Alt+B".into(),
            wake_taps: 3,
            wake_window_secs: 2.0,
            dim_backlight: true,
            keep_awake_full: true,
            keep_awake_partial: false,
            known: BTreeMap::new(),
        }
    }
}

fn path() -> PathBuf {
    crate::paths::config_dir().join("settings.toml")
}

impl Settings {
    pub fn load() -> Self {
        match std::fs::read_to_string(path()) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                logfile::write(&format!("settings.toml can't be read, using defaults: {e}"));
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::create_dir_all(crate::paths::config_dir())?;
        std::fs::write(path(), text)
    }

    pub fn is_target(&self, monitor_id: &str) -> bool {
        self.all_monitors || self.selected.iter().any(|id| id == monitor_id)
    }

    pub fn set_selected(&mut self, monitor_id: &str, selected: bool) {
        self.selected.retain(|id| id != monitor_id);
        if selected {
            self.selected.push(monitor_id.to_string());
        }
    }

    /// The nickname if one was given, otherwise the model and serial.
    pub fn name_for(&self, monitor: &Monitor) -> String {
        self.known
            .get(&monitor.id)
            .map(|known| known.nickname.trim())
            .filter(|nickname| !nickname.is_empty())
            .map_or_else(|| monitor.label(), str::to_string)
    }

    pub fn wake_taps(&self) -> usize {
        self.wake_taps.clamp(2, 5) as usize
    }

    pub fn wake_window(&self) -> Duration {
        Duration::from_secs_f32(self.wake_window_secs.clamp(1.0, 5.0))
    }

    /// Whether a blackout in progress has to be redone to match `other`.
    pub fn blackout_differs(&self, other: &Settings) -> bool {
        self.all_monitors != other.all_monitors
            || (!other.all_monitors && self.selected != other.selected)
            || self.dim_backlight != other.dim_backlight
            || self.keep_awake_full != other.keep_awake_full
            || self.keep_awake_partial != other.keep_awake_partial
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let mut settings = Settings { all_monitors: false, ..Settings::default() };
        settings.set_selected("GSM776F-305NTZX56789", true);
        settings.known.insert(
            r"DISPLAY\ABC1234\5&1&UID4".into(),
            KnownMonitor { label: "Generic".into(), nickname: "Facing away".into() },
        );
        let text = toml::to_string_pretty(&settings).unwrap();
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), settings);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let settings: Settings = toml::from_str("hotkey = \"Ctrl+Alt+K\"").unwrap();
        assert_eq!(settings.hotkey, "Ctrl+Alt+K");
        assert_eq!(settings.wake_taps, 3);
        assert!(settings.all_monitors && settings.keep_awake_full && !settings.keep_awake_partial);
    }

    #[test]
    fn targets_follow_the_selection() {
        let mut settings = Settings { all_monitors: false, ..Settings::default() };
        assert!(!settings.is_target("a"));
        settings.set_selected("a", true);
        settings.set_selected("a", true);
        assert_eq!(settings.selected, ["a"]);
        assert!(settings.is_target("a") && !settings.is_target("b"));
        settings.set_selected("a", false);
        assert!(settings.selected.is_empty());
    }

    #[test]
    fn wake_limits_are_clamped() {
        let settings = Settings { wake_taps: 9, wake_window_secs: 0.1, ..Settings::default() };
        assert_eq!(settings.wake_taps(), 5);
        assert_eq!(settings.wake_window(), Duration::from_secs(1));
    }

    #[test]
    fn only_blackout_changes_force_a_redo() {
        let before = Settings::default();
        let mut nickname_only = before.clone();
        nickname_only.known.insert("a".into(), KnownMonitor { label: "x".into(), nickname: "Left".into() });
        assert!(!before.blackout_differs(&nickname_only));
        let partial = Settings { all_monitors: false, ..before.clone() };
        assert!(before.blackout_differs(&partial));
    }
}
