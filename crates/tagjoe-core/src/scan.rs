//! Read folders and audio files (tags, properties, pictures) with lofty.

use crate::model::{Field, FileInfo, Kind, PictureInfo, RawItem, Unit};
use crate::containers::{self, Container, STRUCTURAL};
use lofty::file::FileType;
use lofty::tag::{ItemKey, TagType};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const AUDIO_EXT: &[&str] = &["mp3", "flac", "m4a", "m4b", "mp4", "aac", "ogg", "oga", "opus", "wav", "aiff", "aif", "ape", "wv", "mpc"];
pub const IMAGE_EXT: &[&str] = &["jpg", "jpeg", "png"];

#[derive(Debug)]
pub enum ScanError {
    Io(std::io::Error),
    NotAFolder(PathBuf),
    Read { path: PathBuf, msg: String },
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::Io(e) => write!(f, "I/O error: {e}"),
            ScanError::NotAFolder(p) => write!(f, "not a folder: {}", p.display()),
            ScanError::Read { path, msg } => write!(f, "cannot read {}: {msg}", path.display()),
        }
    }
}
impl std::error::Error for ScanError {}
impl From<std::io::Error> for ScanError {
    fn from(e: std::io::Error) -> Self {
        ScanError::Io(e)
    }
}

pub fn is_audio(path: &Path) -> bool {
    ext_lower(path).map(|e| AUDIO_EXT.contains(&e.as_str())).unwrap_or(false)
}
pub fn is_image(path: &Path) -> bool {
    ext_lower(path).map(|e| IMAGE_EXT.contains(&e.as_str())).unwrap_or(false)
}
fn ext_lower(path: &Path) -> Option<String> {
    path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase())
}

fn is_disc_folder(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    for prefix in ["cd", "disc", "disk", "disc ", "cd ", "cd-", "disc-", "cd_", "disc_"] {
        if let Some(rest) = n.strip_prefix(prefix) {
            let rest = rest.trim_start_matches([' ', '-', '_', '.']);
            if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

/// Map the "standard" tag-type name for display.
pub fn tag_type_name(t: TagType) -> &'static str {
    match t {
        TagType::Ape => "APE",
        TagType::Id3v1 => "ID3v1",
        TagType::Id3v2 => "ID3v2",
        TagType::Mp4Ilst => "MP4",
        TagType::VorbisComments => "VorbisComments",
        TagType::RiffInfo => "RIFF INFO",
        TagType::AiffText => "AIFF",
        _ => "Other",
    }
}

fn file_type_name(t: FileType) -> &'static str {
    match t {
        FileType::Aac => "AAC",
        FileType::Aiff => "AIFF",
        FileType::Ape => "APE",
        FileType::Flac => "FLAC",
        FileType::Mpeg => "MP3",
        FileType::Mp4 => "MP4",
        FileType::Mpc => "MPC",
        FileType::Opus => "Opus",
        FileType::Vorbis => "Ogg Vorbis",
        FileType::Speex => "Speex",
        FileType::Wav => "WAV",
        FileType::WavPack => "WavPack",
        _ => "Other",
    }
}

/// Preferred (primary) container per file type, used both for reading and writing.
pub fn primary_tag_type(t: FileType) -> TagType {
    match t {
        FileType::Flac | FileType::Opus | FileType::Vorbis | FileType::Speex => TagType::VorbisComments,
        FileType::Mp4 => TagType::Mp4Ilst,
        FileType::Ape | FileType::WavPack | FileType::Mpc => TagType::Ape,
        FileType::Aiff => TagType::Id3v2,
        _ => TagType::Id3v2,
    }
}

/// Container-specific raw names for fields that lofty has no standard key for.
pub fn custom_name(field: Field, tag_type: TagType) -> Option<String> {
    let name = match field {
        Field::Narrator => "NARRATOR",
        Field::Series => "SERIES",
        Field::SeriesPart => "SERIES-PART",
        Field::Asin => "ASIN",
        _ => return None,
    };
    Some(match tag_type {
        TagType::Id3v2 => format!("TXXX:{name}"),
        TagType::Mp4Ilst => format!("----:com.apple.iTunes:{name}"),
        _ => name.to_string(),
    })
}

/// lofty key for a field in a given container (None for custom fields, see `custom_name`).
pub fn item_key(field: Field) -> Option<ItemKey> {
    Some(match field {
        Field::Title => ItemKey::TrackTitle,
        Field::Artist => ItemKey::TrackArtist,
        Field::AlbumArtist => ItemKey::AlbumArtist,
        Field::Album => ItemKey::AlbumTitle,
        Field::Date => ItemKey::RecordingDate,
        Field::TrackNumber => ItemKey::TrackNumber,
        Field::TrackTotal => ItemKey::TrackTotal,
        Field::DiscNumber => ItemKey::DiscNumber,
        Field::DiscTotal => ItemKey::DiscTotal,
        Field::Genre => ItemKey::Genre,
        Field::Label => ItemKey::Label,
        Field::CatalogNumber => ItemKey::CatalogNumber,
        Field::Barcode => ItemKey::Barcode,
        Field::Compilation => ItemKey::FlagCompilation,
        Field::MbReleaseId => ItemKey::MusicBrainzReleaseId,
        Field::MbReleaseGroupId => ItemKey::MusicBrainzReleaseGroupId,
        Field::MbRecordingId => ItemKey::MusicBrainzRecordingId,
        Field::MbArtistId => ItemKey::MusicBrainzArtistId,
        Field::MbAlbumArtistId => ItemKey::MusicBrainzReleaseArtistId,
        Field::Publisher => ItemKey::Publisher,
        Field::Description => ItemKey::Description,
        Field::Isrc => ItemKey::Isrc,
        Field::Narrator | Field::Series | Field::SeriesPart | Field::Asin => return None,
    })
}

/// Keys that also carry a field's value in some containers (read-only aliases).
fn alias_keys(field: Field) -> &'static [ItemKey] {
    match field {
        Field::Date => &[ItemKey::Year],
        // ID3 has one frame (TPUB) for label and publisher; lofty reads it as Publisher
        Field::Label => &[ItemKey::Publisher],
        // Audiobookshelf / Plex convention: narrator in composer
        Field::Narrator => &[ItemKey::Composer],
        _ => &[],
    }
}

/// Raw names that spell the same custom field (case-insensitive compare).
fn custom_aliases(field: Field, tag_type: TagType) -> Vec<String> {
    let mut v = Vec::new();
    if let Some(n) = custom_name(field, tag_type) {
        v.push(n);
    }
    match (field, tag_type) {
        (Field::Narrator, TagType::Id3v2) => v.push("TXXX:narrator".into()),
        (Field::Series, TagType::Id3v2) => v.extend(["TXXX:series".into(), "TXXX:SERIES".into()]),
        (Field::SeriesPart, TagType::Id3v2) => v.extend(["TXXX:series-part".into(), "TXXX:SERIESPART".into(), "TXXX:MVIN".into()]),
        (Field::Asin, TagType::Id3v2) => v.push("TXXX:ASIN".into()),
        (Field::Series, TagType::VorbisComments) => v.push("SERIES".into()),
        (Field::SeriesPart, TagType::VorbisComments) => v.push("SERIESPART".into()),
        (Field::Series, TagType::Mp4Ilst) => v.push("----:com.apple.iTunes:SERIES".into()),
        (Field::SeriesPart, TagType::Mp4Ilst) => v.push("----:com.apple.iTunes:SERIES-PART".into()),
        _ => {}
    }
    v
}

/// Raw name a lofty key has in a container (e.g. `TIT2`, `TITLE`, `©nam`).
pub fn key_name(key: ItemKey, tag_type: TagType) -> Option<String> {
    let name = key.map_key(tag_type)?;
    // lofty lists ID3v2 TXXX frames by their description only ("BARCODE", "MusicBrainz Album Id")
    if tag_type == TagType::Id3v2 && name.len() != 4 && !name.contains(':') {
        return Some(format!("TXXX:{name}"));
    }
    Some(name.to_string())
}

fn eq_name(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Read one container: canonical fields, foreign entries and pictures.
fn read_container(c: &Container, out: &mut FileInfo) {
    let tt = c.tag_type();
    let generic = c.to_generic();
    let raw = c.raw_entries();
    let mut claimed: Vec<String> = Vec::new();
    for field in Field::ALL {
        let mut val: Option<String> = None;
        if let Some(key) = item_key(field) {
            val = generic.get_string(key).map(str::to_string);
            if let Some(n) = key_name(key, tt) {
                claimed.push(n);
            }
            for alias in alias_keys(field) {
                if let Some(n) = key_name(*alias, tt) {
                    claimed.push(n);
                }
                if val.is_none() {
                    if let Some(s) = generic.get_string(*alias) {
                        val = Some(s.to_string());
                    }
                }
            }
        }
        for name in custom_aliases(field, tt) {
            if let Some((_, v)) = raw.iter().find(|(k, _)| eq_name(k, &name)) {
                if val.is_none() && !v.trim().is_empty() {
                    val = Some(v.clone());
                }
                claimed.push(name.clone());
            }
        }
        if let Some(v) = val {
            let v = v.trim().to_string();
            if !v.is_empty() {
                out.fields.entry(field).or_insert(v);
            }
        }
    }
    // numbers: lofty maps TRCK/TPOS to both number and total; the raw frame name is claimed via TrackNumber
    for (k, v) in raw {
        if STRUCTURAL.iter().any(|s| eq_name(s, &k)) {
            continue;
        }
        if claimed.iter().any(|c| eq_name(c, &k)) {
            continue;
        }
        out.foreign.push(RawItem { key: k, value: v });
    }
    for pic in generic.pictures() {
        let (w, h) = image_dimensions(pic.data());
        out.pictures.push(PictureInfo {
            pic_type: format!("{:?}", pic.pic_type()),
            mime: pic.mime_type().map(|m| m.to_string()).unwrap_or_default(),
            width: w,
            height: h,
            bytes: pic.data().len(),
        });
    }
}

pub fn image_dimensions(data: &[u8]) -> (u32, u32) {
    image::ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
        .unwrap_or((0, 0))
}

/// Read one audio file.
pub fn read_file(path: &Path) -> Result<FileInfo, ScanError> {
    let loaded = containers::load(path).map_err(|msg| ScanError::Read { path: path.to_path_buf(), msg })?;
    let props = &loaded.properties;
    let ft = loaded.file_type;
    let mut info = FileInfo {
        path: path.to_path_buf(),
        file_name: path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
        ext: ext_lower(path).unwrap_or_default(),
        format: file_type_name(ft).to_string(),
        duration_secs: props.duration().as_secs_f64(),
        bitrate_kbps: props.audio_bitrate(),
        sample_rate: props.sample_rate(),
        bit_depth: props.bit_depth(),
        channels: props.channels(),
        tag_type: None,
        extra_tag_types: Vec::new(),
        fields: BTreeMap::new(),
        foreign: Vec::new(),
        pictures: Vec::new(),
    };
    let primary = primary_tag_type(ft);
    let mut cs: Vec<&Container> = loaded.containers.iter().collect();
    cs.sort_by_key(|c| if c.tag_type() == primary { 0 } else { 1 });
    for c in cs {
        if c.tag_type() == primary {
            info.tag_type = Some(tag_type_name(primary).to_string());
            read_container(c, &mut info);
        } else {
            info.extra_tag_types.push(tag_type_name(c.tag_type()).to_string());
            // secondary containers only fill gaps; they are reported as a whole (removable container)
            let mut tmp = FileInfo { fields: BTreeMap::new(), foreign: Vec::new(), pictures: Vec::new(), ..info.clone() };
            read_container(c, &mut tmp);
            for (k, v) in tmp.fields {
                info.fields.entry(k).or_insert(v);
            }
            if info.pictures.is_empty() {
                info.pictures = tmp.pictures;
            }
        }
    }
    if info.tag_type.is_none() {
        info.tag_type = Some(tag_type_name(primary).to_string());
    }
    Ok(info)
}

fn list_dir(path: &Path) -> Result<(Vec<PathBuf>, Vec<PathBuf>), ScanError> {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            dirs.push(p);
        } else {
            files.push(p);
        }
    }
    files.sort();
    dirs.sort();
    Ok((files, dirs))
}

fn rank_cover(p: &Path) -> u8 {
    let n = p.file_stem().map(|s| s.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match n.as_str() {
        "cover" | "folder" | "front" => 0,
        _ if n.contains("cover") || n.contains("front") || n.contains("folder") => 1,
        _ => 2,
    }
}

/// Scan a folder. A folder with audio files is one unit (CD subfolders are merged);
/// otherwise each subfolder is scanned in turn.
pub fn scan_path(path: &Path) -> Result<Vec<Unit>, ScanError> {
    let mut units = Vec::new();
    scan_path_with(path, &mut |e| {
        if let ScanEvent::Unit(u) = e {
            units.push(u);
        }
    })?;
    Ok(units)
}

/// What a scan reports while it runs (for a progress display).
#[derive(Debug)]
pub enum ScanEvent {
    /// Audio files found by listing the folders (before any file is read).
    Total(usize),
    /// One file read: files done so far, total, path.
    File { done: usize, total: usize, path: PathBuf },
    /// A finished unit (album, compilation or audiobook folder).
    Unit(Unit),
}

/// Like [`scan_path`], but reports every file and hands over each unit as soon as it is read.
/// Counting first costs only directory listings, which are cheap next to reading tags over a
/// network share.
pub fn scan_path_with(path: &Path, report: &mut dyn FnMut(ScanEvent)) -> Result<(), ScanError> {
    if !path.is_dir() {
        return Err(ScanError::NotAFolder(path.to_path_buf()));
    }
    let total = count_audio(path);
    report(ScanEvent::Total(total));
    let mut done = 0;
    scan_into(path, &mut done, total, report)
}

/// Audio files under `path` (hidden entries skipped, like the scan itself).
fn count_audio(path: &Path) -> usize {
    let Ok((files, dirs)) = list_dir(path) else { return 0 };
    files.iter().filter(|p| is_audio(p)).count() + dirs.iter().map(|d| count_audio(d)).sum::<usize>()
}

fn scan_into(path: &Path, done: &mut usize, total: usize, report: &mut dyn FnMut(ScanEvent)) -> Result<(), ScanError> {
    let (files, dirs) = list_dir(path)?;
    let audio: Vec<&PathBuf> = files.iter().filter(|p| is_audio(p)).collect();
    let disc_dirs: Vec<&PathBuf> = dirs.iter().filter(|d| d.file_name().map(|n| is_disc_folder(&n.to_string_lossy())).unwrap_or(false)).collect();
    if !audio.is_empty() || !disc_dirs.is_empty() {
        let mut unit = Unit { path: path.to_path_buf(), kind: Kind::Unknown, files: Vec::new(), cover_files: Vec::new(), disc_folders: !disc_dirs.is_empty(), warnings: Vec::new() };
        let mut audio_paths: Vec<PathBuf> = audio.into_iter().cloned().collect();
        let mut images: Vec<PathBuf> = files.iter().filter(|p| is_image(p)).cloned().collect();
        for d in &disc_dirs {
            let (f2, _) = list_dir(d)?;
            audio_paths.extend(f2.iter().filter(|p| is_audio(p)).cloned());
            images.extend(f2.iter().filter(|p| is_image(p)).cloned());
        }
        for p in &audio_paths {
            match read_file(p) {
                Ok(info) => unit.files.push(info),
                Err(e) => unit.warnings.push(e.to_string()),
            }
            *done += 1;
            report(ScanEvent::File { done: *done, total, path: p.clone() });
        }
        images.sort_by_key(|p| (rank_cover(p), p.clone()));
        unit.cover_files = images;
        unit.kind = crate::classify::classify(&unit);
        report(ScanEvent::Unit(unit));
        // other subfolders (not disc folders) are scanned as their own units
        for d in dirs.iter().filter(|d| !disc_dirs.contains(d)) {
            scan_into(d, done, total, report)?;
        }
    } else {
        for d in &dirs {
            scan_into(d, done, total, report)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disc_folders() {
        for n in ["CD1", "cd 2", "Disc 03", "disk-1", "DISC_2"] {
            assert!(is_disc_folder(n), "{n}");
        }
        for n in ["CDs", "Disco", "1", "cd"] {
            assert!(!is_disc_folder(n), "{n}");
        }
    }
    #[test]
    fn extensions() {
        assert!(is_audio(Path::new("a.MP3")));
        assert!(is_audio(Path::new("b.flac")));
        assert!(!is_audio(Path::new("c.jpg")));
        assert!(is_image(Path::new("c.jpg")));
    }
}
