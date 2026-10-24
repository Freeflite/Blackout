// Embeds the app icon (drawn by src/icon_art.rs) and a manifest asking for per-monitor DPI
// awareness, so overlay bounds are always real pixels.

include!("src/icon_art.rs");

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
    println!("cargo:rerun-if-changed=src/icon_art.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let ico = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("blackout.ico");
    std::fs::write(&ico, ico_bytes(&[16, 20, 24, 32, 40, 48, 64, 256])).unwrap();

    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(ico.to_str().unwrap());
    resource.set_manifest(MANIFEST);
    if let Err(e) = resource.compile() {
        println!("cargo:warning=building without the icon and manifest: {e}");
    }
}

/// A .ico file holding one 32-bit image per size.
fn ico_bytes(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<Vec<u8>> = sizes.iter().map(|&size| bmp_image(size)).collect();
    let mut ico = vec![0, 0, 1, 0];
    ico.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&size, image) in sizes.iter().zip(&images) {
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

/// BITMAPINFOHEADER, then bottom-up BGRA pixels, then an all-clear AND mask (alpha does the masking).
fn bmp_image(size: u32) -> Vec<u8> {
    let rgba = icon_rgba(size);
    let mut image = Vec::new();
    for value in [40, size, size * 2] {
        image.extend_from_slice(&value.to_le_bytes()); // header size, width, height of image + mask
    }
    image.extend_from_slice(&1u16.to_le_bytes()); // planes
    image.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    image.extend_from_slice(&[0; 24]); // no compression, palette or resolution
    for y in (0..size).rev() {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            image.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    let mask_row = size.div_ceil(32) * 4;
    image.extend(std::iter::repeat_n(0u8, (mask_row * size) as usize));
    image
}
