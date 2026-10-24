// The Blackout icon, drawn in code so the tray, the settings window and the .exe share one picture.
// build.rs include!()s this file too, so it must not reference anything else in the crate.

/// RGBA pixels of the icon (a monitor with a blacked-out screen) at `size`×`size`.
pub fn icon_rgba(size: u32) -> Vec<u8> {
    // Rectangles on a 32×32 design grid as (left, top, right, bottom, RGBA); later ones paint over earlier ones.
    const SHAPES: [(f32, f32, f32, f32, [u8; 4]); 5] = [
        (8.0, 26.0, 24.0, 29.0, [0x9A, 0x9A, 0x9A, 0xFF]),  // foot
        (13.0, 23.0, 19.0, 27.0, [0x9A, 0x9A, 0x9A, 0xFF]), // neck
        (1.0, 3.0, 31.0, 24.0, [0x30, 0x30, 0x30, 0xFF]),   // outline, so the bezel shows on light taskbars
        (2.0, 4.0, 30.0, 23.0, [0xE8, 0xE8, 0xE8, 0xFF]),   // bezel
        (4.0, 6.0, 28.0, 21.0, [0x00, 0x00, 0x00, 0xFF]),   // screen
    ];
    let scale = 32.0 / size as f32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let (u, v) = ((x as f32 + 0.5) * scale, (y as f32 + 0.5) * scale);
            for &(left, top, right, bottom, color) in &SHAPES {
                if u >= left && u < right && v >= top && v < bottom {
                    let i = ((y * size + x) * 4) as usize;
                    rgba[i..i + 4].copy_from_slice(&color);
                }
            }
        }
    }
    rgba
}
