//! The log file: events only, never message text, response bodies or the key.
use std::fs::OpenOptions;
use std::path::Path;
use std::sync::Mutex;

/// Sends `tracing` events to the file at `path`; without a writable file nothing is logged and the terminal stays clean.
pub fn start(path: &Path) {
    let Some(dir) = path.parent() else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let Ok(file) = OpenOptions::new().create(true).append(true).open(path) else { return };
    tracing_subscriber::fmt().with_writer(Mutex::new(file)).with_ansi(false).with_max_level(tracing::Level::INFO).init();
}
