//! Test stand-in for `D2RLoader.exe` (#58), used by `tests/game_process.rs`.
//!
//! Writes `{"args": [...], "cwd": "...", "compat_layer": ..., "moz": [...]}`
//! (`__COMPAT_LAYER` or null, and the names of `MOZ_*` variables) to the file named by
//! `FAKE_D2RLOADER_OUT` (when set), then sleeps `FAKE_D2RLOADER_SLEEP_MS`
//! milliseconds (default 0) and exits.

use std::time::Duration;
use std::{env, fs, thread};

fn main() {
    if let Ok(out) = env::var("FAKE_D2RLOADER_OUT") {
        let record = serde_json::json!({
            "args": env::args().skip(1).collect::<Vec<_>>(),
            "cwd": env::current_dir().expect("cwd"),
            "compat_layer": env::var("__COMPAT_LAYER").ok(),
            "moz": env::vars().map(|(k, _)| k).filter(|k| k.starts_with("MOZ_")).collect::<Vec<_>>(),
        });
        fs::write(out, record.to_string()).expect("write record");
    }
    let ms = env::var("FAKE_D2RLOADER_SLEEP_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    thread::sleep(Duration::from_millis(ms));
}
