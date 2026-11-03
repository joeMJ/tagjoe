//! Update check via GitHub releases: off by default, no telemetry. The request sends the IP
//! address and the program id to GitHub. Downloads only after confirmation; the package is
//! verified against `SHA256SUMS` from the same release.

use crate::VERSION;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Public repository (published through the filtered branch).
pub const REPO: &str = "joeMJ/tagjoe";
const MAX_PACKAGE_BYTES: usize = 200 * 1024 * 1024;
/// A day in seconds: the automatic check runs at most this often.
pub const CHECK_INTERVAL_SECS: u64 = 24 * 3600;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

/// `v1.2.3` or `1.2.3` (suffixes after `-`/`+` are ignored).
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let core = s.split(['-', '+']).next()?;
    let mut it = core.split('.').map(|p| p.parse::<u64>().ok());
    Some((it.next()??, it.next().flatten().unwrap_or(0), it.next().flatten().unwrap_or(0)))
}

pub fn is_newer(current: &str, candidate: &str) -> bool {
    matches!((parse_version(current), parse_version(candidate)), (Some(a), Some(b)) if b > a)
}

pub fn parse_release(json: &str) -> Result<Release, String> {
    serde_json::from_str(json).map_err(|e| format!("GitHub answer unreadable: {e}"))
}

/// Debian architecture of this program.
pub fn deb_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

impl Release {
    /// One package for every Debian/Ubuntu version: `tagjoe_<version>_<arch>.deb`.
    pub fn package_for(&self, arch: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name.ends_with(&format!("_{arch}.deb")) && a.name.starts_with("tagjoe_"))
    }
    /// Windows installer: `tagjoe-<version>-windows-<arch>.msi`.
    pub fn msi_for(&self, arch: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name.ends_with(&format!("-windows-{arch}.msi")))
    }
    pub fn sums(&self) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == "SHA256SUMS")
    }
    /// Checksum file for a package: `<name>.sha256`, else `SHA256SUMS`.
    fn sums_for(&self, name: &str) -> Option<&Asset> {
        let own = format!("{name}.sha256");
        self.assets.iter().find(|a| a.name == own).or_else(|| self.sums())
    }
    /// The package for this system.
    pub fn package(&self) -> Result<&Asset, String> {
        if cfg!(windows) {
            let arch = std::env::consts::ARCH;
            return self.msi_for(arch).ok_or_else(|| format!("no Windows installer for {arch} in the release"));
        }
        self.package_for(deb_arch()).ok_or_else(|| format!("no package for {} in the release", deb_arch()))
    }
}

/// Checksum for a file name from a `sha256sum` listing.
pub fn expected_sum(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, file) = l.split_once(char::is_whitespace)?;
        (file.trim().trim_start_matches('*') == name).then(|| hash.to_ascii_lowercase())
    })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder().user_agent(format!("tagjoe/{VERSION}")).timeout(Duration::from_secs(60)).build().map_err(|e| e.to_string())
}

fn fetch(client: &reqwest::blocking::Client, url: &str, limit: usize) -> Result<Vec<u8>, String> {
    let resp = client.get(url).send().map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(format!("{url}: status {status}"));
    }
    let bytes = resp.bytes().map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err(format!("{url}: larger than {limit} bytes"));
    }
    Ok(bytes.to_vec())
}

/// Ask GitHub for the latest release. `Ok(None)`: nothing newer (or no release yet).
pub fn check() -> Result<Option<Release>, String> {
    let client = client()?;
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = client.get(&url).header("Accept", "application/vnd.github+json").send().map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    if status == 404 {
        return Ok(None);
    }
    if status != 200 {
        return Err(format!("GitHub answers with status {status}"));
    }
    let json = resp.text().map_err(|e| e.to_string())?;
    let release = parse_release(&json)?;
    Ok((!release.draft && !release.prerelease && is_newer(VERSION, &release.tag_name)).then_some(release))
}

/// Download the package for this system into `dir` and verify it against `SHA256SUMS`.
pub fn download(release: &Release, dir: &Path) -> Result<PathBuf, String> {
    let asset = release.package()?;
    let sums = release.sums_for(&asset.name).ok_or("release without SHA256SUMS – download refused")?;
    let client = client()?;
    let sums_text = String::from_utf8(fetch(&client, &sums.browser_download_url, 1024 * 1024)?).map_err(|_| "SHA256SUMS unreadable".to_string())?;
    let expected = expected_sum(&sums_text, &asset.name).ok_or_else(|| format!("{} missing in SHA256SUMS", asset.name))?;
    let bytes = fetch(&client, &asset.browser_download_url, MAX_PACKAGE_BYTES)?;
    let actual = sha256_hex(&bytes);
    if actual != expected {
        return Err(format!("checksum mismatch (expected {expected}, got {actual})"));
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(&asset.name);
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Command that installs a downloaded package (shown before it runs).
/// Windows: per-user installer with progress; Linux: apt through pkexec (asks for the password).
pub fn install_command(package: &Path) -> Vec<String> {
    if cfg!(windows) {
        return vec!["msiexec".into(), "/i".into(), package.to_string_lossy().into_owned(), "/passive".into()];
    }
    vec!["pkexec".into(), "apt-get".into(), "install".into(), "-y".into(), package.to_string_lossy().into_owned()]
}

/// Release notes as plain lines for the interface: the part in the interface language
/// (German before `---`, English after it under `**English**`), bullets kept, `**`/`` ` `` dropped.
pub fn release_notes(body: &str, english: bool) -> Vec<String> {
    let mut parts = body.split("\n---");
    let german = parts.next().unwrap_or_default();
    let text = match parts.next() {
        Some(en) if english => en,
        _ => german,
    };
    text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && *l != "---" && *l != "**English**")
        .map(|l| {
            let (bullet, rest) = match l.strip_prefix("- ").or_else(|| l.strip_prefix("* ")) {
                Some(r) => (true, r),
                None => (false, l),
            };
            let plain = rest.replace("**", "").replace('`', "");
            if bullet { format!("• {plain}") } else { plain }
        })
        .collect()
}

/// Unix time now.
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v0.1.2"), Some((0, 1, 2)));
        assert_eq!(parse_version("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("1.2.3-beta+5"), Some((1, 2, 3)));
        assert!(is_newer("0.1.0", "v0.1.1"));
        assert!(!is_newer("0.1.1", "v0.1.1"));
        assert!(!is_newer("0.2.0", "v0.1.9"));
        assert!(!is_newer("0.1.0", "junk"));
    }

    const JSON: &str = r#"{
        "tag_name": "v9.9.9", "name": "tagjoe 9.9.9", "body": "- **Neu:** Protokoll\n\n---\n\n**English**\n\n- **New:** log",
        "html_url": "https://github.com/joeMJ/tagjoe/releases/tag/v9.9.9",
        "draft": false, "prerelease": false,
        "assets": [
            {"name": "tagjoe_9.9.9_amd64.deb", "browser_download_url": "https://x.invalid/tagjoe_9.9.9_amd64.deb", "size": 10},
            {"name": "tagjoe-9.9.9-windows-x86_64.msi", "browser_download_url": "https://x.invalid/tagjoe-9.9.9-windows-x86_64.msi"},
            {"name": "SHA256SUMS", "browser_download_url": "https://x.invalid/SHA256SUMS"}
        ]}"#;

    #[test]
    fn release_assets_and_notes() {
        let r = parse_release(JSON).unwrap();
        assert_eq!(r.package_for("amd64").unwrap().name, "tagjoe_9.9.9_amd64.deb");
        assert!(r.package_for("arm64").is_none());
        assert_eq!(r.msi_for("x86_64").unwrap().name, "tagjoe-9.9.9-windows-x86_64.msi");
        assert!(r.sums().is_some());
        assert_eq!(r.sums_for("tagjoe_9.9.9_amd64.deb").unwrap().name, "SHA256SUMS");
        assert!(is_newer(VERSION, &r.tag_name));
        assert_eq!(release_notes(r.body.as_deref().unwrap(), false), vec!["• Neu: Protokoll"]);
        assert_eq!(release_notes(r.body.as_deref().unwrap(), true), vec!["• New: log"]);
        assert_eq!(release_notes("Nur deutsch", true), vec!["Nur deutsch"]);
    }

    #[test]
    fn sums() {
        let sums = "abc  tagjoe_1.0.0_amd64.deb\ndef *other.msi\n";
        assert_eq!(expected_sum(sums, "tagjoe_1.0.0_amd64.deb").as_deref(), Some("abc"));
        assert_eq!(expected_sum(sums, "other.msi").as_deref(), Some("def"));
        assert_eq!(expected_sum(sums, "none"), None);
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let cmd = install_command(Path::new("/tmp/x.deb"));
        assert!(cmd.iter().any(|c| c == "/tmp/x.deb"));
    }
}
