//! "Start at sign-in": a value under HKCU\...\Run, which also shows in Task Manager's Startup apps.

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::{PCWSTR, w};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const VALUE: PCWSTR = w!("Blackout");

pub fn is_enabled() -> bool {
    unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE, RRF_RT_REG_SZ, None, None, None) == ERROR_SUCCESS }
}

/// Turns start-at-sign-in on (pointing at this .exe) or off; false if the registry refused.
pub fn set(enabled: bool) -> bool {
    unsafe {
        if !enabled {
            let status = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE);
            return status == ERROR_SUCCESS || !is_enabled();
        }
        let Ok(exe) = std::env::current_exe() else { return false };
        let command: Vec<u16> =
            format!("\"{}\" --resident", exe.display()).encode_utf16().chain(std::iter::once(0)).collect();
        let status = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE,
            REG_SZ.0,
            Some(command.as_ptr().cast()),
            (command.len() * 2) as u32,
        );
        status == ERROR_SUCCESS
    }
}
