//! Acoustic fingerprints via the `fpcalc` helper (Chromaprint, as MusicBrainz Picard does).

use super::{Result, SourceError};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub struct Fingerprint {
    pub duration: f64,
    pub fingerprint: String,
}

/// Where `fpcalc` lives: next to our executable (Windows bundle), else on PATH.
pub fn fpcalc_path() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in ["fpcalc.exe", "fpcalc"] {
                let p = dir.join(name);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    let name = if cfg!(windows) { "fpcalc.exe" } else { "fpcalc" };
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(name)).find(|p| p.is_file())
}

pub fn available() -> bool {
    fpcalc_path().is_some()
}

/// Compute the fingerprint of the first `length` seconds (120 as AcoustID recommends).
pub fn compute(path: &Path, length: u32) -> Result<Fingerprint> {
    let exe = fpcalc_path().ok_or_else(|| SourceError::Tool("fpcalc not found (install libchromaprint-tools or put fpcalc next to tagjoe)".into()))?;
    let out = Command::new(exe).arg("-json").arg("-length").arg(length.to_string()).arg(path).output().map_err(|e| SourceError::Tool(format!("cannot run fpcalc: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(SourceError::Tool(format!("fpcalc failed for {}: {err}", path.display())));
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| SourceError::Tool(format!("fpcalc output: {e}")))?;
    let duration = v.get("duration").and_then(|d| d.as_f64()).unwrap_or(0.0);
    let fingerprint = v.get("fingerprint").and_then(|f| f.as_str()).unwrap_or("").to_string();
    if fingerprint.is_empty() {
        return Err(SourceError::Tool(format!("empty fingerprint for {}", path.display())));
    }
    Ok(Fingerprint { duration, fingerprint })
}

/// Fingerprint many files with a few worker threads; `None` where fpcalc failed.
pub fn compute_all(paths: &[PathBuf], length: u32, progress: super::Progress<'_>) -> Vec<Option<Fingerprint>> {
    let n = paths.len();
    let workers = std::thread::available_parallelism().map(|c| c.get()).unwrap_or(2).clamp(1, 4);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results: Vec<std::sync::Mutex<Option<Fingerprint>>> = (0..n).map(|_| std::sync::Mutex::new(None)).collect();
    let done = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if i >= n {
                        break;
                    }
                    let r = compute(&paths[i], length).ok();
                    *results[i].lock().unwrap() = r;
                    let d = done.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                    progress("fingerprint", d, n);
                }
            });
        }
    });
    results.into_iter().map(|m| m.into_inner().unwrap()).collect()
}
