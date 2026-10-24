// Embeds the FreeFlite app icon (from assets/freeflite.png) at every size Windows asks for, and a
// manifest asking for per-monitor DPI awareness so overlay bounds are always real pixels. Also writes
// a 64px raw RGBA copy of the icon for the settings window to show while it starts up.

use std::io::Cursor;
use std::path::PathBuf;

use image::imageops::FilterType;
use image::{ImageFormat, RgbaImage};

const ICON_SOURCE: &str = "assets/freeflite.png";
const ICON_SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 128, 256];
/// Must match WINDOW_ICON_SIZE in src/ui.rs.
const WINDOW_ICON_SIZE: u32 = 64;

const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
</assembly>
"#;

fn main() {
    println!("cargo:rerun-if-changed={ICON_SOURCE}");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let icon = image::open(ICON_SOURCE).expect("read the app icon").to_rgba8();

    let window_icon = resize(&icon, WINDOW_ICON_SIZE);
    std::fs::write(out.join("window-icon.rgba"), window_icon.as_raw()).unwrap();

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let ico = out.join("blackout.ico");
    std::fs::write(&ico, ico_bytes(&icon)).unwrap();

    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(ico.to_str().unwrap()); // resource ID 1, which the settings window loads too
    resource.set_manifest(MANIFEST);
    if let Err(e) = resource.compile() {
        println!("cargo:warning=building without the icon and manifest: {e}");
    }
}

fn resize(icon: &RgbaImage, size: u32) -> RgbaImage {
    image::imageops::resize(icon, size, size, FilterType::Lanczos3)
}

/// A .ico file holding a PNG-compressed image per size, each scaled down from the master.
fn ico_bytes(icon: &RgbaImage) -> Vec<u8> {
    let images: Vec<Vec<u8>> = ICON_SIZES
        .iter()
        .map(|&size| {
            let mut png = Vec::new();
            resize(icon, size).write_to(&mut Cursor::new(&mut png), ImageFormat::Png).unwrap();
            png
        })
        .collect();
    let mut ico = vec![0, 0, 1, 0];
    ico.extend_from_slice(&(ICON_SIZES.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * ICON_SIZES.len() as u32;
    for (&size, image) in ICON_SIZES.iter().zip(&images) {
        let dim = if size >= 256 { 0 } else { size as u8 }; // 0 means 256
        ico.extend_from_slice(&[dim, dim, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes()); // planes
        ico.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        ico.extend_from_slice(&(image.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += image.len() as u32;
    }
    for image in images {
        ico.extend(image);
    }
    ico
}
