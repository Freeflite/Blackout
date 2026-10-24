//! The settings window (`blackout --settings`). It runs as its own short-lived process so the copy in
//! the tray stays small. Changes save as you make them, and the tray copy reloads them.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow};
use windows::core::{PCWSTR, w};

use crate::hotkey::Hotkey;
use crate::ipc::{self, HotkeyStatus};
use crate::monitors::{self, Monitor};
use crate::settings::Settings;
use crate::{autostart, ddc, icon_art, logfile, overlay};

const TITLE: &str = "Blackout settings";
/// Typing a nickname saves once you pause rather than on every keystroke.
const SAVE_DELAY: Duration = Duration::from_millis(500);
const STATUS_POLL: Duration = Duration::from_secs(1);

pub fn run() {
    let Some(_instance) = ipc::SingleInstance::acquire(w!("Local\\Blackout.Settings")) else {
        // Already open: bring that window forward instead.
        unsafe {
            if let Ok(hwnd) = FindWindowW(PCWSTR::null(), w!("Blackout settings")) {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
            }
        }
        return;
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(TITLE)
            .with_inner_size([600.0, 580.0])
            .with_min_inner_size([480.0, 400.0])
            .with_icon(egui::IconData { rgba: icon_art::icon_rgba(64), width: 64, height: 64 }),
        ..Default::default()
    };
    let result = eframe::run_native(TITLE, options, Box::new(|cc| Ok(Box::new(SettingsApp::new(&cc.egui_ctx)))));
    if let Err(e) = result {
        logfile::write(&format!("settings window failed: {e}"));
    }
}

struct SettingsApp {
    settings: Settings,
    /// When the settings last changed without being saved yet.
    dirty_since: Option<Instant>,
    /// Connected monitors, left to right; their numbers match Identify.
    monitors: Vec<Monitor>,
    /// Monitor ID → whether its backlight can be turned down; filled in by a background probe.
    dimmable: Arc<Mutex<HashMap<String, bool>>>,
    capturing_hotkey: bool,
    hotkey_error: Option<String>,
    autostart: bool,
    engine: Option<HotkeyStatus>,
    engine_checked: Instant,
}

impl SettingsApp {
    fn new(ctx: &egui::Context) -> Self {
        let mut settings = Settings::load();
        let before = settings.clone();
        let monitors = monitors::enumerate();
        // Remember every monitor seen, so its choice and nickname survive while it's unplugged.
        for monitor in &monitors {
            settings.known.entry(monitor.id.clone()).or_default().label = monitor.label();
        }
        let dirty_since = (settings != before).then(Instant::now);

        let dimmable = Arc::new(Mutex::new(HashMap::new()));
        let (results, ctx) = (dimmable.clone(), ctx.clone());
        std::thread::spawn(move || {
            for monitor in monitors::enumerate() {
                let works = ddc::supports_brightness(monitor.handle());
                results.lock().unwrap().insert(monitor.id, works);
                ctx.request_repaint();
            }
        });

        Self {
            settings,
            dirty_since,
            monitors,
            dimmable,
            capturing_hotkey: false,
            hotkey_error: None,
            autostart: autostart::is_enabled(),
            engine: ipc::engine_status(),
            engine_checked: Instant::now(),
        }
    }

    fn save(&mut self) {
        match self.settings.save() {
            Ok(()) => {
                ipc::post(ipc::WM_RELOAD, 0);
            }
            Err(e) => logfile::write(&format!("can't save settings: {e}")),
        }
        self.dirty_since = None;
    }

    fn save_when_settled(&mut self, ctx: &egui::Context) {
        if let Some(since) = self.dirty_since {
            let waited = since.elapsed();
            if waited >= SAVE_DELAY {
                self.save();
            } else {
                ctx.request_repaint_after(SAVE_DELAY - waited);
            }
        }
    }

    fn poll_engine(&mut self, ctx: &egui::Context) {
        if self.engine_checked.elapsed() >= STATUS_POLL {
            self.engine = ipc::engine_status();
            self.engine_checked = Instant::now();
        }
        ctx.request_repaint_after(STATUS_POLL);
    }

    fn status_section(&mut self, ui: &mut egui::Ui) {
        match self.engine {
            None => {
                ui.horizontal(|ui| {
                    ui.label("Blackout isn't running in the tray.");
                    if ui.button("Start it").clicked() {
                        start_resident();
                    }
                });
            }
            Some(HotkeyStatus::Unavailable) => {
                let warning = format!("{} is taken by another app. Pick a different hotkey below.", self.settings.hotkey);
                ui.colored_label(ui.visuals().warn_fg_color, warning);
            }
            Some(_) => {
                ui.label(format!("Running in the tray. {} turns a blackout on and off.", self.settings.hotkey));
            }
        }
    }

    fn monitors_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Monitors");
        ui.radio_value(&mut self.settings.all_monitors, true, "All monitors, including ones connected later");
        ui.radio_value(&mut self.settings.all_monitors, false, "Only the ones I pick");
        ui.add_space(4.0);

        let dimmable = self.dimmable.lock().unwrap().clone();
        egui::Grid::new("monitors").num_columns(5).spacing([12.0, 6.0]).show(ui, |ui| {
            for (i, monitor) in self.monitors.iter().enumerate() {
                let mut picked = self.settings.is_target(&monitor.id);
                let checkbox = ui.add_enabled(!self.settings.all_monitors, egui::Checkbox::without_text(&mut picked));
                if checkbox.changed() {
                    self.settings.set_selected(&monitor.id, picked);
                }
                ui.label(egui::RichText::new((i + 1).to_string()).strong().size(16.0));
                let mut label = monitor.label();
                if monitor.primary {
                    label.push_str("  · main");
                }
                ui.label(label);
                let known = self.settings.known.entry(monitor.id.clone()).or_default();
                ui.add_sized([140.0, 20.0], egui::TextEdit::singleline(&mut known.nickname).hint_text("Nickname"));
                ui.label(match dimmable.get(&monitor.id) {
                    None => "checking…",
                    Some(true) => "dims",
                    Some(false) => "can't dim",
                })
                .on_hover_text("Whether the backlight can be turned down over DDC/CI");
                ui.end_row();
            }
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("Identify").on_hover_text("Show each monitor's number on its screen").clicked() {
                let badges: Vec<_> = self
                    .monitors
                    .iter()
                    .enumerate()
                    .map(|(i, m)| (m.rect, (i + 1).to_string(), self.settings.name_for(m)))
                    .collect();
                overlay::identify(&badges);
            }
            let picked = self.monitors.iter().filter(|m| self.settings.is_target(&m.id)).count();
            let total = self.monitors.len();
            if picked == 0 {
                ui.colored_label(ui.visuals().warn_fg_color, "No connected monitor is picked, so a blackout does nothing.");
            } else if picked == total {
                ui.label("Every monitor is picked: a full blackout.");
            } else {
                ui.label(format!("{picked} of {total} picked: a partial blackout; the others stay on."));
            }
        });

        self.offline_monitors(ui);
    }

    /// Monitors seen before but not connected now, so their choice can still be changed or forgotten.
    fn offline_monitors(&mut self, ui: &mut egui::Ui) {
        let offline: Vec<(String, String)> = self
            .settings
            .known
            .iter()
            .filter(|(id, _)| !self.monitors.iter().any(|m| &m.id == *id))
            .map(|(id, known)| {
                let name = if known.nickname.trim().is_empty() { known.label.clone() } else { known.nickname.clone() };
                (id.clone(), name)
            })
            .collect();
        if offline.is_empty() {
            return;
        }
        ui.collapsing(format!("Not connected right now ({})", offline.len()), |ui| {
            for (id, name) in offline {
                ui.horizontal(|ui| {
                    let mut picked = self.settings.is_target(&id);
                    let checkbox = ui.add_enabled(!self.settings.all_monitors, egui::Checkbox::new(&mut picked, name));
                    if checkbox.changed() {
                        self.settings.set_selected(&id, picked);
                    }
                    if ui.small_button("Forget").clicked() {
                        self.settings.known.remove(&id);
                        self.settings.set_selected(&id, false);
                    }
                });
            }
        });
    }

    fn hotkey_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Hotkey");
        ui.horizontal(|ui| {
            let text = if self.capturing_hotkey { "Press the new hotkey…".to_string() } else { self.settings.hotkey.clone() };
            if ui.add(egui::Button::new(text).selected(self.capturing_hotkey)).clicked() && !self.capturing_hotkey {
                self.capturing_hotkey = true;
                self.hotkey_error = None;
                ipc::post(ipc::WM_SUSPEND_HOTKEY, 1);
            }
            ui.label(if self.capturing_hotkey { "Esc cancels" } else { "Click to change" });
        });
        if let Some(error) = &self.hotkey_error {
            ui.colored_label(ui.visuals().warn_fg_color, error);
        }
        if self.capturing_hotkey {
            self.capture_hotkey(ui);
        }
    }

    fn capture_hotkey(&mut self, ui: &egui::Ui) {
        for event in ui.input(|input| input.events.clone()) {
            let egui::Event::Key { key, pressed: true, modifiers, .. } = event else { continue };
            if key == egui::Key::Escape {
                self.end_capture();
                return;
            }
            let mut text = String::new();
            for (held, name) in [(modifiers.ctrl, "Ctrl+"), (modifiers.alt, "Alt+"), (modifiers.shift, "Shift+")] {
                if held {
                    text.push_str(name);
                }
            }
            text.push_str(key.name());
            match Hotkey::parse(&text) {
                Ok(hotkey) => {
                    self.settings.hotkey = hotkey.to_string();
                    self.save();
                    self.end_capture();
                    return;
                }
                Err(e) => self.hotkey_error = Some(format!("{text} won't work: {e}")),
            }
        }
    }

    fn end_capture(&mut self) {
        self.capturing_hotkey = false;
        ipc::post(ipc::WM_SUSPEND_HOTKEY, 0);
    }

    fn full_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Full blackout");
        ui.label("Every monitor goes black, like for an overnight run.");
        ui.horizontal(|ui| {
            ui.label("Wake by tapping Enter");
            ui.add(egui::DragValue::new(&mut self.settings.wake_taps).range(2..=5));
            ui.label("times within");
            ui.add(egui::DragValue::new(&mut self.settings.wake_window_secs).range(1.0..=5.0).speed(0.1).suffix(" s"));
        });
        ui.checkbox(&mut self.settings.keep_awake_full, "Keep the PC and displays awake");
    }

    fn partial_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Partial blackout");
        ui.label("Some monitors stay on. Enter isn't captured, so you can keep typing; the hotkey or the tray icon wakes it.");
        ui.checkbox(&mut self.settings.keep_awake_partial, "Keep the PC and displays awake")
            .on_hover_text("Off: your normal sleep and screen-off settings still apply");
    }

    fn general_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Both");
        ui.checkbox(&mut self.settings.dim_backlight, "Turn the backlight down while blacked out (DDC/CI)");
        let mut autostart = self.autostart;
        if ui.checkbox(&mut autostart, "Start Blackout when I sign in").changed() && autostart::set(autostart) {
            self.autostart = autostart;
        }
    }
}

impl eframe::App for SettingsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_engine(&ctx);
        let before = self.settings.clone();
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.status_section(ui);
                ui.separator();
                self.monitors_section(ui);
                ui.separator();
                self.hotkey_section(ui);
                ui.separator();
                self.full_section(ui);
                ui.separator();
                self.partial_section(ui);
                ui.separator();
                self.general_section(ui);
            });
        });
        if self.settings != before {
            self.dirty_since = Some(Instant::now());
        }
        self.save_when_settled(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.capturing_hotkey {
            self.end_capture();
        }
        if self.dirty_since.is_some() {
            self.save();
        }
    }
}

fn start_resident() {
    let spawned = std::env::current_exe().and_then(|exe| std::process::Command::new(exe).arg("--resident").spawn());
    if let Err(e) = spawned {
        logfile::write(&format!("can't start Blackout: {e}"));
    }
}
