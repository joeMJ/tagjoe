//! Apply a plan: write tags, embed covers, remove foreign entries and secondary containers,
//! rename files and folders. Every run writes a journal that `undo` can replay.

use crate::config::Config;
use crate::containers::{self, atom_name, frame_entry, Container};
use crate::journal::{Journal, JournalEntry};
use crate::model::{Field, RawItem};
use crate::propose::{CoverSource, FilePlan, Plan};
use crate::scan::{self, custom_name, item_key, primary_tag_type};
use crate::schema::Status;
use lofty::ape::{ApeItem, ApeTag};
use lofty::config::WriteOptions;
use lofty::id3::v2::Id3v2Tag;
use lofty::mp4::{Atom, AtomData, AtomIdent, Ilst};
use lofty::ogg::OggPictureStorage;
use lofty::ogg::tag::VorbisComments;
use lofty::picture::{Picture, PictureType};
use lofty::prelude::*;
use lofty::tag::{ItemKey, ItemValue, MergeTag, SplitTag, Tag, TagType};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileResult {
    pub path: PathBuf,
    pub new_path: PathBuf,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ApplyResult {
    pub files: Vec<FileResult>,
    pub folder: Option<(PathBuf, PathBuf)>,
    pub journal: Option<PathBuf>,
    pub errors: usize,
    /// Folder files removed by the clean-up.
    #[serde(default)]
    pub deleted: Vec<PathBuf>,
}

/// One step of an apply run, reported after each file (and each deletion).
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    /// Steps finished so far.
    pub done: usize,
    /// Audio files to write plus files to delete.
    pub total: usize,
    pub path: PathBuf,
    pub ok: bool,
    pub message: String,
    /// False when reported right before a file is written (so the name shows at once),
    /// true once the step is done.
    pub finished: bool,
}

fn eq_name(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn write_options(cfg: &Config) -> WriteOptions {
    WriteOptions::new().use_id3v23(cfg.tags.id3_version.trim() == "2.3")
}

/// Load the primary container of a file (created empty if absent) plus the other containers present.
fn load_primary(path: &Path) -> Result<(Container, Vec<TagType>, lofty::file::FileType), String> {
    let loaded = containers::load(path)?;
    let primary = primary_tag_type(loaded.file_type);
    let others: Vec<TagType> = loaded.containers.iter().map(|c| c.tag_type()).filter(|t| *t != primary).collect();
    let c = loaded.containers.into_iter().find(|c| c.tag_type() == primary).unwrap_or_else(|| match primary {
        TagType::VorbisComments => Container::Vorbis(VorbisComments::default()),
        TagType::Mp4Ilst => Container::Ilst(Ilst::default()),
        TagType::Ape => Container::Ape(ApeTag::default()),
        _ => Container::Id3v2(Id3v2Tag::default()),
    });
    Ok((c, others, loaded.file_type))
}

/// Standard-field edit through lofty's generic `Tag`: split, edit, merge back.
fn edit_generic(c: Container, f: impl FnOnce(&mut Tag)) -> Container {
    match c {
        Container::Id3v2(t) => {
            let (rem, mut g) = t.split_tag();
            f(&mut g);
            Container::Id3v2(rem.merge_tag(g))
        }
        Container::Vorbis(t) => {
            let (rem, mut g) = t.split_tag();
            f(&mut g);
            Container::Vorbis(rem.merge_tag(g))
        }
        Container::Ilst(t) => {
            let (rem, mut g) = t.split_tag();
            f(&mut g);
            Container::Ilst(rem.merge_tag(g))
        }
        Container::Ape(t) => {
            let (rem, mut g) = t.split_tag();
            f(&mut g);
            Container::Ape(rem.merge_tag(g))
        }
        Container::Generic(mut g) => {
            f(&mut g);
            Container::Generic(g)
        }
        other => other,
    }
}

/// Set (or clear, when `value` is empty) one field in a container.
pub fn set_field(c: Container, field: Field, value: &str) -> Container {
    let value = value.trim().to_string();
    let tt = c.tag_type();
    let key = item_key(field);
    let narrator = field == Field::Narrator;
    let c = edit_generic(c, |g| {
        if let Some(k) = key {
            g.remove_key(k);
            if !value.is_empty() {
                g.insert_text(k, value.clone());
            }
        }
        if field == Field::Date {
            // a stale YEAR next to DATE would contradict the new value
            g.remove_key(ItemKey::Year);
        }
        if narrator {
            // Audiobookshelf / Plex read the narrator from the composer frame
            g.remove_key(ItemKey::Composer);
            if !value.is_empty() {
                g.insert_text(ItemKey::Composer, value.clone());
            }
        }
    });
    let Some(n) = custom_name(field, tt) else { return c };
    match c {
        Container::Id3v2(mut t) => {
            let desc = n.trim_start_matches("TXXX:").to_string();
            t.remove_user_text(&desc);
            if !value.is_empty() {
                t.insert_user_text(desc, value);
            }
            Container::Id3v2(t)
        }
        Container::Vorbis(mut t) => {
            let _ = t.remove(&n).count();
            if !value.is_empty() {
                t.insert(n, value);
            }
            Container::Vorbis(t)
        }
        Container::Ilst(mut t) => {
            let name = n.rsplit(':').next().unwrap_or("").to_string();
            let ident = AtomIdent::Freeform { mean: "com.apple.iTunes".into(), name: name.into() };
            let _ = t.remove(&ident).count();
            if !value.is_empty() {
                t.insert(Atom::new(ident, AtomData::UTF8(value)));
            }
            Container::Ilst(t)
        }
        Container::Ape(mut t) => {
            t.remove(&n);
            if !value.is_empty() {
                if let Ok(item) = ApeItem::new(n, ItemValue::Text(value)) {
                    t.insert(item);
                }
            }
            Container::Ape(t)
        }
        other => other,
    }
}

/// Remove every entry whose raw name equals `key` (case-insensitive).
pub fn remove_raw(c: Container, key: &str) -> Container {
    let tt = c.tag_type();
    let c = edit_generic(c, |g| g.retain(|item| !scan::key_name(item.key(), tt).map(|n| eq_name(&n, key)).unwrap_or(false)));
    match c {
        Container::Id3v2(mut t) => {
            t.retain(|f| !frame_entry(f).map(|(n, _)| eq_name(&n, key)).unwrap_or(false));
            Container::Id3v2(t)
        }
        Container::Vorbis(mut t) => {
            let _ = t.remove(key).count();
            Container::Vorbis(t)
        }
        Container::Ilst(mut t) => {
            t.retain(|a| !eq_name(&atom_name(a), key));
            Container::Ilst(t)
        }
        Container::Ape(mut t) => {
            t.remove(key);
            Container::Ape(t)
        }
        other => other,
    }
}

/// Insert a raw entry again (for undo). Returns false when the container cannot hold it.
pub fn insert_raw(c: Container, key: &str, value: &str) -> (Container, bool) {
    let tt = c.tag_type();
    if value.starts_with('<') && value.ends_with('>') {
        return (c, false); // binary placeholders cannot be restored
    }
    match c {
        Container::Id3v2(mut t) => {
            if let Some(desc) = key.strip_prefix("TXXX:") {
                t.insert_user_text(desc.to_string(), value.to_string());
                (Container::Id3v2(t), true)
            } else if key.starts_with("COMM") {
                (edit_generic(Container::Id3v2(t), |g| { g.insert_text(ItemKey::Comment, value.to_string()); }), true)
            } else if let Some(k) = ItemKey::from_key(TagType::Id3v2, key) {
                (edit_generic(Container::Id3v2(t), |g| { g.insert_text(k, value.to_string()); }), true)
            } else {
                (Container::Id3v2(t), false)
            }
        }
        Container::Vorbis(mut t) => {
            t.push(key.to_string(), value.to_string());
            (Container::Vorbis(t), true)
        }
        Container::Ilst(mut t) => {
            if let Some(rest) = key.strip_prefix("----:") {
                let (mean, name) = rest.split_once(':').unwrap_or(("com.apple.iTunes", rest));
                t.insert(Atom::new(AtomIdent::Freeform { mean: mean.to_string().into(), name: name.to_string().into() }, AtomData::UTF8(value.to_string())));
                (Container::Ilst(t), true)
            } else if let Some(k) = ItemKey::from_key(TagType::Mp4Ilst, key) {
                (edit_generic(Container::Ilst(t), |g| { g.insert_text(k, value.to_string()); }), true)
            } else {
                (Container::Ilst(t), false)
            }
        }
        Container::Ape(mut t) => match ApeItem::new(key.to_string(), ItemValue::Text(value.to_string())) {
            Ok(item) => {
                t.insert(item);
                (Container::Ape(t), true)
            }
            Err(_) => (Container::Ape(t), false),
        },
        Container::Generic(mut g) => match ItemKey::from_key(tt, key) {
            Some(k) => {
                g.insert_text(k, value.to_string());
                (Container::Generic(g), true)
            }
            None => (Container::Generic(g), false),
        },
        other => (other, false),
    }
}

/// Front cover bytes currently embedded (first CoverFront, else first Other).
pub fn current_cover(c: &Container) -> Option<Vec<u8>> {
    let g = c.to_generic();
    let pics = g.pictures();
    pics.iter().find(|p| p.pic_type() == PictureType::CoverFront).or_else(|| pics.iter().find(|p| p.pic_type() == PictureType::Other)).map(|p| p.data().to_vec())
}

/// Replace the front cover (or remove it when `bytes` is None).
pub fn set_cover(c: Container, bytes: Option<&[u8]>) -> Result<Container, String> {
    let make = |b: &[u8]| -> Result<Picture, String> {
        let mut p = Picture::from_reader(&mut std::io::Cursor::new(b)).map_err(|e| format!("cover image: {e}"))?;
        p.set_pic_type(PictureType::CoverFront);
        Ok(p)
    };
    Ok(match c {
        Container::Id3v2(mut t) => {
            t.remove_picture_type(PictureType::CoverFront);
            t.remove_picture_type(PictureType::Other);
            if let Some(b) = bytes {
                t.insert_picture(make(b)?);
            }
            Container::Id3v2(t)
        }
        Container::Vorbis(mut t) => {
            t.remove_picture_type(PictureType::CoverFront);
            t.remove_picture_type(PictureType::Other);
            if let Some(b) = bytes {
                t.insert_picture(make(b)?, None).map_err(|e| format!("cover: {e}"))?;
            }
            Container::Vorbis(t)
        }
        Container::Ilst(mut t) => {
            t.remove_pictures();
            if let Some(b) = bytes {
                t.insert_picture(make(b)?);
            }
            Container::Ilst(t)
        }
        Container::Ape(t) => Container::Ape(t),
        Container::Generic(mut g) => {
            g.remove_picture_type(PictureType::CoverFront);
            g.remove_picture_type(PictureType::Other);
            if let Some(b) = bytes {
                g.push_picture(make(b)?);
            }
            Container::Generic(g)
        }
        other => other,
    })
}

/// Save the container into the file.
pub fn save(c: &Container, path: &Path, opts: WriteOptions) -> Result<(), String> {
    let r = match c {
        Container::Id3v2(t) => t.save_to_path(path, opts),
        Container::Vorbis(t) => t.save_to_path(path, opts),
        Container::Ilst(t) => t.save_to_path(path, opts),
        Container::Ape(t) => t.save_to_path(path, opts),
        Container::Id3v1(t) => t.save_to_path(path, opts),
        Container::Generic(t) => t.save_to_path(path, opts),
    };
    r.map_err(|e| format!("write {}: {e}", path.display()))
}

/// Load cover bytes from a source and scale them down to `max_px` (longest edge) when larger.
pub fn cover_bytes(src: &CoverSource, max_px: u32) -> Result<Vec<u8>, String> {
    let data = match src {
        CoverSource::File { path } => std::fs::read(path).map_err(|e| format!("cover file {}: {e}", path.display()))?,
        CoverSource::Bytes { bytes, .. } => bytes.clone(),
        CoverSource::Url { url } => return Err(format!("cover URL not fetched: {url}")),
    };
    scale_cover(&data, max_px)
}

pub fn scale_cover(data: &[u8], max_px: u32) -> Result<Vec<u8>, String> {
    let (w, h) = scan::image_dimensions(data);
    if max_px == 0 || w.max(h) <= max_px || w == 0 {
        return Ok(data.to_vec());
    }
    let img = image::load_from_memory(data).map_err(|e| format!("cover decode: {e}"))?;
    let scale = max_px as f64 / w.max(h) as f64;
    let (nw, nh) = (((w as f64) * scale).round().max(1.0) as u32, ((h as f64) * scale).round().max(1.0) as u32);
    let resized = img.resize_exact(nw, nh, image::imageops::FilterType::Lanczos3);
    let mut out = std::io::Cursor::new(Vec::new());
    let rgb = resized.to_rgb8();
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90);
    rgb.write_with_encoder(enc).map_err(|e| format!("cover encode: {e}"))?;
    Ok(out.into_inner())
}

/// Waits between rename attempts (0.5 s, 1 s, 2 s): network shares (SMB) sometimes refuse a
/// rename for a moment – access denied, busy – although it succeeds a second later.
pub const RENAME_DELAYS_MS: [u64; 3] = [500, 1000, 2000];

/// Rename with retries; returns the number of attempts it took. Every error is retried once
/// per delay, so the caller sees an error only after `delays.len() + 1` attempts.
pub fn rename_retry(from: &Path, to: &Path, delays: &[u64]) -> Result<usize, std::io::Error> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(attempt),
            Err(_) if attempt <= delays.len() => std::thread::sleep(std::time::Duration::from_millis(delays[attempt - 1])),
            Err(e) => return Err(e),
        }
    }
}

fn attempts_note(attempts: usize) -> String {
    if attempts > 1 { format!(" (attempt {attempts})") } else { String::new() }
}

/// Unique target path: errors when a different file already exists there.
fn check_target(from: &Path, to: &Path) -> Result<(), String> {
    if to == from {
        return Ok(());
    }
    if to.exists() {
        let same = std::fs::canonicalize(from).ok() == std::fs::canonicalize(to).ok();
        if !same {
            return Err(format!("target exists: {}", to.display()));
        }
    }
    Ok(())
}

fn apply_file(fp: &FilePlan, cfg: &Config, journal_dir: &Path, run_id: &str, index: usize) -> Result<(JournalEntry, PathBuf, String), String> {
    let path = &fp.path;
    let (mut c, others, _ft) = load_primary(path)?;
    let mut entry = JournalEntry { path: path.clone(), new_path: path.clone(), fields: vec![], removed: vec![], containers: vec![], cover: None, cover_was_present: false, cover_changed: false };
    let mut actions = Vec::new();

    // fields
    for ch in fp.changes.iter().filter(|c| c.apply && c.status != Status::Ok) {
        entry.fields.push((ch.field.key().to_string(), ch.old.clone()));
        c = set_field(c, ch.field, &ch.new);
    }
    let nf = entry.fields.len();
    if nf > 0 {
        actions.push(format!("{nf} fields"));
    }
    // removals
    for r in fp.removals.iter().filter(|r| r.apply) {
        entry.removed.push(RawItem { key: r.key.clone(), value: r.value.clone() });
        c = remove_raw(c, &r.key);
    }
    if !entry.removed.is_empty() {
        actions.push(format!("{} removed", entry.removed.len()));
    }
    // cover
    if let Some(cv) = fp.cover.as_ref().filter(|c| c.apply) {
        let old = current_cover(&c);
        entry.cover_was_present = old.is_some();
        if let Some(old) = old {
            let p = journal_dir.join(format!("{run_id}-{index:03}-cover.bin"));
            std::fs::create_dir_all(journal_dir).map_err(|e| e.to_string())?;
            std::fs::write(&p, &old).map_err(|e| format!("journal cover: {e}"))?;
            entry.cover = Some(p);
        }
        let bytes = cover_bytes(&cv.source, cfg.tags.cover_max_px)?;
        c = set_cover(c, Some(&bytes))?;
        entry.cover_changed = true;
        actions.push("cover".into());
    }
    // write primary container
    let changed = nf > 0 || !entry.removed.is_empty() || entry.cover_changed;
    if changed {
        save(&c, path, write_options(cfg))?;
    }
    // secondary containers
    for name in &fp.remove_containers {
        let tt = match name.as_str() {
            "ID3v1" => Some(TagType::Id3v1),
            "APE" => Some(TagType::Ape),
            "ID3v2" => Some(TagType::Id3v2),
            _ => None,
        };
        if let Some(tt) = tt {
            if others.contains(&tt) {
                tt.remove_from_path(path, write_options(cfg)).map_err(|e| format!("remove {name}: {e}"))?;
                entry.containers.push(name.clone());
                actions.push(format!("{name} removed"));
            }
        }
    }
    // rename
    if let Some(n) = &fp.new_name {
        let to = path.parent().map(PathBuf::from).unwrap_or_default().join(n);
        check_target(path, &to)?;
        let attempts = rename_retry(path, &to, &RENAME_DELAYS_MS).map_err(|e| format!("rename: {e} (after {} attempts)", RENAME_DELAYS_MS.len() + 1))?;
        entry.new_path = to.clone();
        actions.push(format!("renamed{}", attempts_note(attempts)));
    }
    let new_path = entry.new_path.clone();
    Ok((entry, new_path, if actions.is_empty() { "nothing to do".into() } else { actions.join(", ") }))
}

/// Apply a plan. Writes a journal into `journal_dir` (unless nothing changed).
pub fn apply(plan: &Plan, cfg: &Config, journal_dir: &Path) -> ApplyResult {
    apply_with_progress(plan, cfg, journal_dir, &mut |_| {})
}

/// Like [`apply`], reporting every file through `progress` as soon as it is written.
/// Folder files on the clean-up list are deleted only when every audio file was written
/// without error; the deletions are reported too.
pub fn apply_with_progress(plan: &Plan, cfg: &Config, journal_dir: &Path, progress: &mut dyn FnMut(&Progress)) -> ApplyResult {
    let run_id = crate::journal::timestamp();
    let mut journal = Journal { created: run_id.clone(), unit: plan.unit.clone(), new_folder: None, entries: vec![], deleted: vec![] };
    let mut result = ApplyResult::default();
    let deletions: Vec<&PathBuf> = plan.delete_files.iter().filter(|d| d.apply).map(|d| &d.path).collect();
    let total = plan.files.iter().filter(|f| f.apply && !f.is_noop()).count() + deletions.len();
    let mut done = 0;
    for (i, fp) in plan.files.iter().enumerate() {
        if !fp.apply || fp.is_noop() {
            continue;
        }
        progress(&Progress { done, total, path: fp.path.clone(), ok: true, message: "writing".into(), finished: false });
        done += 1;
        match apply_file(fp, cfg, journal_dir, &run_id, i) {
            Ok((entry, new_path, msg)) => {
                journal.entries.push(entry);
                progress(&Progress { done, total, path: fp.path.clone(), ok: true, message: msg.clone(), finished: true });
                result.files.push(FileResult { path: fp.path.clone(), new_path, ok: true, message: msg });
            }
            Err(e) => {
                result.errors += 1;
                progress(&Progress { done, total, path: fp.path.clone(), ok: false, message: e.clone(), finished: true });
                result.files.push(FileResult { path: fp.path.clone(), new_path: fp.path.clone(), ok: false, message: e });
            }
        }
    }
    // folder clean-up: only after a clean run, and only files inside the unit folder
    if result.errors == 0 {
        for p in deletions {
            done += 1;
            let r = if !p.starts_with(&plan.unit) {
                Err("outside the folder".to_string())
            } else if crate::scan::is_audio(p) {
                Err("audio file".to_string())
            } else {
                std::fs::remove_file(p).map_err(|e| format!("delete: {e}"))
            };
            match r {
                Ok(()) => {
                    journal.deleted.push(p.clone());
                    result.deleted.push(p.clone());
                    progress(&Progress { done, total, path: p.clone(), ok: true, message: "deleted".into(), finished: true });
                    result.files.push(FileResult { path: p.clone(), new_path: p.clone(), ok: true, message: "deleted".into() });
                }
                Err(e) => {
                    result.errors += 1;
                    progress(&Progress { done, total, path: p.clone(), ok: false, message: e.clone(), finished: true });
                    result.files.push(FileResult { path: p.clone(), new_path: p.clone(), ok: false, message: e });
                }
            }
        }
    } else if !deletions.is_empty() {
        result.files.push(FileResult { path: plan.unit.clone(), new_path: plan.unit.clone(), ok: true, message: format!("{} file(s) not deleted because of errors", deletions.len()) });
    }
    if let Some(nf) = &plan.new_folder {
        match (|| -> Result<usize, String> {
            if let Some(parent) = nf.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            check_target(&plan.unit, nf)?;
            rename_retry(&plan.unit, nf, &RENAME_DELAYS_MS).map_err(|e| format!("rename folder: {e} (after {} attempts)", RENAME_DELAYS_MS.len() + 1))
        })() {
            Ok(attempts) => {
                journal.new_folder = Some(nf.clone());
                result.folder = Some((plan.unit.clone(), nf.clone()));
                if attempts > 1 {
                    result.files.push(FileResult { path: plan.unit.clone(), new_path: nf.clone(), ok: true, message: format!("folder renamed{}", attempts_note(attempts)) });
                }
            }
            Err(e) => {
                result.errors += 1;
                result.files.push(FileResult { path: plan.unit.clone(), new_path: nf.clone(), ok: false, message: e });
            }
        }
    }
    if !journal.entries.is_empty() || journal.new_folder.is_some() || !journal.deleted.is_empty() {
        match journal.save(journal_dir) {
            Ok(p) => result.journal = Some(p),
            Err(e) => {
                result.errors += 1;
                result.files.push(FileResult { path: plan.unit.clone(), new_path: plan.unit.clone(), ok: false, message: format!("journal: {e}") });
            }
        }
    }
    result
}

/// Undo a journal: restore names, fields, removed entries and covers (best effort).
pub fn undo(journal: &Journal, cfg: &Config) -> ApplyResult {
    let mut result = ApplyResult::default();
    if !journal.deleted.is_empty() {
        result.files.push(FileResult { path: journal.unit.clone(), new_path: journal.unit.clone(), ok: true, message: format!("{} deleted folder file(s) cannot be restored", journal.deleted.len()) });
    }
    // folder first (files live inside the renamed folder)
    let mut base_map: Option<(PathBuf, PathBuf)> = None;
    if let Some(nf) = &journal.new_folder {
        if nf.exists() {
            match rename_retry(nf, &journal.unit, &RENAME_DELAYS_MS) {
                Ok(_) => {
                    result.folder = Some((nf.clone(), journal.unit.clone()));
                    base_map = Some((nf.clone(), journal.unit.clone()));
                }
                Err(e) => {
                    result.errors += 1;
                    result.files.push(FileResult { path: nf.clone(), new_path: journal.unit.clone(), ok: false, message: format!("rename folder back: {e}") });
                }
            }
        }
    }
    let remap = |p: &Path| -> PathBuf {
        if let Some((from, to)) = &base_map {
            if let Ok(rest) = p.strip_prefix(from) {
                return to.join(rest);
            }
        }
        p.to_path_buf()
    };
    for e in journal.entries.iter().rev() {
        let current = remap(&e.new_path);
        let original = remap(&e.path);
        let r = (|| -> Result<String, String> {
            let mut actions = Vec::new();
            if !current.exists() {
                return Err(format!("missing: {}", current.display()));
            }
            if current != original {
                check_target(&current, &original)?;
                rename_retry(&current, &original, &RENAME_DELAYS_MS).map_err(|err| format!("rename back: {err}"))?;
                actions.push("name".into());
            }
            let p = original.as_path();
            if !e.fields.is_empty() || !e.removed.is_empty() || e.cover_changed || !e.containers.is_empty() {
                let (mut c, _, _) = load_primary(p)?;
                for (k, old) in &e.fields {
                    if let Some(f) = Field::from_key(k) {
                        c = set_field(c, f, old);
                    }
                }
                let mut skipped = 0;
                for r in &e.removed {
                    let (nc, ok) = insert_raw(c, &r.key, &r.value);
                    c = nc;
                    if !ok {
                        skipped += 1;
                    }
                }
                if e.cover_changed {
                    let old = match &e.cover {
                        Some(p) => Some(std::fs::read(p).map_err(|err| format!("journal cover: {err}"))?),
                        None => None,
                    };
                    c = set_cover(c, old.as_deref())?;
                    actions.push("cover".into());
                }
                save(&c, p, write_options(cfg))?;
                if !e.fields.is_empty() {
                    actions.push(format!("{} fields", e.fields.len()));
                }
                if !e.removed.is_empty() {
                    actions.push(format!("{} entries restored{}", e.removed.len() - skipped, if skipped > 0 { format!(", {skipped} binary skipped") } else { String::new() }));
                }
                if !e.containers.is_empty() {
                    actions.push(format!("{} not restored (secondary container)", e.containers.join(", ")));
                }
            }
            Ok(actions.join(", "))
        })();
        match r {
            Ok(msg) => result.files.push(FileResult { path: current, new_path: original, ok: true, message: msg }),
            Err(msg) => {
                result.errors += 1;
                result.files.push(FileResult { path: current, new_path: original, ok: false, message: msg });
            }
        }
    }
    result
}

#[cfg(test)]
mod rename_tests {
    use super::rename_retry;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("tagjoe-rename-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn succeeds_on_a_later_attempt() {
        let d = dir("later");
        let (from, to) = (d.join("a"), d.join("b"));
        let f = from.clone();
        // the source appears only after the first attempt has failed
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(60));
            std::fs::write(f, b"x").unwrap();
        });
        let attempts = rename_retry(&from, &to, &[40, 40, 40, 40, 40, 40]).unwrap();
        assert!(attempts > 1 && attempts <= 7, "attempts: {attempts}");
        assert!(to.exists() && !from.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn gives_up_after_the_last_delay() {
        let d = dir("giveup");
        let t = std::time::Instant::now();
        let err = rename_retry(&d.join("missing"), &d.join("b"), &[10, 10, 10]).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        assert!(t.elapsed() >= std::time::Duration::from_millis(30));
        let _ = std::fs::remove_dir_all(&d);
    }
}
