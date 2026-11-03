//! Acoustic fingerprints via the `fpcalc` helper (Chromaprint, as MusicBrainz Picard does).

use super::{Result, SourceError};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub struct Fingerprint {
    pub duration: f64,
    pub fingerprint: String,
}

/// Chromaprint release tagjoe downloads on request (Settings → Sources → AcoustID).
pub const FPCALC_VERSION: &str = "1.6.2";
/// Archive name, download address and SHA-256 for this system; `None` where no archive exists.
pub fn fpcalc_release() -> Option<(&'static str, String, &'static str)> {
    let (name, sum) = if cfg!(all(windows, target_arch = "x86_64")) {
        ("chromaprint-fpcalc-1.6.2-windows-x86_64.zip", "b74369726676670af3ab0eadff45f960cf969fdf4a87a971ba37eaf8d530e874")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        ("chromaprint-fpcalc-1.6.2-linux-x86_64.tar.gz", "c968ad4a0821592a79b158fb2e1f46cbe0a22a0a1dbc0743c67d68e75d15d2bb")
    } else {
        return None;
    };
    Some((name, format!("https://github.com/acoustid/chromaprint/releases/download/v{FPCALC_VERSION}/{name}"), sum))
}

/// Where `fpcalc` lives: tagjoe's own tools folder (downloaded), next to our executable, else on PATH.
pub fn fpcalc_path() -> Option<PathBuf> {
    let name = if cfg!(windows) { "fpcalc.exe" } else { "fpcalc" };
    let own = crate::config::tools_dir().join(name);
    if own.is_file() {
        return Some(own);
    }
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

/// Download the Chromaprint release for this system (after the user asked for it), verify its
/// SHA-256 against the value compiled in, unpack `fpcalc` into [`crate::config::tools_dir`]
/// with the system's own unpacker (PowerShell `Expand-Archive` / `tar`) and return its path.
pub fn download_fpcalc() -> std::result::Result<PathBuf, String> {
    use sha2::{Digest, Sha256};
    let (name, url, expected) = fpcalc_release().ok_or("no fpcalc build for this system; install Chromaprint yourself")?;
    let client = reqwest::blocking::Client::builder().user_agent(format!("tagjoe/{}", crate::VERSION)).timeout(std::time::Duration::from_secs(120)).build().map_err(|e| e.to_string())?;
    let resp = client.get(&url).send().map_err(|e| e.to_string())?;
    if resp.status().as_u16() != 200 {
        return Err(format!("{url}: status {}", resp.status().as_u16()));
    }
    let bytes = resp.bytes().map_err(|e| e.to_string())?;
    if bytes.len() > 20 * 1024 * 1024 {
        return Err("archive larger than expected".into());
    }
    let actual: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    if actual != expected {
        return Err(format!("checksum mismatch for {name} (expected {expected}, got {actual})"));
    }
    let dir = crate::config::tools_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let archive = dir.join(name);
    std::fs::write(&archive, &bytes).map_err(|e| e.to_string())?;
    let unpack = dir.join("unpack");
    let _ = std::fs::remove_dir_all(&unpack);
    std::fs::create_dir_all(&unpack).map_err(|e| e.to_string())?;
    let status = if cfg!(windows) {
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &format!("Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force", archive.display(), unpack.display())])
            .status()
    } else {
        std::process::Command::new("tar").arg("xzf").arg(&archive).arg("-C").arg(&unpack).status()
    };
    match status {
        Ok(st) if st.success() => {}
        Ok(st) => return Err(format!("unpacking failed (exit {st})")),
        Err(e) => return Err(format!("cannot run the unpacker: {e}")),
    }
    let exe_name = if cfg!(windows) { "fpcalc.exe" } else { "fpcalc" };
    let found = find_file(&unpack, exe_name).ok_or_else(|| format!("{exe_name} missing in {name}"))?;
    let target = dir.join(exe_name);
    std::fs::copy(&found, &target).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755));
    }
    let _ = std::fs::remove_dir_all(&unpack);
    let _ = std::fs::remove_file(&archive);
    Ok(target)
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(f) = find_file(&p, name) {
                return Some(f);
            }
        } else if p.file_name().map(|n| n == name).unwrap_or(false) {
            return Some(p);
        }
    }
    None
}

/// Compute the fingerprint of the first `length` seconds (120 as AcoustID recommends).
pub fn compute(path: &Path, length: u32) -> Result<Fingerprint> {
    let exe = fpcalc_path().ok_or_else(|| SourceError::Tool("fpcalc not found (Settings → Sources → AcoustID: download it, or install libchromaprint-tools)".into()))?;
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
