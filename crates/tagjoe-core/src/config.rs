//! Configuration (TOML) in the user's config directory.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Album: the track artist is added only when it differs from the album artist (DJ mixes, guests).
pub const DEFAULT_ALBUM: &str = "{albumartist} - {album} - {year} - {track} von {tracktotal} - {artist != albumartist ? artist + ' - ' : ''}{title}";
pub const DEFAULT_COMPILATION: &str = "VA - {album} - {year} - {track} von {tracktotal} - {artist} - {title}";
pub const DEFAULT_AUDIOBOOK: &str = "{author} - {book} - {year} - {chapter} von {chaptertotal} - {title}";
pub const DEFAULT_FOLDER_ALBUM: &str = "{albumartist} - {album} - {year}";
pub const DEFAULT_FOLDER_COMPILATION: &str = "VA - {album} - {year}";
pub const DEFAULT_FOLDER_AUDIOBOOK: &str = "{author} - {book} - {year}";
/// Segment inserted before the track number when `disc_mode = "segment"` and a release has several discs.
pub const DEFAULT_DISC_SEGMENT: &str = "CD{disc} von {disctotal} - ";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Templates {
    pub album: String,
    pub compilation: String,
    pub audiobook: String,
    pub folder_enabled: bool,
    pub folder_album: String,
    pub folder_compilation: String,
    pub folder_audiobook: String,
    pub max_name_bytes: usize,
}
impl Default for Templates {
    fn default() -> Self {
        Self {
            album: DEFAULT_ALBUM.into(),
            compilation: DEFAULT_COMPILATION.into(),
            audiobook: DEFAULT_AUDIOBOOK.into(),
            folder_enabled: false,
            folder_album: DEFAULT_FOLDER_ALBUM.into(),
            folder_compilation: DEFAULT_FOLDER_COMPILATION.into(),
            folder_audiobook: DEFAULT_FOLDER_AUDIOBOOK.into(),
            max_name_bytes: 255,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DiscMode {
    /// Number tracks continuously across discs (14 of 27).
    #[default]
    Continuous,
    /// Keep per-disc numbers and insert a disc segment into the name.
    Segment,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Numbering {
    pub pad: bool,
    pub disc_mode: DiscMode,
    pub disc_segment: String,
}
impl Default for Numbering {
    fn default() -> Self {
        Self { pad: true, disc_mode: DiscMode::Continuous, disc_segment: DEFAULT_DISC_SEGMENT.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TagOptions {
    /// "2.4" or "2.3"
    pub id3_version: String,
    pub cover_max_px: u32,
    /// Foreign keys to keep (case-insensitive, `*` wildcard allowed), e.g. `REPLAYGAIN_*`.
    pub keep_fields: Vec<String>,
    /// Remove secondary containers (ID3v1, APE in MP3).
    pub remove_secondary_containers: bool,
    /// Replace an embedded cover that is smaller than this (px) when a better one is available.
    pub cover_min_px: u32,
}
impl Default for TagOptions {
    fn default() -> Self {
        Self { id3_version: "2.4".into(), cover_max_px: 1200, keep_fields: vec![], remove_secondary_containers: true, cover_min_px: 500 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sources {
    pub acoustid_key: String,
    pub discogs_enabled: bool,
    pub discogs_token: String,
    pub audiobookdb_key: String,
    pub musicbrainz_url: String,
    pub audnexus_url: String,
    pub audiobookdb_url: String,
    pub cache_days: u32,
}
impl Default for Sources {
    fn default() -> Self {
        Self {
            acoustid_key: String::new(),
            discogs_enabled: false,
            discogs_token: String::new(),
            audiobookdb_key: String::new(),
            musicbrainz_url: "https://musicbrainz.org/ws/2".into(),
            audnexus_url: "https://api.audnex.us".into(),
            audiobookdb_url: "https://audiobookdb.org/api".into(),
            cache_days: 30,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub templates: Templates,
    pub numbering: Numbering,
    pub tags: TagOptions,
    pub sources: Sources,
    /// Language for the UI ("" = system).
    pub language: String,
}

pub fn config_dir() -> PathBuf {
    if cfg!(windows) {
        if let Some(a) = std::env::var_os("APPDATA") {
            return PathBuf::from(a).join("tagjoe");
        }
    }
    if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(x).join("tagjoe");
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join(".config").join("tagjoe")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}
pub fn journal_dir() -> PathBuf {
    config_dir().join("journal")
}
pub fn cache_dir() -> PathBuf {
    if let Some(x) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(x).join("tagjoe");
    }
    config_dir().join("cache")
}

impl Config {
    pub fn load() -> Config {
        Self::load_from(&config_path()).unwrap_or_default()
    }
    pub fn load_from(path: &std::path::Path) -> Option<Config> {
        let text = std::fs::read_to_string(path).ok()?;
        toml::from_str(&text).ok()
    }
    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&config_path())
    }
    pub fn save_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| std::io::Error::other(e.to_string()))?;
        std::fs::write(path, text)
    }
    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let c = Config::default();
        let t = c.to_toml();
        let back: Config = toml::from_str(&t).unwrap();
        assert_eq!(c, back);
        assert!(t.contains("von {tracktotal}"));
    }
    #[test]
    fn partial_file_uses_defaults() {
        let c: Config = toml::from_str("[numbering]\ndisc_mode = \"segment\"\n").unwrap();
        assert_eq!(c.numbering.disc_mode, DiscMode::Segment);
        assert_eq!(c.templates.album, DEFAULT_ALBUM);
    }
}
