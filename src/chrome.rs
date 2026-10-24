//! The settings window's title bar: the FreeFlite icon at sharp sizes, and colors that follow the theme.

use std::ffi::c_void;
use std::mem::size_of;

use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWINDOWATTRIBUTE, DwmSetWindowAttribute,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW, SM_CXICON, SM_CXSMICON,
    SendMessageW, WM_SETICON,
};
use windows::core::{BOOL, PCWSTR};

/// build.rs embeds the app icon as resource ID 1.
const ICON_RESOURCE: PCWSTR = PCWSTR(1 as _);

/// Loads the .exe's own icon at exactly the sizes Windows draws, so the title bar and taskbar show it
/// sharp instead of scaled from a single bitmap.
pub fn set_icons(hwnd: HWND) {
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else { return };
        for (slot, metric) in [(ICON_SMALL, SM_CXSMICON), (ICON_BIG, SM_CXICON)] {
            let size = GetSystemMetrics(metric);
            let icon = LoadImageW(Some(HINSTANCE(module.0)), ICON_RESOURCE, IMAGE_ICON, size, size, LR_DEFAULTCOLOR);
            if let Ok(icon) = icon {
                SendMessageW(hwnd, WM_SETICON, Some(WPARAM(slot as usize)), Some(LPARAM(icon.0 as isize)));
            }
        }
    }
}

/// Paints the title bar in the app's own background and text colors, with light or dark caption
/// buttons to match. Windows 10 ignores the colors and just switches between light and dark.
pub fn color_title_bar(hwnd: HWND, dark: bool, background: [u8; 3], text: [u8; 3]) {
    let colorref = |[r, g, b]: [u8; 3]| COLORREF(u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16);
    set_attribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &BOOL::from(dark));
    set_attribute(hwnd, DWMWA_CAPTION_COLOR, &colorref(background));
    set_attribute(hwnd, DWMWA_TEXT_COLOR, &colorref(text));
}

fn set_attribute<T>(hwnd: HWND, attribute: DWMWINDOWATTRIBUTE, value: &T) {
    unsafe {
        let _ = DwmSetWindowAttribute(hwnd, attribute, value as *const T as *const c_void, size_of::<T>() as u32);
    }
}
