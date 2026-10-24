//! Backlight brightness over DDC/CI (MCCS VCP code 0x10), through dxva2.

use std::thread::sleep;
use std::time::Duration;

use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitor, GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR,
    GetVCPFeatureAndVCPFeatureReply, PHYSICAL_MONITOR, SetVCPFeature,
};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Gdi::HMONITOR;

const VCP_BRIGHTNESS: u8 = 0x10;
/// DDC/CI is a slow, lossy bus, so every read and write gets a few tries.
const ATTEMPTS: usize = 3;

/// One physical monitor's DDC/CI handle, released on drop.
pub struct Physical {
    handle: HANDLE,
    /// Position behind its HMONITOR; only a cloned display has more than one.
    pub index: usize,
}

pub fn physical_monitors(hmonitor: HMONITOR) -> Vec<Physical> {
    let mut count = 0u32;
    unsafe {
        if GetNumberOfPhysicalMonitorsFromHMONITOR(hmonitor, &mut count).is_err() || count == 0 {
            return Vec::new();
        }
        let mut physical = vec![PHYSICAL_MONITOR::default(); count as usize];
        if GetPhysicalMonitorsFromHMONITOR(hmonitor, &mut physical).is_err() {
            return Vec::new();
        }
        physical
            .into_iter()
            .enumerate()
            .map(|(index, p)| Physical { handle: p.hPhysicalMonitor, index })
            .collect()
    }
}

/// Whether any physical monitor behind `hmonitor` answers brightness queries.
pub fn supports_brightness(hmonitor: HMONITOR) -> bool {
    physical_monitors(hmonitor).iter().any(|p| p.brightness().is_some())
}

impl Physical {
    pub fn brightness(&self) -> Option<u32> {
        for _ in 0..ATTEMPTS {
            let (mut current, mut max) = (0u32, 0u32);
            let ok = unsafe {
                GetVCPFeatureAndVCPFeatureReply(self.handle, VCP_BRIGHTNESS, None, &mut current, Some(&mut max))
            };
            if ok != 0 {
                return Some(current);
            }
            sleep(Duration::from_millis(50));
        }
        None
    }

    pub fn set_brightness(&self, level: u32) -> bool {
        for _ in 0..ATTEMPTS {
            if unsafe { SetVCPFeature(self.handle, VCP_BRIGHTNESS, level) } != 0 {
                return true;
            }
            sleep(Duration::from_millis(100));
        }
        false
    }
}

impl Drop for Physical {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyPhysicalMonitor(self.handle);
        }
    }
}
