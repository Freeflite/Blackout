//! The resident copy: a hidden window that owns the hotkeys and the tray icon, and runs the blackouts.
//!
//! A full blackout (every monitor) hides the pointer, grabs Enter for the triple-tap wake and, if the
//! settings say so, keeps the PC awake. A partial blackout leaves the pointer and Enter alone, since
//! you're still working on the other monitors; only the hotkey or the tray icon ends it.

use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering};

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, SetThreadExecutionState};
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::Win32::UI::Input::KeyboardAndMouse::{MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey, VK_RETURN};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, GetSystemMetrics, KillTimer, MSG,
    PostMessageW, PostQuitMessage, RegisterClassW, SM_CXSMICON, SetTimer, SetWindowTextW, TranslateMessage,
    WM_DESTROY, WM_DISPLAYCHANGE, WM_ENDSESSION, WM_HOTKEY, WM_TIMER, WNDCLASSW, WS_EX_TOOLWINDOW,
    WS_OVERLAPPEDWINDOW,
};
use windows::core::{HSTRING, w};

use crate::hotkey::Hotkey;
use crate::monitors::{self, Monitor};
use crate::overlay::Overlay;
use crate::settings::Settings;
use crate::taps::TapCounter;
use crate::{ddc, icon_art, ipc, logfile, state};

const HOTKEY_TOGGLE: i32 = 1;
const HOTKEY_ENTER: i32 = 2;
const TIMER_DISPLAY_CHANGED: usize = 1;
const TIMER_RESUME_HOTKEY: usize = 2;
/// Monitors come and go in bursts; wait this long for things to settle before redoing a blackout.
const DISPLAY_SETTLE_MS: u32 = 1500;
/// The settings window gets this long to capture a new hotkey before ours comes back regardless.
const HOTKEY_SUSPEND_MS: u32 = 30_000;

const MENU_TOGGLE: &str = "toggle";
const MENU_SETTINGS: &str = "settings";
const MENU_EXIT: &str = "exit";

thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
}
/// The engine window, for the tray and menu callbacks, which must be Send (HWND isn't).
static ENGINE_HWND: AtomicIsize = AtomicIsize::new(0);

struct Blackout {
    overlays: Vec<Overlay>,
    dimmed: Vec<Dimmed>,
}

struct Dimmed {
    key: String,
    monitor: ddc::Physical,
    original: u32,
}

struct Engine {
    hwnd: HWND,
    settings: Settings,
    /// The toggle hotkey as registered; None while it's unavailable or suspended.
    hotkey: Option<Hotkey>,
    hotkey_suspended: bool,
    taps: TapCounter,
    active: Option<Blackout>,
    tray: Option<TrayIcon>,
    toggle_item: MenuItem,
}

/// Runs the resident copy until it's told to exit. With `blackout_now` it blacks out straight away,
/// and if a copy is already running, that copy toggles instead.
pub fn run(blackout_now: bool) {
    let Some(_instance) = ipc::SingleInstance::acquire(w!("Local\\Blackout.Engine")) else {
        if blackout_now {
            ipc::post_patiently(ipc::WM_TOGGLE);
        }
        return;
    };
    // The manifest asks for this already; overlay bounds must be real pixels.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let hwnd = match create_window() {
        Ok(hwnd) => hwnd,
        Err(e) => {
            logfile::write(&format!("can't create the engine window: {e}"));
            return;
        }
    };
    ENGINE_HWND.store(hwnd.0 as isize, Ordering::Relaxed);

    restore_leftover_brightness();
    let settings = Settings::load();
    let mut engine = Engine {
        hwnd,
        taps: TapCounter::new(settings.wake_taps(), settings.wake_window()),
        settings,
        hotkey: None,
        hotkey_suspended: false,
        active: None,
        tray: None,
        toggle_item: MenuItem::with_id(MENU_TOGGLE, "Black out now", true, None),
    };
    engine.register_toggle_hotkey();
    engine.tray = engine.build_tray();
    engine.update_tray();
    ENGINE.with_borrow_mut(|slot| *slot = Some(engine));
    logfile::write("started");

    if blackout_now {
        post_self(ipc::WM_TOGGLE);
    }
    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    // Wake (restoring brightness) before the tray icon goes away with the engine.
    if let Some(mut engine) = ENGINE.with_borrow_mut(Option::take) {
        engine.stop();
    }
    logfile::write("exited");
}

fn create_window() -> windows::core::Result<HWND> {
    unsafe {
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(engine_proc),
            hInstance: instance,
            lpszClassName: ipc::ENGINE_CLASS,
            ..Default::default()
        });
        // A top-level window (never shown) so it hears WM_DISPLAYCHANGE and WM_ENDSESSION; it has a
        // caption so other processes can read the status in its title.
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            ipc::ENGINE_CLASS,
            w!("Blackout engine"),
            WS_OVERLAPPEDWINDOW,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
    }
}

fn post_self(message: u32) {
    let hwnd = HWND(ENGINE_HWND.load(Ordering::Relaxed) as _);
    unsafe {
        let _ = PostMessageW(Some(hwnd), message, WPARAM(0), LPARAM(0));
    }
}

unsafe extern "system" fn engine_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_HOTKEY
        | WM_TIMER
        | WM_DISPLAYCHANGE
        | ipc::WM_TOGGLE
        | ipc::WM_RELOAD
        | ipc::WM_EXIT
        | ipc::WM_SUSPEND_HOTKEY
        | ipc::WM_OPEN_SETTINGS => {
            let handled = ENGINE.with(|cell| match cell.try_borrow_mut() {
                Ok(mut engine) => {
                    if let Some(engine) = engine.as_mut() {
                        engine.handle(msg, wparam);
                    }
                    true
                }
                Err(_) => false,
            });
            // It arrived while the engine was mid-call (a nested message loop); take it again later.
            if !handled {
                unsafe {
                    let _ = PostMessageW(Some(hwnd), msg, wparam, lparam);
                }
            }
            LRESULT(0)
        }
        WM_ENDSESSION if wparam.0 != 0 => {
            // Signing out or shutting down: put the brightness back while there's still time.
            ENGINE.with(|cell| {
                if let Ok(mut engine) = cell.try_borrow_mut()
                    && let Some(engine) = engine.as_mut()
                {
                    engine.stop();
                }
            });
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

impl Engine {
    fn handle(&mut self, msg: u32, wparam: WPARAM) {
        match msg {
            WM_HOTKEY if wparam.0 as i32 == HOTKEY_TOGGLE => self.toggle(),
            WM_HOTKEY if wparam.0 as i32 == HOTKEY_ENTER => {
                if self.taps.tap() {
                    self.stop();
                }
            }
            ipc::WM_TOGGLE => self.toggle(),
            ipc::WM_RELOAD => self.reload(),
            ipc::WM_SUSPEND_HOTKEY if wparam.0 != 0 => self.suspend_hotkey(),
            ipc::WM_SUSPEND_HOTKEY => self.resume_hotkey(),
            ipc::WM_OPEN_SETTINGS => open_settings(),
            ipc::WM_EXIT => {
                self.stop();
                unsafe {
                    let _ = DestroyWindow(self.hwnd);
                }
            }
            WM_DISPLAYCHANGE if self.active.is_some() => unsafe {
                SetTimer(Some(self.hwnd), TIMER_DISPLAY_CHANGED, DISPLAY_SETTLE_MS, None);
            },
            WM_TIMER if wparam.0 == TIMER_DISPLAY_CHANGED => {
                unsafe {
                    let _ = KillTimer(Some(self.hwnd), TIMER_DISPLAY_CHANGED);
                }
                // Redo the blackout so the overlays fit the monitors as they are now.
                if self.active.is_some() {
                    self.stop();
                    self.start();
                }
            }
            WM_TIMER if wparam.0 == TIMER_RESUME_HOTKEY => self.resume_hotkey(),
            _ => {}
        }
    }

    fn toggle(&mut self) {
        if self.active.is_some() { self.stop() } else { self.start() }
    }

    fn start(&mut self) {
        if self.active.is_some() {
            return;
        }
        let monitors = monitors::enumerate();
        let targets: Vec<&Monitor> = monitors.iter().filter(|m| self.settings.is_target(&m.id)).collect();
        if targets.is_empty() {
            logfile::write("nothing to black out: none of the chosen monitors is connected");
            return;
        }
        let full = targets.len() == monitors.len();

        let overlays: Vec<Overlay> = targets.iter().filter_map(|m| Overlay::create(m.rect, full)).collect();
        if full && let Some(first) = overlays.first() {
            first.focus();
        }

        let keep_awake = if full { self.settings.keep_awake_full } else { self.settings.keep_awake_partial };
        if keep_awake {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
            }
        }

        // Only a full blackout grabs Enter: with a monitor still on, you may be typing on it.
        if full {
            self.taps = TapCounter::new(self.settings.wake_taps(), self.settings.wake_window());
            if let Err(e) = unsafe { RegisterHotKey(Some(self.hwnd), HOTKEY_ENTER, MOD_NOREPEAT, VK_RETURN.0 as u32) } {
                logfile::write(&format!("can't grab Enter, so only the hotkey wakes this blackout: {e}"));
            }
        }

        let dimmed = if self.settings.dim_backlight { dim(&targets) } else { Vec::new() };
        logfile::write(&format!(
            "blackout on: {} of {} monitors, {} dimmed, keep awake {keep_awake}",
            targets.len(),
            monitors.len(),
            dimmed.len()
        ));
        self.active = Some(Blackout { overlays, dimmed });
        self.update_tray();
    }

    fn stop(&mut self) {
        let Some(blackout) = self.active.take() else { return };
        unsafe {
            let _ = UnregisterHotKey(Some(self.hwnd), HOTKEY_ENTER);
        }
        // Brightness first, while the screens are still black.
        let mut saved = state::load();
        for dimmed in &blackout.dimmed {
            if dimmed.monitor.set_brightness(dimmed.original) {
                saved.remove(&dimmed.key);
            } else {
                logfile::write(&format!("couldn't restore {}; will try again next start", dimmed.key));
            }
        }
        state::save(&saved);
        drop(blackout.overlays);
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS);
        }
        logfile::write("blackout off");
        self.update_tray();
    }

    fn reload(&mut self) {
        let settings = Settings::load();
        let redo = self.active.is_some() && self.settings.blackout_differs(&settings);
        let hotkey_changed = settings.hotkey != self.settings.hotkey;
        self.settings = settings;
        self.taps = TapCounter::new(self.settings.wake_taps(), self.settings.wake_window());
        if !self.hotkey_suspended && (hotkey_changed || self.hotkey.is_none()) {
            self.register_toggle_hotkey();
        }
        if redo {
            self.stop();
            self.start();
        }
        self.update_tray();
    }

    fn register_toggle_hotkey(&mut self) {
        unsafe {
            let _ = UnregisterHotKey(Some(self.hwnd), HOTKEY_TOGGLE);
        }
        self.hotkey = match Hotkey::parse(&self.settings.hotkey) {
            Ok(hotkey) => match unsafe { RegisterHotKey(Some(self.hwnd), HOTKEY_TOGGLE, hotkey.modifiers(), hotkey.vk) } {
                Ok(()) => Some(hotkey),
                Err(e) => {
                    logfile::write(&format!("{hotkey} is taken by another app: {e}"));
                    None
                }
            },
            Err(e) => {
                logfile::write(&format!("hotkey \"{}\" can't be used: {e}", self.settings.hotkey));
                None
            }
        };
        self.publish_status();
    }

    /// Lets the settings window capture a new hotkey, which would otherwise come to us.
    fn suspend_hotkey(&mut self) {
        unsafe {
            let _ = UnregisterHotKey(Some(self.hwnd), HOTKEY_TOGGLE);
            SetTimer(Some(self.hwnd), TIMER_RESUME_HOTKEY, HOTKEY_SUSPEND_MS, None);
        }
        self.hotkey = None;
        self.hotkey_suspended = true;
        self.publish_status();
    }

    fn resume_hotkey(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_RESUME_HOTKEY);
        }
        self.hotkey_suspended = false;
        self.register_toggle_hotkey();
        self.update_tray();
    }

    fn publish_status(&self) {
        let status = match (self.hotkey_suspended, &self.hotkey) {
            (true, _) => ipc::HotkeyStatus::Suspended,
            (false, Some(_)) => ipc::HotkeyStatus::Active,
            (false, None) => ipc::HotkeyStatus::Unavailable,
        };
        unsafe {
            let _ = SetWindowTextW(self.hwnd, &HSTRING::from(status.title()));
        }
    }

    fn build_tray(&self) -> Option<TrayIcon> {
        let menu = Menu::new();
        let settings = MenuItem::with_id(MENU_SETTINGS, "Settings…", true, None);
        let exit = MenuItem::with_id(MENU_EXIT, "Exit", true, None);
        let separator = PredefinedMenuItem::separator();
        let _ = menu.append_items(&[&self.toggle_item, &separator, &settings, &exit]);

        MenuEvent::set_event_handler(Some(|event: MenuEvent| {
            let message = match event.id.0.as_str() {
                MENU_TOGGLE => ipc::WM_TOGGLE,
                MENU_SETTINGS => ipc::WM_OPEN_SETTINGS,
                MENU_EXIT => ipc::WM_EXIT,
                _ => return,
            };
            post_self(message);
        }));
        TrayIconEvent::set_event_handler(Some(|event: TrayIconEvent| {
            if let TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } = event {
                post_self(ipc::WM_OPEN_SETTINGS);
            }
        }));

        // The process is DPI aware, so this is the tray icon size in real pixels.
        let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.clamp(16, 64) as u32;
        let icon = Icon::from_rgba(icon_art::icon_rgba(size), size, size).ok()?;
        let built = TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip("Blackout")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build();
        match built {
            Ok(tray) => Some(tray),
            Err(e) => {
                logfile::write(&format!("can't add the tray icon: {e}"));
                None
            }
        }
    }

    fn update_tray(&self) {
        self.toggle_item.set_text(if self.active.is_some() { "Wake now" } else { "Black out now" });
        let tooltip = match (&self.active, &self.hotkey) {
            (Some(_), Some(hotkey)) => format!("Blackout is on ({hotkey} wakes)"),
            (Some(_), None) => "Blackout is on".to_string(),
            (None, Some(hotkey)) => format!("Blackout ({hotkey})"),
            (None, None) if self.hotkey_suspended => "Blackout".to_string(),
            (None, None) => format!("Blackout ({} is unavailable)", self.settings.hotkey),
        };
        if let Some(tray) = &self.tray {
            let _ = tray.set_tooltip(Some(tooltip));
        }
    }
}

/// Turns the targets' backlights down, after writing their original levels to disk.
fn dim(targets: &[&Monitor]) -> Vec<Dimmed> {
    // Leftovers from a blackout that never woke are the real originals, not today's dimmed reading.
    let mut saved = state::load();
    let mut dimmed = Vec::new();
    for monitor in targets {
        for physical in ddc::physical_monitors(monitor.handle()) {
            let key = state::key(&monitor.id, physical.index);
            let Some(current) = physical.brightness() else { continue };
            let original = *saved.entry(key.clone()).or_insert(current);
            dimmed.push(Dimmed { key, monitor: physical, original });
        }
    }
    state::save(&saved);
    for d in &dimmed {
        if !d.monitor.set_brightness(0) {
            logfile::write(&format!("couldn't dim {}", d.key));
        }
    }
    dimmed
}

/// Puts back levels from a blackout that never woke (killed, or the PC restarted mid-blackout).
fn restore_leftover_brightness() {
    let mut saved = state::load();
    if saved.is_empty() {
        return;
    }
    for monitor in monitors::enumerate() {
        for physical in ddc::physical_monitors(monitor.handle()) {
            let key = state::key(&monitor.id, physical.index);
            if let Some(&level) = saved.get(&key)
                && physical.set_brightness(level)
            {
                saved.remove(&key);
                logfile::write(&format!("restored leftover brightness for {key}"));
            }
        }
    }
    // Anything left (unplugged, or not answering yet) waits for the next start.
    state::save(&saved);
}

fn open_settings() {
    let spawned = std::env::current_exe().and_then(|exe| std::process::Command::new(exe).arg("--settings").spawn());
    if let Err(e) = spawned {
        logfile::write(&format!("can't open settings: {e}"));
    }
}
