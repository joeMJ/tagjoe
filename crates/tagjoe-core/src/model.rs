//! Data model: kinds, canonical fields, scanned files and units (release folders).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// What a folder holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Album,
    Compilation,
    Audiobook,
    Unknown,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Album => "Album",
            Kind::Compilation => "Compilation",
            Kind::Audiobook => "Audiobook",
            Kind::Unknown => "Unknown",
        }
    }
    pub fn parse(s: &str) -> Option<Kind> {
        match s.to_ascii_lowercase().as_str() {
            "album" => Some(Kind::Album),
            "compilation" | "sampler" | "va" => Some(Kind::Compilation),
            "audiobook" | "hoerbuch" | "hörbuch" | "book" => Some(Kind::Audiobook),
            "unknown" => Some(Kind::Unknown),
            _ => None,
        }
    }
}

/// Canonical tag fields. Everything outside this set is "foreign".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Title,
    Artist,
    AlbumArtist,
    Album,
    Date,
    TrackNumber,
    TrackTotal,
    DiscNumber,
    DiscTotal,
    Genre,
    Label,
    CatalogNumber,
    Barcode,
    Compilation,
    MbReleaseId,
    MbReleaseGroupId,
    MbRecordingId,
    MbArtistId,
    MbAlbumArtistId,
    Narrator,
    Series,
    SeriesPart,
    Publisher,
    Asin,
    Description,
    Isrc,
}

impl Field {
    pub const ALL: [Field; 26] = [
        Field::Title,
        Field::Artist,
        Field::AlbumArtist,
        Field::Album,
        Field::Date,
        Field::TrackNumber,
        Field::TrackTotal,
        Field::DiscNumber,
        Field::DiscTotal,
        Field::Genre,
        Field::Label,
        Field::CatalogNumber,
        Field::Barcode,
        Field::Compilation,
        Field::MbReleaseId,
        Field::MbReleaseGroupId,
        Field::MbRecordingId,
        Field::MbArtistId,
        Field::MbAlbumArtistId,
        Field::Narrator,
        Field::Series,
        Field::SeriesPart,
        Field::Publisher,
        Field::Asin,
        Field::Description,
        Field::Isrc,
    ];

    /// Stable key used in config, JSON and templates.
    pub fn key(self) -> &'static str {
        match self {
            Field::Title => "title",
            Field::Artist => "artist",
            Field::AlbumArtist => "albumartist",
            Field::Album => "album",
            Field::Date => "date",
            Field::TrackNumber => "track",
            Field::TrackTotal => "tracktotal",
            Field::DiscNumber => "disc",
            Field::DiscTotal => "disctotal",
            Field::Genre => "genre",
            Field::Label => "label",
            Field::CatalogNumber => "catalog",
            Field::Barcode => "barcode",
            Field::Compilation => "compilation",
            Field::MbReleaseId => "mb_release_id",
            Field::MbReleaseGroupId => "mb_release_group_id",
            Field::MbRecordingId => "mb_recording_id",
            Field::MbArtistId => "mb_artist_id",
            Field::MbAlbumArtistId => "mb_albumartist_id",
            Field::Narrator => "narrator",
            Field::Series => "series",
            Field::SeriesPart => "part",
            Field::Publisher => "publisher",
            Field::Asin => "asin",
            Field::Description => "description",
            Field::Isrc => "isrc",
        }
    }

    pub fn from_key(k: &str) -> Option<Field> {
        Field::ALL.iter().copied().find(|f| f.key() == k)
    }

    /// Human label (English base language; translated in the UI).
    pub fn label(self, kind: Kind) -> &'static str {
        let book = kind == Kind::Audiobook;
        match self {
            Field::Title => {
                if book { "Chapter title" } else { "Title" }
            }
            Field::Artist => {
                if book { "Author" } else { "Artist" }
            }
            Field::AlbumArtist => {
                if book { "Author (album)" } else { "Album artist" }
            }
            Field::Album => {
                if book { "Book" } else { "Album" }
            }
            Field::Date => "Date",
            Field::TrackNumber => {
                if book { "Chapter" } else { "Track" }
            }
            Field::TrackTotal => {
                if book { "Chapters" } else { "Tracks" }
            }
            Field::DiscNumber => "Disc",
            Field::DiscTotal => "Discs",
            Field::Genre => "Genre",
            Field::Label => "Label",
            Field::CatalogNumber => "Catalog number",
            Field::Barcode => "Barcode",
            Field::Compilation => "Compilation",
            Field::MbReleaseId => "MusicBrainz release",
            Field::MbReleaseGroupId => "MusicBrainz release group",
            Field::MbRecordingId => "MusicBrainz recording",
            Field::MbArtistId => "MusicBrainz artist",
            Field::MbAlbumArtistId => "MusicBrainz album artist",
            Field::Narrator => "Narrator",
            Field::Series => "Series",
            Field::SeriesPart => "Series part",
            Field::Publisher => "Publisher",
            Field::Asin => "ASIN",
            Field::Description => "Description",
            Field::Isrc => "ISRC",
        }
    }

    /// Fields that belong to the schema of a kind (all others are foreign for that kind).
    pub fn schema(kind: Kind) -> &'static [Field] {
        const MUSIC: &[Field] = &[
            Field::Title,
            Field::Artist,
            Field::AlbumArtist,
            Field::Album,
            Field::Date,
            Field::TrackNumber,
            Field::TrackTotal,
            Field::DiscNumber,
            Field::DiscTotal,
            Field::Genre,
            Field::Label,
            Field::CatalogNumber,
            Field::Barcode,
            Field::Compilation,
            Field::MbReleaseId,
            Field::MbReleaseGroupId,
            Field::MbRecordingId,
            Field::MbArtistId,
            Field::MbAlbumArtistId,
            Field::Isrc,
        ];
        const BOOK: &[Field] = &[
            Field::Title,
            Field::Artist,
            Field::AlbumArtist,
            Field::Album,
            Field::Date,
            Field::TrackNumber,
            Field::TrackTotal,
            Field::DiscNumber,
            Field::DiscTotal,
            Field::Genre,
            Field::Narrator,
            Field::Series,
            Field::SeriesPart,
            Field::Publisher,
            Field::Asin,
            Field::Description,
            Field::Barcode,
            Field::CatalogNumber,
            Field::Isrc,
        ];
        match kind {
            Kind::Audiobook => BOOK,
            Kind::Unknown => &Field::ALL,
            _ => MUSIC,
        }
    }

    /// Fields that must be present for a file of this kind to count as complete.
    pub fn required(kind: Kind) -> &'static [Field] {
        const MUSIC: &[Field] = &[
            Field::Title,
            Field::Artist,
            Field::AlbumArtist,
            Field::Album,
            Field::Date,
            Field::TrackNumber,
            Field::TrackTotal,
            Field::Genre,
            Field::Label,
            Field::MbReleaseId,
            Field::MbRecordingId,
        ];
        const COMP: &[Field] = &[
            Field::Title,
            Field::Artist,
            Field::AlbumArtist,
            Field::Album,
            Field::Date,
            Field::TrackNumber,
            Field::TrackTotal,
            Field::Genre,
            Field::Label,
            Field::Compilation,
            Field::MbReleaseId,
            Field::MbRecordingId,
        ];
        const BOOK: &[Field] = &[
            Field::Title,
            Field::Artist,
            Field::AlbumArtist,
            Field::Album,
            Field::Date,
            Field::TrackNumber,
            Field::TrackTotal,
            Field::Genre,
            Field::Narrator,
        ];
        match kind {
            Kind::Album => MUSIC,
            Kind::Compilation => COMP,
            Kind::Audiobook => BOOK,
            Kind::Unknown => &[],
        }
    }

    /// Fields shared by every file of a unit (album level).
    pub fn is_album_level(self) -> bool {
        !matches!(self, Field::Title | Field::Artist | Field::TrackNumber | Field::MbRecordingId | Field::MbArtistId | Field::Isrc)
    }
}

/// A tag item outside the schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawItem {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PictureInfo {
    pub pic_type: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub bytes: usize,
}

/// One scanned audio file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: PathBuf,
    pub file_name: String,
    pub ext: String,
    pub format: String,
    pub duration_secs: f64,
    pub bitrate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u8>,
    pub channels: Option<u8>,
    /// Primary tag container (e.g. "ID3v2", "VorbisComments").
    pub tag_type: Option<String>,
    /// Other tag containers found in the file (e.g. "ID3v1", "APE").
    pub extra_tag_types: Vec<String>,
    pub fields: BTreeMap<Field, String>,
    pub foreign: Vec<RawItem>,
    pub pictures: Vec<PictureInfo>,
}

impl FileInfo {
    pub fn get(&self, f: Field) -> &str {
        self.fields.get(&f).map(String::as_str).unwrap_or("")
    }
    pub fn has_cover(&self) -> bool {
        self.pictures.iter().any(|p| p.pic_type == "CoverFront" || p.pic_type == "Other")
    }
}

/// A folder that holds one release (album, compilation or audiobook).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Unit {
    pub path: PathBuf,
    pub kind: Kind,
    pub files: Vec<FileInfo>,
    /// Image files found next to the audio (cover.jpg, folder.jpg, …), best first.
    pub cover_files: Vec<PathBuf>,
    /// True when the audio lives in CD1/CD2 subfolders.
    pub disc_folders: bool,
    pub warnings: Vec<String>,
}

impl Unit {
    pub fn name(&self) -> String {
        self.path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
    }
    pub fn total_secs(&self) -> f64 {
        self.files.iter().map(|f| f.duration_secs).sum()
    }
}
