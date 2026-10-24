//! The black windows that cover monitors, and the big numbers that Identify shows.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::sync::Once;

use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BLACK_BRUSH, BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, CreateSolidBrush, DEFAULT_CHARSET,
    DT_CENTER, DT_END_ELLIPSIS, DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawTextW, EndPaint, FW_SEMIBOLD, FillRect,
    GetStockObject, HBRUSH, HDC, OUT_DEFAULT_PRECIS, PAINTSTRUCT, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetClientRect, GetWindowLongPtrW,
    IDC_ARROW, LoadCursorW, MA_NOACTIVATE, RegisterClassW, SW_SHOW, SW_SHOWNOACTIVATE, SetCursor, SetForegroundWindow,
    SetTimer, SetWindowLongPtrW, ShowWindow, WM_CLOSE, WM_MOUSEACTIVATE, WM_NCCREATE, WM_NCDESTROY, WM_PAINT,
    WM_SETCURSOR, WM_TIMER, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, w};

const OVERLAY_CLASS: PCWSTR = w!("Blackout.Overlay");
const IDENTIFY_CLASS: PCWSTR = w!("Blackout.Identify");
const IDENTIFY_MS: u32 = 3000;

thread_local! {
    /// Full blackout: no pointer over the overlays. Partial: a normal arrow, and they never take focus.
    static FULL: Cell<bool> = const { Cell::new(true) };
    static BADGES: RefCell<Vec<isize>> = const { RefCell::new(Vec::new()) };
}

fn instance() -> HINSTANCE {
    unsafe { GetModuleHandleW(None) }.map(|module| HINSTANCE(module.0)).unwrap_or_default()
}

fn register_classes() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(overlay_proc),
            hInstance: instance(),
            lpszClassName: OVERLAY_CLASS,
            hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
            ..Default::default()
        });
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(badge_proc),
            hInstance: instance(),
            lpszClassName: IDENTIFY_CLASS,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        });
    });
}

/// A black, topmost window over one monitor; destroyed on drop.
pub struct Overlay(HWND);

impl Overlay {
    /// A `full` blackout hides the pointer and takes focus; a partial one leaves both alone.
    pub fn create(rect: RECT, full: bool) -> Option<Self> {
        register_classes();
        FULL.set(full);
        let mut ex_style = WS_EX_TOPMOST | WS_EX_TOOLWINDOW;
        if !full {
            ex_style |= WS_EX_NOACTIVATE;
        }
        let hwnd = unsafe {
            CreateWindowExW(
                ex_style,
                OVERLAY_CLASS,
                w!("Blackout"),
                WS_POPUP,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                None,
                None,
                Some(instance()),
                None,
            )
        }
        .ok()?;
        unsafe {
            let _ = ShowWindow(hwnd, if full { SW_SHOW } else { SW_SHOWNOACTIVATE });
        }
        Some(Self(hwnd))
    }

    /// Takes the foreground so stray keys land here, and hides the pointer straight away.
    pub fn focus(&self) {
        unsafe {
            let _ = SetForegroundWindow(self.0);
            SetCursor(None);
        }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_SETCURSOR => {
                SetCursor(if FULL.get() { None } else { LoadCursorW(None, IDC_ARROW).ok() });
                LRESULT(1)
            }
            // Alt+F4 lands here; overlays only go away when the blackout ends.
            WM_CLOSE => LRESULT(0),
            WM_MOUSEACTIVATE if !FULL.get() => LRESULT(MA_NOACTIVATE as isize),
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

struct Badge {
    number: String,
    name: String,
}

/// Shows a big number and a name in the middle of each monitor for a few seconds.
/// Each entry is (monitor bounds, number, name).
pub fn identify(monitors: &[(RECT, String, String)]) {
    register_classes();
    for hwnd in BADGES.take() {
        unsafe {
            let _ = DestroyWindow(HWND(hwnd as _));
        }
    }
    for (rect, number, name) in monitors {
        let height = (rect.bottom - rect.top) * 3 / 10;
        let width = height * 8 / 5;
        let x = rect.left + (rect.right - rect.left - width) / 2;
        let y = rect.top + (rect.bottom - rect.top - height) / 2;
        // Owned by the window from here on; badge_proc frees it in WM_NCDESTROY.
        let badge = Box::into_raw(Box::new(Badge { number: number.clone(), name: name.clone() }));
        let created = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                IDENTIFY_CLASS,
                w!("Blackout identify"),
                WS_POPUP,
                x,
                y,
                width,
                height,
                None,
                None,
                Some(instance()),
                Some(badge as *const c_void),
            )
        };
        if let Ok(hwnd) = created {
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                SetTimer(Some(hwnd), 1, IDENTIFY_MS, None);
            }
            BADGES.with_borrow_mut(|badges| badges.push(hwnd.0 as isize));
        }
    }
}

unsafe extern "system" fn badge_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_NCCREATE => {
                let create = &*(lparam.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_PAINT => {
                paint_badge(hwnd);
                LRESULT(0)
            }
            WM_TIMER => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_NCDESTROY => {
                let badge = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) as *mut Badge;
                if !badge.is_null() {
                    drop(Box::from_raw(badge));
                }
                BADGES.with_borrow_mut(|badges| badges.retain(|&h| h != hwnd.0 as isize));
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe fn paint_badge(hwnd: HWND) {
    unsafe {
        let mut paint = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut paint);
        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let background = CreateSolidBrush(COLORREF(0x0020_2020));
        FillRect(hdc, &rect, background);
        let _ = DeleteObject(background.into());

        let badge = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Badge;
        if let Some(badge) = badge.as_ref() {
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, COLORREF(0x00FF_FFFF));
            let height = rect.bottom - rect.top;
            let split = rect.top + height * 7 / 10;
            draw_text(hdc, &badge.number, RECT { bottom: split, ..rect }, height * 11 / 20);
            draw_text(hdc, &badge.name, RECT { top: split - height / 20, bottom: rect.bottom - height / 20, ..rect }, height / 9);
        }
        let _ = EndPaint(hwnd, &paint);
    }
}

unsafe fn draw_text(hdc: HDC, text: &str, mut rect: RECT, pixel_height: i32) {
    unsafe {
        let font = CreateFontW(
            -pixel_height,
            0,
            0,
            0,
            FW_SEMIBOLD.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            0,
            w!("Segoe UI"),
        );
        let previous = SelectObject(hdc, font.into());
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(hdc, &mut wide, &mut rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
        SelectObject(hdc, previous);
        let _ = DeleteObject(font.into());
    }
}
