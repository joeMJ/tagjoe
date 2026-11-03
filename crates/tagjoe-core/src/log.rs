//! Plain-text activity log (`log.txt` in the config directory): one line per written,
//! renamed, deleted or undone file and per error. Kept to the last `MAX_LINES` lines.

use crate::config::config_dir;
use std::path::PathBuf;

pub const MAX_LINES: usize = 2000;

pub fn log_path() -> PathBuf {
    config_dir().join("log.txt")
}

/// One log line: local time, folder, file, action.
pub fn line(unit: &str, file: &str, action: &str) -> String {
    format!("{}\t{unit}\t{file}\t{action}", crate::journal::now_human())
}

/// Append lines and trim the file to the last `MAX_LINES` lines.
pub fn append(lines: &[String]) -> std::io::Result<()> {
    if lines.is_empty() {
        return Ok(());
    }
    let p = log_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let mut all: Vec<String> = std::fs::read_to_string(&p).map(|s| s.lines().map(|l| l.to_string()).collect()).unwrap_or_default();
    all.extend(lines.iter().cloned());
    if all.len() > MAX_LINES {
        all.drain(0..all.len() - MAX_LINES);
    }
    let mut text = all.join("\n");
    text.push('\n');
    std::fs::write(&p, text)
}

/// The last `n` lines, oldest first.
pub fn tail(n: usize) -> Vec<String> {
    let all: Vec<String> = std::fs::read_to_string(log_path()).map(|s| s.lines().map(|l| l.to_string()).collect()).unwrap_or_default();
    let skip = all.len().saturating_sub(n);
    all.into_iter().skip(skip).collect()
}

pub fn clear() -> std::io::Result<()> {
    let p = log_path();
    if p.exists() {
        std::fs::remove_file(p)
    } else {
        Ok(())
    }
}
