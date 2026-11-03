//! Configuration (TOML) in the user's config directory.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Album: the track artist is added only when it differs from the album artist (DJ mixes, guests).
pub const DEFAULT_ALBUM: &str = "{albumartist} - {album} - {year} - {track} von {tracktotal} - {artist != albumartist ? artist + ' - ' : ''}{title}";
pub const DEFAULT_COMPILATION: &str = "VA - {album} - {year} - {track} von {tracktotal} - {artist} - {title}";
pub const DEFAULT_AUDIOBOOK: &str = "{author} - {book} - {year} - {chapter} von {chaptertotal} - {title}";
pub const DEFAULT_FOLDER_ALBUM: &str = "{albumartist} - {album} - {year}";
pub const DEFAULT_FOLDER_COMPILATION: &str = "{album} - {year}";
pub const DEFAULT_FOLDER_AUDIOBOOK: &str = "{author} - {book}";
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
            folder_enabled: true,
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
    /// Only used when `cover_best` is off.
    pub cover_min_px: u32,
    /// Always check the cover and embed the best one: files without a cover get it, an existing
    /// cover is replaced when the found one is larger.
    pub cover_best: bool,
}
impl Default for TagOptions {
    fn default() -> Self {
        Self { id3_version: "2.4".into(), cover_max_px: 1200, keep_fields: vec![], remove_secondary_containers: true, cover_min_px: 500, cover_best: true }
    }
}

/// Files removed from the folder by Apply and Clean (after every audio file was written without error).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FolderClean {
    pub enabled: bool,
    pub nfo: bool,
    /// .m3u and .m3u8
    pub m3u: bool,
    /// .jpg, .jpeg, .png (the folder cover is embedded first)
    pub images: bool,
    pub txt: bool,
    /// Further extensions without the dot.
    pub extra: Vec<String>,
}
impl Default for FolderClean {
    fn default() -> Self {
        Self { enabled: true, nfo: true, m3u: true, images: true, txt: true, extra: ["cue", "sfv", "log", "url", "db", "ini"].iter().map(|s| s.to_string()).collect() }
    }
}
impl FolderClean {
    /// Lower-case extensions to remove, or empty when the clean-up is off.
    pub fn extensions(&self) -> Vec<String> {
        if !self.enabled {
            return vec![];
        }
        let mut v: Vec<String> = Vec::new();
        if self.nfo {
            v.push("nfo".into());
        }
        if self.m3u {
            v.extend(["m3u".to_string(), "m3u8".to_string()]);
        }
        if self.images {
            v.extend(["jpg".to_string(), "jpeg".to_string(), "png".to_string()]);
        }
        if self.txt {
            v.push("txt".into());
        }
        for e in &self.extra {
            let e = e.trim().trim_start_matches('.').to_ascii_lowercase();
            if !e.is_empty() && !v.contains(&e) {
                v.push(e);
            }
        }
        v
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
/// Online sources. Every one is off until the user switches it on (privacy: nothing leaves the
/// computer by default). With everything off tagjoe works from tags and folder names only.
pub struct Sources {
    pub acoustid_enabled: bool,
    pub acoustid_key: String,
    pub musicbrainz_enabled: bool,
    /// Cover Art Archive (covers for MusicBrainz releases).
    pub caa_enabled: bool,
    pub discogs_enabled: bool,
    pub discogs_token: String,
    pub audiobookdb_enabled: bool,
    pub audiobookdb_key: String,
    pub audnexus_enabled: bool,
    /// Deutsche Nationalbibliothek (German audiobooks).
    pub dnb_enabled: bool,
    /// Open Library covers by ISBN (for DNB results).
    pub openlibrary_enabled: bool,
    pub musicbrainz_url: String,
    pub audnexus_url: String,
    pub audiobookdb_url: String,
    pub cache_days: u32,
}
impl Default for Sources {
    fn default() -> Self {
        Self {
            acoustid_enabled: false,
            acoustid_key: String::new(),
            musicbrainz_enabled: false,
            caa_enabled: false,
            discogs_enabled: false,
            discogs_token: String::new(),
            audiobookdb_enabled: false,
            audiobookdb_key: String::new(),
            audnexus_enabled: false,
            dnb_enabled: false,
            openlibrary_enabled: false,
            musicbrainz_url: "https://musicbrainz.org/ws/2".into(),
            audnexus_url: "https://api.audnex.us".into(),
            audiobookdb_url: "https://audiobookdb.org/api".into(),
            cache_days: 30,
        }
    }
}
impl Sources {
    /// True when at least one online source is switched on.
    pub fn any_enabled(&self) -> bool {
        self.acoustid_enabled || self.musicbrainz_enabled || self.caa_enabled || self.discogs_enabled || self.audiobookdb_enabled || self.audnexus_enabled || self.dnb_enabled || self.openlibrary_enabled
    }
    /// Sources useful for music (albums, compilations).
    pub fn music_enabled(&self) -> bool {
        self.acoustid_enabled || self.musicbrainz_enabled || self.discogs_enabled
    }
    /// Sources useful for audiobooks.
    pub fn book_enabled(&self) -> bool {
        self.audiobookdb_enabled || self.audnexus_enabled || self.musicbrainz_enabled || self.dnb_enabled
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Audiobook {
    /// Title for chapters without a name; `{chapter}` and `{chaptertotal}` are available.
    pub chapter_title: String,
    /// Audible regions to try on Audnexus, in order.
    pub audnexus_regions: Vec<String>,
    /// Preferred release language on AudiobookDB (release pick when a book has several).
    pub language: String,
}
impl Default for Audiobook {
    fn default() -> Self {
        Self { chapter_title: "Kapitel {chapter}".into(), audnexus_regions: vec!["de".into()], language: "German".into() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub audiobook: Audiobook,
    pub templates: Templates,
    pub numbering: Numbering,
    pub tags: TagOptions,
    pub sources: Sources,
    pub clean: FolderClean,
    /// Language for the UI ("" = system).
    pub language: String,
    /// GUI: drop a folder from the queue once it was written without errors.
    pub remove_done_from_queue: bool,
    /// Ask GitHub for a newer release (at most once a day); off by default.
    pub update_check: bool,
    /// Unix time of the last update check.
    pub update_last_check: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self { audiobook: Audiobook::default(), templates: Templates::default(), numbering: Numbering::default(), tags: TagOptions::default(), sources: Sources::default(), clean: FolderClean::default(), language: String::new(), remove_done_from_queue: true, update_check: false, update_last_check: 0 }
    }
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
        assert!(!c.sources.any_enabled(), "every online source is off by default");
        assert!(!c.update_check);
        assert!(c.remove_done_from_queue && c.templates.folder_enabled);
    }
    #[test]
    fn partial_file_uses_defaults() {
        let c: Config = toml::from_str("[numbering]\ndisc_mode = \"segment\"\n").unwrap();
        assert_eq!(c.numbering.disc_mode, DiscMode::Segment);
        assert_eq!(c.templates.album, DEFAULT_ALBUM);
    }
}
