//! Format-specific tag containers: load them, list every raw entry (so nothing foreign hides),
//! and convert to lofty's generic `Tag` for the known fields.

use lofty::ape::ApeTag;
use lofty::config::ParseOptions;
use lofty::file::{AudioFile, FileType};
use lofty::id3::v1::Id3v1Tag;
use lofty::id3::v2::{Frame, Id3v2Tag};
use lofty::mp4::{AtomData, AtomIdent, Ilst};
use lofty::ogg::tag::VorbisComments;
use lofty::properties::FileProperties;
use lofty::tag::{ItemValue, Tag, TagType};
use std::fs::File;
use std::path::Path;

#[derive(Clone)]
pub enum Container {
    Id3v2(Id3v2Tag),
    Id3v1(Id3v1Tag),
    Ape(ApeTag),
    Vorbis(VorbisComments),
    Ilst(Ilst),
    /// RIFF INFO, AIFF text chunks: only the generic view is available.
    Generic(Tag),
}

/// Structural frames that are never reported as foreign (chapters, tables of contents).
pub const STRUCTURAL: &[&str] = &["CHAP", "CTOC", "APIC", "METADATA_BLOCK_PICTURE", "covr"];

#[derive(Clone)]
pub struct Loaded {
    pub file_type: FileType,
    pub properties: FileProperties,
    pub containers: Vec<Container>,
}

fn text(v: &ItemValue) -> String {
    match v {
        ItemValue::Text(s) | ItemValue::Locator(s) => s.clone(),
        ItemValue::Binary(b) => format!("<binary {} bytes>", b.len()),
    }
}

impl Container {
    pub fn tag_type(&self) -> TagType {
        match self {
            Container::Id3v2(_) => TagType::Id3v2,
            Container::Id3v1(_) => TagType::Id3v1,
            Container::Ape(_) => TagType::Ape,
            Container::Vorbis(_) => TagType::VorbisComments,
            Container::Ilst(_) => TagType::Mp4Ilst,
            Container::Generic(t) => t.tag_type(),
        }
    }

    pub fn to_generic(&self) -> Tag {
        match self {
            Container::Id3v2(t) => Tag::from(t.clone()),
            Container::Id3v1(t) => Tag::from(t.clone()),
            Container::Ape(t) => Tag::from(t.clone()),
            Container::Vorbis(t) => Tag::from(t.clone()),
            Container::Ilst(t) => Tag::from(t.clone()),
            Container::Generic(t) => t.clone(),
        }
    }

    /// Every entry of the container as (container-specific name, text), pictures excluded.
    pub fn raw_entries(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        match self {
            Container::Id3v2(tag) => {
                for frame in tag {
                    let id = frame.id().as_str().to_string();
                    match frame {
                        Frame::Text(f) => out.push((id, f.value.to_string())),
                        Frame::UserText(f) => out.push((format!("TXXX:{}", f.description), f.content.to_string())),
                        Frame::Comment(f) => {
                            let name = if f.description.is_empty() { "COMM".to_string() } else { format!("COMM:{}", f.description) };
                            out.push((name, f.content.to_string()))
                        }
                        Frame::UnsynchronizedText(f) => out.push(("USLT".into(), f.content.to_string())),
                        Frame::Url(f) => out.push((id, f.url().to_string())),
                        Frame::UserUrl(f) => out.push((format!("WXXX:{}", f.description), f.content.to_string())),
                        Frame::UniqueFileIdentifier(f) => out.push((format!("UFID:{}", f.owner), String::from_utf8_lossy(&f.identifier).to_string())),
                        Frame::Private(f) => out.push((format!("PRIV:{}", f.owner), format!("<binary {} bytes>", f.private_data.len()))),
                        Frame::KeyValue(f) => out.push((id, f.key_value_pairs.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("; "))),
                        Frame::Popularimeter(f) => out.push(("POPM".into(), format!("{} rating {}", f.email, f.rating))),
                        Frame::Picture(_) => {}
                        Frame::Chapter(_) => out.push(("CHAP".into(), "<chapter>".into())),
                        Frame::TableOfContents(_) => out.push(("CTOC".into(), "<table of contents>".into())),
                        Frame::Binary(f) => out.push((id, format!("<binary {} bytes>", f.data.len()))),
                        _ => out.push((id, "<data>".into())),
                    }
                }
            }
            Container::Id3v1(t) => {
                let mut push = |k: &str, v: Option<String>| {
                    if let Some(v) = v {
                        if !v.trim().is_empty() {
                            out.push((format!("ID3v1:{k}"), v));
                        }
                    }
                };
                push("title", t.title.clone());
                push("artist", t.artist.clone());
                push("album", t.album.clone());
                push("year", t.year.map(|y| y.to_string()));
                push("comment", t.comment.clone());
                push("track", t.track_number.map(|n| n.to_string()));
                push("genre", t.genre.map(|g| g.to_string()));
            }
            Container::Ape(t) => {
                for item in t {
                    out.push((item.key().to_string(), text(item.value())));
                }
            }
            Container::Vorbis(t) => {
                for (k, v) in t.items() {
                    let (k, v): (&str, &str) = (k, v);
                    out.push((k.to_string(), v.to_string()));
                }
            }
            Container::Ilst(t) => {
                for atom in t {
                    let name = match atom.ident() {
                        AtomIdent::Fourcc(b) => b.iter().map(|&c| c as char).collect::<String>(),
                        AtomIdent::Freeform { mean, name } => format!("----:{mean}:{name}"),
                    };
                    if STRUCTURAL.contains(&name.as_str()) {
                        continue;
                    }
                    let mut vals = Vec::new();
                    for d in atom.data() {
                        match d {
                            AtomData::UTF8(s) | AtomData::UTF16(s) => vals.push(s.clone()),
                            AtomData::SignedInteger(i) => vals.push(i.to_string()),
                            AtomData::Bool(b) => vals.push((*b as u8).to_string()),
                            AtomData::Picture(_) => {}
                            AtomData::Unknown { data, .. } => vals.push(format!("<binary {} bytes>", data.len())),
                            _ => vals.push("<data>".into()),
                        }
                    }
                    if !vals.is_empty() || atom.data().count() == 0 {
                        out.push((name, vals.join(" / ")));
                    }
                }
            }
            Container::Generic(t) => {
                let tt = t.tag_type();
                for item in t.items() {
                    let name = item.key().map_key(tt).map(str::to_string).unwrap_or_else(|| format!("{:?}", item.key()));
                    out.push((name, text(item.value())));
                }
            }
        }
        out
    }
}

macro_rules! load_as {
    ($ty:ty, $file:expr, $push:expr) => {{
        let f = <$ty>::read_from($file, ParseOptions::new()).map_err(|e| e.to_string())?;
        let props: FileProperties = f.properties().clone().into();
        #[allow(clippy::redundant_closure_call)]
        let containers = ($push)(&f);
        (props, containers)
    }};
}

/// Load a file with its format-specific tag containers (primary container first).
pub fn load(path: &Path) -> Result<Loaded, String> {
    let file_type = FileType::from_path(path).ok_or_else(|| "unknown file type".to_string())?;
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let file = &mut file;
    let (properties, containers): (FileProperties, Vec<Container>) = match file_type {
        FileType::Mpeg => load_as!(lofty::mpeg::MpegFile, file, |f: &lofty::mpeg::MpegFile| {
            let mut v = Vec::new();
            if let Some(t) = f.id3v2() { v.push(Container::Id3v2(t.clone())) }
            if let Some(t) = f.ape() { v.push(Container::Ape(t.clone())) }
            if let Some(t) = f.id3v1() { v.push(Container::Id3v1(t.clone())) }
            v
        }),
        FileType::Flac => load_as!(lofty::flac::FlacFile, file, |f: &lofty::flac::FlacFile| {
            let mut v = Vec::new();
            if let Some(t) = f.vorbis_comments() { v.push(Container::Vorbis(t.clone())) }
            if let Some(t) = f.id3v2() { v.push(Container::Id3v2(t.clone())) }
            v
        }),
        FileType::Mp4 => load_as!(lofty::mp4::Mp4File, file, |f: &lofty::mp4::Mp4File| {
            let mut v = Vec::new();
            if let Some(t) = f.ilst() { v.push(Container::Ilst(t.clone())) }
            v
        }),
        FileType::Opus => load_as!(lofty::ogg::OpusFile, file, |f: &lofty::ogg::OpusFile| vec![Container::Vorbis(f.vorbis_comments().clone())]),
        FileType::Vorbis => load_as!(lofty::ogg::VorbisFile, file, |f: &lofty::ogg::VorbisFile| vec![Container::Vorbis(f.vorbis_comments().clone())]),
        FileType::Speex => load_as!(lofty::ogg::SpeexFile, file, |f: &lofty::ogg::SpeexFile| vec![Container::Vorbis(f.vorbis_comments().clone())]),
        FileType::Ape => load_as!(lofty::ape::ApeFile, file, |f: &lofty::ape::ApeFile| {
            let mut v = Vec::new();
            if let Some(t) = f.ape() { v.push(Container::Ape(t.clone())) }
            if let Some(t) = f.id3v2() { v.push(Container::Id3v2(t.clone())) }
            if let Some(t) = f.id3v1() { v.push(Container::Id3v1(t.clone())) }
            v
        }),
        FileType::WavPack => load_as!(lofty::wavpack::WavPackFile, file, |f: &lofty::wavpack::WavPackFile| {
            let mut v = Vec::new();
            if let Some(t) = f.ape() { v.push(Container::Ape(t.clone())) }
            if let Some(t) = f.id3v1() { v.push(Container::Id3v1(t.clone())) }
            v
        }),
        FileType::Mpc => load_as!(lofty::musepack::MpcFile, file, |f: &lofty::musepack::MpcFile| {
            let mut v = Vec::new();
            if let Some(t) = f.ape() { v.push(Container::Ape(t.clone())) }
            if let Some(t) = f.id3v2() { v.push(Container::Id3v2(t.clone())) }
            if let Some(t) = f.id3v1() { v.push(Container::Id3v1(t.clone())) }
            v
        }),
        FileType::Aac => load_as!(lofty::aac::AacFile, file, |f: &lofty::aac::AacFile| {
            let mut v = Vec::new();
            if let Some(t) = f.id3v2() { v.push(Container::Id3v2(t.clone())) }
            if let Some(t) = f.id3v1() { v.push(Container::Id3v1(t.clone())) }
            v
        }),
        FileType::Wav => load_as!(lofty::iff::wav::WavFile, file, |f: &lofty::iff::wav::WavFile| {
            let mut v = Vec::new();
            if let Some(t) = f.id3v2() { v.push(Container::Id3v2(t.clone())) }
            if let Some(t) = f.riff_info() { v.push(Container::Generic(Tag::from(t.clone()))) }
            v
        }),
        FileType::Aiff => load_as!(lofty::iff::aiff::AiffFile, file, |f: &lofty::iff::aiff::AiffFile| {
            let mut v = Vec::new();
            if let Some(t) = f.id3v2() { v.push(Container::Id3v2(t.clone())) }
            if let Some(t) = f.text_chunks() { v.push(Container::Generic(Tag::from(t.clone()))) }
            v
        }),
        other => return Err(format!("unsupported file type {other:?}")),
    };
    Ok(Loaded { file_type, properties, containers })
}
