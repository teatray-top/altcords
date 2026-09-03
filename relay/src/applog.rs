use std::io::Write;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Above this the log is rotated to `<name>.1`, so it cannot grow without end.
const MAX_BYTES: u64 = 4 * 1024 * 1024;

/// Set `ALTCORDS_LOG_TEXT=1` to record the text being spoken. Off by default:
/// everything typed into the relay would otherwise sit in a plain file forever.
fn log_text_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("ALTCORDS_LOG_TEXT")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

/// A sentence as it should appear in the log: the text itself only in
/// diagnostic mode, otherwise its shape.
pub fn redact(text: &str) -> String {
    if log_text_enabled() {
        format!("{text:?}")
    } else {
        format!("<{} chars>", text.chars().count())
    }
}

fn rotate(path: &str) {
    let too_big = std::fs::metadata(path).map(|m| m.len() > MAX_BYTES).unwrap_or(false);
    if too_big {
        let _ = std::fs::rename(path, format!("{path}.1"));
    }
}

pub fn log(msg: &str) {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let path = crate::paths::log_path();
    rotate(path);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "[{t:.2}] {msg}");
    }
}
