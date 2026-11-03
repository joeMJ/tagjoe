//! Journal of one apply run, enough to undo it.

use crate::model::RawItem;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub path: PathBuf,
    pub new_path: PathBuf,
    /// (field key, old value) – empty old value means the field was absent.
    pub fields: Vec<(String, String)>,
    pub removed: Vec<RawItem>,
    pub containers: Vec<String>,
    /// File holding the previous cover bytes.
    pub cover: Option<PathBuf>,
    pub cover_was_present: bool,
    pub cover_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Journal {
    pub created: String,
    pub unit: PathBuf,
    pub new_folder: Option<PathBuf>,
    pub entries: Vec<JournalEntry>,
    /// Folder files removed by the clean-up (deleted for good; listed for the log, not restorable).
    #[serde(default)]
    pub deleted: Vec<PathBuf>,
}

/// Civil date-time from the system clock, as `YYYY-MM-DDTHH-MM-SS` (file-name safe).
pub fn timestamp() -> String {
    civil_now('T', '-')
}

/// Civil date-time for humans in local time, `YYYY-MM-DD HH:MM:SS`.
pub fn now_human() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    civil(secs + local_offset_secs(), ' ', ':')
}

/// Offset of the local time zone in seconds (asked once from `date +%z` on Unix; 0 elsewhere).
fn local_offset_secs() -> i64 {
    static OFFSET: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *OFFSET.get_or_init(|| {
        if cfg!(unix) {
            let out = std::process::Command::new("date").arg("+%z").output().ok();
            let s = out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
            // +0100 / -0530
            if s.len() == 5 {
                let sign = if s.starts_with('-') { -1 } else { 1 };
                let h: i64 = s[1..3].parse().unwrap_or(0);
                let m: i64 = s[3..5].parse().unwrap_or(0);
                return sign * (h * 3600 + m * 60);
            }
        }
        0
    })
}

fn civil_now(sep: char, time_sep: char) -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    civil(secs, sep, time_sep)
}

fn civil(secs: i64, sep: char, time_sep: char) -> String {
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Howard Hinnant's civil_from_days
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}{sep}{h:02}{time_sep}{m:02}{time_sep}{s:02}")
}

impl Journal {
    pub fn file_name(&self) -> String {
        let unit = self.unit.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let unit: String = unit.chars().filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_').take(40).collect();
        format!("{}-{unit}.json", self.created)
    }
    pub fn save(&self, dir: &Path) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let p = dir.join(self.file_name());
        std::fs::write(&p, serde_json::to_string_pretty(self).map_err(std::io::Error::other)?)?;
        Ok(p)
    }
    pub fn load(path: &Path) -> std::io::Result<Journal> {
        let text = std::fs::read_to_string(path)?;
        serde_json::from_str(&text).map_err(std::io::Error::other)
    }
    /// Journals in a directory, newest first.
    pub fn list(dir: &Path) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().map(|e| e == "json").unwrap_or(false)).collect()).unwrap_or_default();
        v.sort();
        v.reverse();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timestamp_shape() {
        let t = timestamp();
        assert_eq!(t.len(), 19, "{t}");
        assert!(t.starts_with("20"));
        assert_eq!(&t[10..11], "T");
    }
}
