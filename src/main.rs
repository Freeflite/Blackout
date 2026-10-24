//! Blackout: blacks out some or all monitors while the PC and GPU keep working.
//!
//! `blackout`             black out now, or toggle the copy already running in the tray
//! `blackout --resident`  start in the tray without blacking out (used at sign-in)
//! `blackout --settings`  open the settings window
//! `blackout --exit`      wake and quit the copy in the tray
#![windows_subsystem = "windows"]

mod autostart;
mod chrome;
mod ddc;
mod edid;
mod engine;
mod hotkey;
mod icon_art;
mod ipc;
mod logfile;
mod monitors;
mod overlay;
mod paths;
mod settings;
mod state;
mod taps;
mod ui;

fn main() {
    std::panic::set_hook(Box::new(|info| logfile::write(&format!("panic: {info}"))));
    match std::env::args().nth(1).as_deref() {
        None => engine::run(true),
        Some("--resident") => engine::run(false),
        Some("--settings") => ui::run(),
        Some("--exit") => {
            ipc::post(ipc::WM_EXIT, 0);
        }
        Some(other) => logfile::write(&format!("unknown argument {other:?}")),
    }
}
