//! The launcher's own log (`logs/launcher.log`): starts, stops, downloads, updates, errors.
//! KoboldCpp's output goes to `logs/koboldcpp.log` (see `server.rs`).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static LOG: Mutex<Option<File>> = Mutex::new(None);

/// Keep at most this much; the previous log is kept once as `launcher.log.old`.
const MAX_BYTES: u64 = 2 * 1024 * 1024;

pub fn init(path: &Path) {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(path, path.with_extension("log.old"));
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(f) = OpenOptions::new().create(true).append(true).open(path)
        && let Ok(mut g) = LOG.lock()
    {
        *g = Some(f);
    }
}

/// "2026-11-05 03:12:01" (UTC) from unix seconds.
pub fn timestamp(unix: i64) -> String {
    let days = unix.div_euclid(86400);
    let secs = unix.rem_euclid(86400);
    // civil-from-days (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", secs / 3600, secs % 3600 / 60, secs % 60)
}

pub fn write(msg: &str) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let line = format!("{} UTC  {msg}\n", timestamp(now));
    if let Ok(mut g) = LOG.lock()
        && let Some(f) = g.as_mut()
    {
        let _ = f.write_all(line.as_bytes());
    }
    #[cfg(debug_assertions)]
    eprint!("{line}");
}

/// Last `max` bytes of a text file (whole lines), or an empty string.
pub fn tail(path: &PathBuf, max: u64) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = File::open(path) else { return String::new() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(max);
    let _ = f.seek(SeekFrom::Start(start));
    let mut buf = Vec::new();
    let _ = f.read_to_end(&mut buf);
    let text = String::from_utf8_lossy(&buf).into_owned();
    if start > 0 { text.split_once('\n').map(|(_, rest)| rest.to_string()).unwrap_or(text) } else { text }
}

#[macro_export]
macro_rules! applog {
    ($($arg:tt)*) => { $crate::applog::write(&format!($($arg)*)) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps() {
        assert_eq!(timestamp(0), "1970-01-01 00:00:00");
        assert_eq!(timestamp(1_793_836_800 + 3 * 3600 + 61), "2026-11-05 03:01:01");
    }

    #[test]
    fn tail_keeps_whole_lines() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x.log");
        std::fs::write(&p, "first line\nsecond\nthird\n").unwrap();
        assert_eq!(tail(&p, 13), "third\n");
        assert_eq!(tail(&p, 1000), "first line\nsecond\nthird\n");
        assert_eq!(tail(&d.path().join("missing"), 10), "");
    }
}
