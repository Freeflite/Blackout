//! How copies of Blackout talk to each other. A named mutex keeps one resident copy; other copies
//! (a second launch, the settings window) find its hidden window by class name and post it commands.

use std::thread::sleep;
use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowTextW, PostMessageW, WM_APP};
use windows::core::{PCWSTR, w};

pub const ENGINE_CLASS: PCWSTR = w!("Blackout.Engine");

pub const WM_TOGGLE: u32 = WM_APP + 1;
pub const WM_RELOAD: u32 = WM_APP + 2;
pub const WM_EXIT: u32 = WM_APP + 3;
/// wParam 1 suspends the toggle hotkey so the settings window can capture a new one; 0 resumes it.
pub const WM_SUSPEND_HOTKEY: u32 = WM_APP + 4;
pub const WM_OPEN_SETTINGS: u32 = WM_APP + 5;

/// The resident copy's hotkey state, published as its (never shown) window title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyStatus {
    Active,
    Unavailable,
    Suspended,
}

impl HotkeyStatus {
    const ALL: [HotkeyStatus; 3] = [HotkeyStatus::Active, HotkeyStatus::Unavailable, HotkeyStatus::Suspended];

    pub fn title(self) -> &'static str {
        match self {
            HotkeyStatus::Active => "Blackout engine: hotkey active",
            HotkeyStatus::Unavailable => "Blackout engine: hotkey unavailable",
            HotkeyStatus::Suspended => "Blackout engine: hotkey suspended",
        }
    }
}

fn engine_window() -> Option<HWND> {
    unsafe { FindWindowW(ENGINE_CLASS, PCWSTR::null()) }.ok()
}

/// The resident copy's hotkey state, or None if it isn't running.
pub fn engine_status() -> Option<HotkeyStatus> {
    let hwnd = engine_window()?;
    let mut title = [0u16; 64];
    let len = unsafe { GetWindowTextW(hwnd, &mut title) }.max(0) as usize;
    let title = String::from_utf16_lossy(&title[..len]);
    // Still starting up if it hasn't published a status yet.
    Some(HotkeyStatus::ALL.into_iter().find(|s| s.title() == title).unwrap_or(HotkeyStatus::Active))
}

/// Posts a command to the resident copy; false if none is running.
pub fn post(message: u32, wparam: usize) -> bool {
    engine_window().is_some_and(|hwnd| unsafe { PostMessageW(Some(hwnd), message, WPARAM(wparam), LPARAM(0)) }.is_ok())
}

/// Like `post`, but gives a copy that's still starting up a few seconds to create its window.
pub fn post_patiently(message: u32) -> bool {
    for _ in 0..30 {
        if post(message, 0) {
            return true;
        }
        sleep(Duration::from_millis(100));
    }
    false
}

/// Holds a named mutex for the life of the process, so only one copy of something runs.
pub struct SingleInstance(HANDLE);

impl SingleInstance {
    /// None if another process already holds `name`.
    pub fn acquire(name: PCWSTR) -> Option<Self> {
        unsafe {
            let handle = CreateMutexW(None, true, name).ok()?;
            if GetLastError() == ERROR_ALREADY_EXISTS {
                let _ = CloseHandle(handle);
                return None;
            }
            Some(Self(handle))
        }
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
