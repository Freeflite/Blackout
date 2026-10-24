//! Connected monitors, each with an ID that survives reboots, driver reinstalls and port swaps.
//!
//! The ID comes from the monitor's own EDID (manufacturer + product code + serial), so two monitors of
//! the same model still differ. A monitor without a usable serial falls back to Windows' device
//! instance path, which is tied to the port it's plugged into.

use std::collections::HashMap;
use std::mem::size_of;

use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, DisplayConfigGetDeviceInfo,
    GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
};
use windows::Win32::Foundation::{ERROR_SUCCESS, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW};
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_BINARY, RegGetValueW};
use windows::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;
use windows::core::{BOOL, HSTRING, w};

use crate::edid;

#[derive(Clone, Debug)]
pub struct Monitor {
    /// Stable identity; the key used in settings.
    pub id: String,
    /// Model name the monitor reports, e.g. "LG ULTRAGEAR".
    pub model: String,
    pub serial: Option<String>,
    /// Bounds in physical pixels (the process is per-monitor DPI aware).
    pub rect: RECT,
    pub primary: bool,
    hmonitor: isize,
}

impl Monitor {
    pub fn handle(&self) -> HMONITOR {
        HMONITOR(self.hmonitor as _)
    }

    /// Model plus the end of the serial: enough to tell two identical monitors apart.
    pub fn label(&self) -> String {
        match &self.serial {
            Some(serial) if serial.len() > 5 => format!("{} (…{})", self.model, &serial[serial.len() - 5..]),
            Some(serial) => format!("{} ({serial})", self.model),
            None => self.model.clone(),
        }
    }
}

/// Connected monitors, left to right.
pub fn enumerate() -> Vec<Monitor> {
    let targets = display_targets();
    let mut found: Vec<(Monitor, String)> = gdi_monitors()
        .into_iter()
        .map(|(hmonitor, rect, primary, gdi_name)| {
            let target = targets.get(&gdi_name);
            let instance = target.and_then(|t| instance_id(&t.device_path));
            let edid = instance.as_deref().and_then(read_edid).and_then(|bytes| edid::parse(&bytes));
            let model = target
                .map(|t| t.friendly_name.clone())
                .filter(|name| !name.is_empty())
                .or_else(|| edid.as_ref().and_then(|e| e.name.clone()))
                .unwrap_or_else(|| "Monitor".to_string());
            let serial = edid.as_ref().and_then(|e| e.serial.clone());
            let port = instance.unwrap_or(gdi_name);
            let id = match (&edid, &serial) {
                (Some(edid), Some(serial)) => format!("{}-{serial}", edid.model_id()),
                _ => port.clone(),
            };
            let monitor = Monitor { id, model, serial, rect, primary, hmonitor: hmonitor.0 as isize };
            (monitor, port)
        })
        .collect();
    make_ids_unique(&mut found);
    let mut monitors: Vec<Monitor> = found.into_iter().map(|(monitor, _)| monitor).collect();
    monitors.sort_by_key(|m| (m.rect.left, m.rect.top));
    monitors
}

/// Two monitors claiming the same EDID serial (cheap panels do this) get told apart by port.
fn make_ids_unique(found: &mut [(Monitor, String)]) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (monitor, _) in found.iter() {
        *counts.entry(monitor.id.clone()).or_default() += 1;
    }
    for (monitor, port) in found.iter_mut() {
        if counts[&monitor.id] > 1 {
            monitor.id = format!("{}@{port}", monitor.id);
        }
    }
}

/// (handle, bounds, primary, GDI name such as \\.\DISPLAY7) for each monitor on the desktop.
fn gdi_monitors() -> Vec<(HMONITOR, RECT, bool, String)> {
    unsafe extern "system" fn collect(hmonitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let found = unsafe { &mut *(data.0 as *mut Vec<(HMONITOR, RECT, bool, String)>) };
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        if unsafe { GetMonitorInfoW(hmonitor, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) }.as_bool() {
            let primary = info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0;
            found.push((hmonitor, info.monitorInfo.rcMonitor, primary, wide_to_string(&info.szDevice)));
        }
        true.into()
    }
    let mut found = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut found as *mut _ as isize));
    }
    found
}

struct Target {
    friendly_name: String,
    device_path: String,
}

/// The monitor behind each GDI display name, from the display configuration.
fn display_targets() -> HashMap<String, Target> {
    let mut targets = HashMap::new();
    let (mut path_count, mut mode_count) = (0u32, 0u32);
    unsafe {
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count) != ERROR_SUCCESS {
            return targets;
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        let status = QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        );
        if status != ERROR_SUCCESS {
            return targets;
        }
        paths.truncate(path_count as usize);

        for path in &paths {
            let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                    size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                    adapterId: path.sourceInfo.adapterId,
                    id: path.sourceInfo.id,
                },
                ..Default::default()
            };
            let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                    size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                    adapterId: path.targetInfo.adapterId,
                    id: path.targetInfo.id,
                },
                ..Default::default()
            };
            if DisplayConfigGetDeviceInfo(&mut source.header) != 0 || DisplayConfigGetDeviceInfo(&mut target.header) != 0 {
                continue;
            }
            // A cloned display has several targets on one source; the first one names it.
            targets.entry(wide_to_string(&source.viewGdiDeviceName)).or_insert(Target {
                friendly_name: wide_to_string(&target.monitorFriendlyDeviceName),
                device_path: wide_to_string(&target.monitorDevicePath),
            });
        }
    }
    targets
}

/// `\\?\DISPLAY#GSM776F#4&1d2e3f40&0&UID1024#{guid}` → `DISPLAY\GSM776F\4&1d2e3f40&0&UID1024`.
fn instance_id(device_path: &str) -> Option<String> {
    let parts: Vec<&str> = device_path.strip_prefix(r"\\?\")?.split('#').collect();
    (parts.len() >= 3).then(|| parts[..3].join("\\"))
}

/// The raw EDID Windows cached for a monitor's device instance.
fn read_edid(instance: &str) -> Option<Vec<u8>> {
    let key = HSTRING::from(format!(r"SYSTEM\CurrentControlSet\Enum\{instance}\Device Parameters"));
    let mut len = 0u32;
    unsafe {
        if RegGetValueW(HKEY_LOCAL_MACHINE, &key, w!("EDID"), RRF_RT_REG_BINARY, None, None, Some(&mut len)) != ERROR_SUCCESS {
            return None;
        }
        let mut bytes = vec![0u8; len as usize];
        let status = RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &key,
            w!("EDID"),
            RRF_RT_REG_BINARY,
            None,
            Some(bytes.as_mut_ptr().cast()),
            Some(&mut len),
        );
        (status == ERROR_SUCCESS).then(|| {
            bytes.truncate(len as usize);
            bytes
        })
    }
}

pub fn wide_to_string(wide: &[u16]) -> String {
    let len = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..len])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(id: &str) -> Monitor {
        Monitor { id: id.into(), model: "LG".into(), serial: None, rect: RECT::default(), primary: false, hmonitor: 0 }
    }

    #[test]
    fn instance_id_from_device_path() {
        let path = r"\\?\DISPLAY#GSM776F#4&1d2e3f40&0&UID1024#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";
        assert_eq!(instance_id(path).as_deref(), Some(r"DISPLAY\GSM776F\4&1d2e3f40&0&UID1024"));
        assert_eq!(instance_id("garbage"), None);
    }

    #[test]
    fn duplicate_ids_get_their_port_appended() {
        let mut found = vec![
            (monitor("GSM776F-1"), "port-a".to_string()),
            (monitor("GSM776F-1"), "port-b".to_string()),
            (monitor("GSM776F-2"), "port-c".to_string()),
        ];
        make_ids_unique(&mut found);
        let ids: Vec<&str> = found.iter().map(|(m, _)| m.id.as_str()).collect();
        assert_eq!(ids, ["GSM776F-1@port-a", "GSM776F-1@port-b", "GSM776F-2"]);
    }

    /// Reads this PC's real monitors: `cargo test -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs real monitors"]
    fn lists_connected_monitors() {
        let monitors = enumerate();
        for m in &monitors {
            let dims = crate::ddc::supports_brightness(m.handle());
            println!("{} | {} | {:?} | primary={} dims={dims}", m.id, m.label(), m.rect, m.primary);
        }
        assert!(!monitors.is_empty());
    }

    #[test]
    fn label_shows_end_of_serial() {
        let mut m = monitor("x");
        m.model = "LG ULTRAGEAR".into();
        m.serial = Some("305NTZX56789".into());
        assert_eq!(m.label(), "LG ULTRAGEAR (…56789)");
        m.serial = None;
        assert_eq!(m.label(), "LG ULTRAGEAR");
    }
}
