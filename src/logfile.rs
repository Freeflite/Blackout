//! Append-only log at %LOCALAPPDATA%\Blackout\blackout.log, since a tray app has no console.

use std::io::Write;

use windows::Win32::System::SystemInformation::GetLocalTime;

/// Past this size the log is moved to blackout.old.log and started afresh.
const MAX_BYTES: u64 = 512 * 1024;

pub fn write(message: &str) {
    let dir = crate::paths::local_dir();
    let path = dir.join("blackout.log");
    let _ = std::fs::create_dir_all(&dir);
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(&path, dir.join("blackout.old.log"));
    }
    let t = unsafe { GetLocalTime() };
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(
            file,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02} [{}] {message}",
            t.wYear,
            t.wMonth,
            t.wDay,
            t.wHour,
            t.wMinute,
            t.wSecond,
            std::process::id()
        );
    }
}
