//! Write a crash record before the process dies.
//!
//! The app runs on three desktop toolkits and we cannot reproduce every
//! window-manager/IME combination. When a panic escapes (the Linux text-input
//! reports are the reason this exists), the default hook prints to a terminal
//! that a launcher-started app usually does not have. This hook appends the
//! payload, location and a backtrace to `panic.log` in the per-user config
//! folder, then lets the default hook print as before.
//!
//! The file is capped so a panic loop cannot fill the disk. Writing uses
//! `std::fs` directly and ignores its error: a crash logger must never panic.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Once;

const MAX_LOG_BYTES: u64 = 64 * 1024;

static INSTALL: Once = Once::new();

/// Install the hook once per process.
pub fn install() {
    INSTALL.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            record(&log_path(), info);
            previous(info);
        }));
    });
}

/// Where the crash record is written.
pub fn log_path() -> PathBuf {
    crate::app::config_dir().join("panic.log")
}

/// Append one formatted entry, rotating the file first when it is large.
pub fn record(path: &Path, info: &std::panic::PanicHookInfo<'_>) {
    let payload = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "non-string panic payload".to_string());
    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "unknown location".to_string());
    let backtrace = std::env::var_os("RUST_BACKTRACE")
        .filter(|v| v != "0")
        .map(|_| std::backtrace::Backtrace::force_capture().to_string());
    let text = entry_text(&payload, &location, backtrace.as_deref());
    append(path, &text);
}

/// The entry body, separate from the hook so it can be tested without
/// deliberately panicking the test process.
fn entry_text(payload: &str, location: &str, backtrace: Option<&str>) -> String {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let mut out = format!(
        "== {now} ==\nScriptCraft {} / {} {}\npanic: {payload}\nat {location}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    if let Some(bt) = backtrace {
        out.push_str(bt.trim_end());
        out.push('\n');
    }
    out.push('\n');
    out
}

fn append(path: &Path, text: &str) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let too_big = std::fs::metadata(path)
        .map(|m| m.len() > MAX_LOG_BYTES)
        .unwrap_or(false);
    if too_big {
        let _ = std::fs::remove_file(path);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = file.write_all(text.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_names_the_version_platform_and_location() {
        let text = entry_text("index out of bounds", "src/script_view.rs:10:4", None);
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
        assert!(text.contains(std::env::consts::OS));
        assert!(text.contains("panic: index out of bounds"));
        assert!(text.contains("src/script_view.rs:10:4"));
    }

    #[test]
    fn backtrace_is_optional_and_appended_verbatim() {
        let text = entry_text("boom", "a.rs:1:1", Some("frame one\nframe two\n"));
        assert!(text.contains("frame one\nframe two\n"));
        assert!(!entry_text("boom", "a.rs:1:1", None).contains("frame one"));
    }

    #[test]
    fn append_keeps_earlier_entries_and_creates_the_folder() {
        let dir = std::env::temp_dir().join(format!("scriptcraft-panic-{}", std::process::id()));
        let path = dir.join("nested").join("panic.log");
        let _ = std::fs::remove_dir_all(&dir);
        append(&path, "first\n");
        append(&path, "second\n");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("first"));
        assert!(text.contains("second"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
