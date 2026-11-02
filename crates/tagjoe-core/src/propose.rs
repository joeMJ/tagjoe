//! Build a plan (per file: field changes, removals, cover, new name) from a unit and reference data.

use crate::classify::is_various;
use crate::config::{Config, DiscMode};
use crate::model::{Field, Kind, Unit};
use crate::naming::{self, Context, Template, TemplateError};
use crate::schema::{self, Status};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Reference data for one release: album-level fields plus one entry per track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ReleaseInfo {
    pub source: String,
    pub album: BTreeMap<Field, String>,
    pub tracks: Vec<TrackInfo>,
    pub cover: Option<CoverSource>,
    /// Confidence 0..=100 (100 = exact).
    pub score: u8,
    /// Short human description, e.g. "Anjunadeep 16 · Various Artists · 2026 · 41 tracks".
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TrackInfo {
    pub fields: BTreeMap<Field, String>,
    pub duration_secs: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum CoverSource {
    File { path: PathBuf },
    Url { url: String },
    Bytes { bytes: Vec<u8>, mime: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldChange {
    pub field: Field,
    pub old: String,
    pub new: String,
    pub status: Status,
    pub apply: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Removal {
    pub key: String,
    pub value: String,
    pub apply: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoverChange {
    pub source: CoverSource,
    pub width: u32,
    pub height: u32,
    pub bytes: usize,
    pub replaces_existing: bool,
    pub apply: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilePlan {
    pub path: PathBuf,
    pub file_name: String,
    /// New file name (with extension) if it differs.
    pub new_name: Option<String>,
    pub changes: Vec<FieldChange>,
    pub removals: Vec<Removal>,
    pub remove_containers: Vec<String>,
    pub cover: Option<CoverChange>,
    /// Index into `ReleaseInfo::tracks`, if matched.
    pub track_index: Option<usize>,
    pub apply: bool,
}

impl FilePlan {
    pub fn is_noop(&self) -> bool {
        self.new_name.is_none()
            && !self.changes.iter().any(|c| c.apply && c.status != Status::Ok)
            && !self.removals.iter().any(|r| r.apply)
            && self.remove_containers.is_empty()
            && self.cover.as_ref().map(|c| !c.apply).unwrap_or(true)
    }
    pub fn count(&self, s: Status) -> usize {
        self.changes.iter().filter(|c| c.status == s).count()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub unit: PathBuf,
    pub kind: Kind,
    pub source: String,
    pub files: Vec<FilePlan>,
    /// New folder path if folder renaming is on and the name differs.
    pub new_folder: Option<PathBuf>,
    pub notes: Vec<String>,
}

impl Plan {
    pub fn is_noop(&self) -> bool {
        self.new_folder.is_none() && self.files.iter().all(FilePlan::is_noop)
    }
}

/// Parse "3" or "3/12" into (number, total).
pub fn parse_num_pub(s: &str) -> (Option<u32>, Option<u32>) {
    parse_num(s)
}

fn parse_num(s: &str) -> (Option<u32>, Option<u32>) {
    let s = s.trim();
    let (a, b) = match s.split_once('/') {
        Some((a, b)) => (a.trim(), Some(b.trim())),
        None => (s, None),
    };
    let a: Option<u32> = a.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok();
    let b = b.and_then(|x| x.parse().ok());
    (a, b)
}

fn majority<'a>(values: impl Iterator<Item = &'a str>) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for v in values {
        let v = v.trim();
        if !v.is_empty() {
            *counts.entry(v).or_default() += 1;
        }
    }
    counts.into_iter().max_by_key(|(_, c)| *c).map(|(v, _)| v.to_string()).unwrap_or_default()
}

/// Title derived from a file name when the tag is empty: strip a leading number, underscores to spaces.
pub fn title_from_file_name(file_name: &str) -> String {
    let stem = file_name.rsplit_once('.').map(|(s, _)| s).unwrap_or(file_name);
    let mut s = stem.trim();
    let digits = s.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && digits <= 3 {
        let rest = &s[digits..];
        let rest = rest.trim_start_matches([' ', '-', '_', '.', ')']);
        if !rest.is_empty() {
            s = rest;
        }
    }
    s.replace('_', " ").trim().to_string()
}

/// Reference derived from the files themselves (no network): majority album values and
/// normalized numbers. Used as a fallback and as the base that sources override.
pub fn release_from_unit(unit: &Unit) -> ReleaseInfo {
    let kind = unit.kind;
    let mut album = BTreeMap::new();
    for f in Field::schema(kind).iter().filter(|f| f.is_album_level()) {
        let v = majority(unit.files.iter().map(|x| x.get(*f)));
        if !v.is_empty() {
            album.insert(*f, v);
        }
    }
    if kind == Kind::Compilation {
        let aa = album.get(&Field::AlbumArtist).cloned().unwrap_or_default();
        if aa.is_empty() || is_various(&aa) {
            album.insert(Field::AlbumArtist, "Various Artists".into());
        }
        album.insert(Field::Compilation, "1".into());
    } else if kind == Kind::Album && !album.contains_key(&Field::AlbumArtist) {
        let a = majority(unit.files.iter().map(|x| x.get(Field::Artist)));
        if !a.is_empty() {
            album.insert(Field::AlbumArtist, a);
        }
    } else if kind == Kind::Audiobook {
        let author = album.get(&Field::AlbumArtist).cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| majority(unit.files.iter().map(|x| x.get(Field::Artist))));
        if !author.is_empty() {
            album.insert(Field::AlbumArtist, author);
        }
    }
    // normalize album name from folder when missing
    album.entry(Field::Album).or_insert_with(|| unit.name());
    // discs
    let discs: Vec<(Option<u32>, Option<u32>)> = unit.files.iter().map(|f| parse_num(f.get(Field::DiscNumber))).collect();
    let disc_total = unit
        .files
        .iter()
        .filter_map(|f| parse_num(f.get(Field::DiscTotal)).0)
        .max()
        .or_else(|| discs.iter().filter_map(|d| d.1).max())
        .or_else(|| discs.iter().filter_map(|d| d.0).max())
        .unwrap_or(1)
        .max(1);
    album.insert(Field::DiscTotal, disc_total.to_string());
    // tracks: per disc totals
    let mut per_disc_count: BTreeMap<u32, u32> = BTreeMap::new();
    for (i, f) in unit.files.iter().enumerate() {
        let d = discs[i].0.unwrap_or(1).max(1);
        let _ = f;
        *per_disc_count.entry(d).or_default() += 1;
    }
    let mut tracks = Vec::new();
    for (i, f) in unit.files.iter().enumerate() {
        let mut t = BTreeMap::new();
        for field in Field::schema(kind).iter().filter(|f| !f.is_album_level()) {
            let v = f.get(*field);
            if !v.is_empty() {
                t.insert(*field, v.to_string());
            }
        }
        t.entry(Field::Title).or_insert_with(|| title_from_file_name(&f.file_name));
        if let (Some(a), false) = (album.get(&Field::AlbumArtist), kind == Kind::Compilation) {
            t.entry(Field::Artist).or_insert_with(|| a.clone());
        }
        let d = discs[i].0.unwrap_or(1).max(1);
        let (n, total_in_tag) = parse_num(f.get(Field::TrackNumber));
        let n = n.unwrap_or(i as u32 + 1);
        t.insert(Field::TrackNumber, n.to_string());
        // the folder is the truth when tags claim fewer tracks than there are files on that disc
        let on_disc = *per_disc_count.get(&d).unwrap_or(&1);
        let total = parse_num(f.get(Field::TrackTotal)).0.or(total_in_tag).unwrap_or(on_disc).max(on_disc).max(n);
        t.insert(Field::TrackTotal, total.to_string());
        t.insert(Field::DiscNumber, d.to_string());
        tracks.push(TrackInfo { fields: t, duration_secs: Some(f.duration_secs) });
    }
    let title = album.get(&Field::Album).cloned().unwrap_or_default();
    let artist = album.get(&Field::AlbumArtist).cloned().unwrap_or_default();
    let date = album.get(&Field::Date).cloned().unwrap_or_default();
    ReleaseInfo {
        source: "tags".into(),
        summary: format!("{title} · {artist} · {date} · {} files", unit.files.len()).replace(" ·  ·", " ·"),
        album,
        tracks,
        cover: unit.cover_files.first().map(|p| CoverSource::File { path: p.clone() }),
        score: 0,
    }
}

fn year_of(date: &str) -> String {
    date.chars().take_while(|c| c.is_ascii_digit()).take(4).collect()
}

fn pad_to(n: u32, width: usize) -> String {
    format!("{n:0width$}")
}

/// Compute continuous numbering (across discs) for a release: returns (position, total) per track index.
fn continuous_numbers(release: &ReleaseInfo) -> Vec<(u32, u32)> {
    let mut order: Vec<(u32, u32, usize)> = release
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| (parse_num(t.fields.get(&Field::DiscNumber).map(String::as_str).unwrap_or("1")).0.unwrap_or(1), parse_num(t.fields.get(&Field::TrackNumber).map(String::as_str).unwrap_or("0")).0.unwrap_or(0), i))
        .collect();
    order.sort();
    let total = order.len() as u32;
    let mut out = vec![(0u32, total); order.len()];
    for (pos, (_, _, i)) in order.iter().enumerate() {
        out[*i] = (pos as u32 + 1, total);
    }
    out
}

/// Build the template context for one file from the merged (expected) values.
pub fn context_for(kind: Kind, merged: &BTreeMap<Field, String>, ext: &str, numbering: (u32, u32, u32, u32), cfg: &Config) -> Context {
    let (track, track_total, disc, disc_total) = numbering;
    let mut c = Context::new();
    for f in Field::ALL {
        c.set(f.key(), merged.get(&f).cloned().unwrap_or_default());
    }
    let width = if cfg.numbering.pad { track_total.to_string().len().max(2) } else { 1 };
    let dwidth = if cfg.numbering.pad { disc_total.to_string().len() } else { 1 };
    c.set("track", pad_to(track, width));
    c.set("tracktotal", track_total.to_string());
    c.set("disc", pad_to(disc, dwidth));
    c.set("disctotal", disc_total.to_string());
    c.set("year", year_of(merged.get(&Field::Date).map(String::as_str).unwrap_or("")));
    let author = merged.get(&Field::AlbumArtist).or(merged.get(&Field::Artist)).cloned().unwrap_or_default();
    c.set("author", author);
    c.set("book", merged.get(&Field::Album).cloned().unwrap_or_default());
    c.set("chapter", pad_to(track, width));
    c.set("chaptertotal", track_total.to_string());
    c.set("ext", ext);
    c.set("kind", kind.label().to_lowercase());
    c
}

pub fn file_template(kind: Kind, cfg: &Config) -> &str {
    match kind {
        Kind::Compilation => &cfg.templates.compilation,
        Kind::Audiobook => &cfg.templates.audiobook,
        _ => &cfg.templates.album,
    }
}
pub fn folder_template(kind: Kind, cfg: &Config) -> &str {
    match kind {
        Kind::Compilation => &cfg.templates.folder_compilation,
        Kind::Audiobook => &cfg.templates.folder_audiobook,
        _ => &cfg.templates.folder_album,
    }
}

/// Render the file name for one context with the kind's template, handling the disc segment.
pub fn render_name(kind: Kind, ctx: &Context, disc_total: u32, cfg: &Config) -> Result<String, TemplateError> {
    let mut src = file_template(kind, cfg).to_string();
    if cfg.numbering.disc_mode == DiscMode::Segment && disc_total > 1 {
        // insert the disc segment before the first {track}/{chapter} placeholder
        let seg = &cfg.numbering.disc_segment;
        if let Some(p) = src.find("{track").or_else(|| src.find("{chapter")) {
            src.insert_str(p, seg);
        } else {
            src = format!("{seg}{src}");
        }
    }
    let t = Template::parse(&src)?;
    let ext = ctx.get("ext").map(|v| v.as_string()).unwrap_or_default();
    let stem = naming::render_file_stem(&t, ctx, &ext, cfg.templates.max_name_bytes)?;
    Ok(if ext.is_empty() { stem } else { format!("{stem}.{ext}") })
}

/// Build the plan for a unit against reference data. `matched[i]` is the track index for file i.
pub fn build_plan(unit: &Unit, release: &ReleaseInfo, matched: &[Option<usize>], cfg: &Config) -> Plan {
    let kind = unit.kind;
    let required = Field::required(kind);
    let schema = Field::schema(kind);
    let cont = continuous_numbers(release);
    let disc_total: u32 = release.album.get(&Field::DiscTotal).and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut notes = Vec::new();
    let mut files = Vec::new();
    let cover_dims = release.cover.as_ref().and_then(cover_dimensions);

    for (i, f) in unit.files.iter().enumerate() {
        let ti = matched.get(i).copied().flatten();
        let mut merged: BTreeMap<Field, String> = BTreeMap::new();
        // old values as base, then album-level, then track-level reference values
        for (k, v) in &f.fields {
            if schema.contains(k) {
                merged.insert(*k, v.clone());
            }
        }
        for (k, v) in &release.album {
            if schema.contains(k) {
                merged.insert(*k, v.clone());
            }
        }
        if let Some(t) = ti.and_then(|t| release.tracks.get(t)) {
            for (k, v) in &t.fields {
                if schema.contains(k) {
                    merged.insert(*k, v.clone());
                }
            }
        }
        // numbering
        let (mut track, mut track_total) = (parse_num(merged.get(&Field::TrackNumber).map(String::as_str).unwrap_or("")).0.unwrap_or(i as u32 + 1), parse_num(merged.get(&Field::TrackTotal).map(String::as_str).unwrap_or("")).0.unwrap_or(unit.files.len() as u32));
        let disc = parse_num(merged.get(&Field::DiscNumber).map(String::as_str).unwrap_or("1")).0.unwrap_or(1);
        if cfg.numbering.disc_mode == DiscMode::Continuous && disc_total > 1 {
            if let Some(t) = ti {
                (track, track_total) = cont[t];
                merged.insert(Field::TrackNumber, track.to_string());
                merged.insert(Field::TrackTotal, track_total.to_string());
                merged.insert(Field::DiscNumber, "1".into());
                merged.insert(Field::DiscTotal, "1".into());
            }
        } else {
            merged.insert(Field::TrackNumber, track.to_string());
            merged.insert(Field::TrackTotal, track_total.to_string());
            merged.insert(Field::DiscNumber, disc.to_string());
            merged.insert(Field::DiscTotal, disc_total.to_string());
        }
        let (disc, disc_total_eff) = if cfg.numbering.disc_mode == DiscMode::Continuous && disc_total > 1 { (1, 1) } else { (disc, disc_total) };

        // field changes
        let mut changes = Vec::new();
        for field in schema {
            let old = f.get(*field).trim().to_string();
            let new = merged.get(field).cloned().unwrap_or_default();
            let new = normalize_value(*field, &new);
            let old_n = normalize_value(*field, &old);
            let req = required.contains(field);
            // single-disc releases: do not add disc numbers that were not there
            if matches!(field, Field::DiscNumber | Field::DiscTotal) && old.is_empty() && disc_total <= 1 {
                continue;
            }
            if old.is_empty() && new.is_empty() {
                if req {
                    changes.push(FieldChange { field: *field, old, new, status: Status::Missing, apply: false });
                }
                continue;
            }
            let status = if old.is_empty() {
                Status::Missing
            } else if new.is_empty() || old_n == new {
                Status::Ok
            } else {
                Status::Differs
            };
            let apply = status != Status::Ok;
            let new = if new.is_empty() { old.clone() } else { new };
            changes.push(FieldChange { field: *field, old, new, status, apply });
        }

        // removals
        let (remove, _kept) = schema::foreign_items(f, kind, cfg);
        let removals = remove.into_iter().map(|r| Removal { key: r.key, value: r.value, apply: true }).collect();
        let remove_containers = if cfg.tags.remove_secondary_containers { f.extra_tag_types.clone() } else { vec![] };

        // cover
        let cover = {
            let existing = f.pictures.iter().filter(|p| p.pic_type == "CoverFront" || p.pic_type == "Other").map(|p| p.width.max(p.height)).max();
            match (&release.cover, cover_dims) {
                (Some(src), Some((w, h, bytes))) => {
                    let replaces = existing.is_some();
                    let better = existing.map(|e| e < cfg.tags.cover_min_px && w.max(h) > e).unwrap_or(true);
                    Some(CoverChange { source: src.clone(), width: w, height: h, bytes, replaces_existing: replaces, apply: better })
                }
                _ => None,
            }
        };

        // name
        let ctx = context_for(kind, &merged, &f.ext, (track, track_total, disc, disc_total_eff), cfg);
        let new_name = match render_name(kind, &ctx, disc_total, cfg) {
            Ok(n) => {
                if n == f.file_name { None } else { Some(n) }
            }
            Err(e) => {
                notes.push(format!("{}: name template error: {e}", f.file_name));
                None
            }
        };
        files.push(FilePlan { path: f.path.clone(), file_name: f.file_name.clone(), new_name, changes, removals, remove_containers, cover, track_index: ti, apply: true });
    }

    // folder
    let new_folder = if cfg.templates.folder_enabled && !unit.files.is_empty() {
        let merged: BTreeMap<Field, String> = release.album.iter().filter(|(k, _)| schema.contains(k)).map(|(k, v)| (*k, v.clone())).collect();
        let ctx = context_for(kind, &merged, "", (1, unit.files.len() as u32, 1, disc_total), cfg);
        match naming::render_folder_path(folder_template(kind, cfg), &ctx, cfg.templates.max_name_bytes) {
            Ok(segs) if !segs.is_empty() => {
                let mut p = unit.path.parent().map(PathBuf::from).unwrap_or_default();
                for s in segs {
                    p.push(s);
                }
                if p == unit.path { None } else { Some(p) }
            }
            Ok(_) => None,
            Err(e) => {
                notes.push(format!("folder template error: {e}"));
                None
            }
        }
    } else {
        None
    };

    // duplicate target names
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for fp in &files {
        let n = fp.new_name.clone().unwrap_or_else(|| fp.file_name.clone()).to_lowercase();
        *seen.entry(n).or_default() += 1;
    }
    for (n, c) in seen {
        if c > 1 {
            notes.push(format!("{c} files would get the same name: {n}"));
        }
    }

    Plan { unit: unit.path.clone(), kind, source: release.source.clone(), files, new_folder, notes }
}

/// Normalize values for comparison and writing (numbers without leading zeros, flags as 1/0).
pub fn normalize_value(field: Field, v: &str) -> String {
    let v = v.trim();
    match field {
        Field::TrackNumber | Field::TrackTotal | Field::DiscNumber | Field::DiscTotal | Field::SeriesPart => {
            parse_num(v).0.map(|n| n.to_string()).unwrap_or_else(|| v.to_string())
        }
        Field::Compilation => match v.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" => "1".into(),
            "" => String::new(),
            _ => "0".into(),
        },
        _ => v.to_string(),
    }
}

fn cover_dimensions(src: &CoverSource) -> Option<(u32, u32, usize)> {
    match src {
        CoverSource::File { path } => {
            let data = std::fs::read(path).ok()?;
            let (w, h) = crate::scan::image_dimensions(&data);
            Some((w, h, data.len()))
        }
        CoverSource::Bytes { bytes, .. } => {
            let (w, h) = crate::scan::image_dimensions(bytes);
            Some((w, h, bytes.len()))
        }
        CoverSource::Url { .. } => None,
    }
}

/// Identity matching: file i ↔ track i (used for the tag-derived reference).
pub fn identity_match(n: usize) -> Vec<Option<usize>> {
    (0..n).map(Some).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FileInfo, Unit};

    fn file(name: &str, fields: &[(Field, &str)]) -> FileInfo {
        FileInfo {
            path: PathBuf::from("/x").join(name),
            file_name: name.into(),
            ext: name.rsplit('.').next().unwrap().into(),
            format: "MP3".into(),
            duration_secs: 200.0,
            bitrate_kbps: None,
            sample_rate: None,
            bit_depth: None,
            channels: None,
            tag_type: Some("ID3v2".into()),
            extra_tag_types: vec!["ID3v1".into()],
            fields: fields.iter().map(|(k, v)| (*k, v.to_string())).collect(),
            foreign: vec![crate::model::RawItem { key: "COMMENT".into(), value: "Ripped by X".into() }],
            pictures: vec![],
        }
    }

    #[test]
    fn plan_from_tags_normalizes_and_renames() {
        let unit = Unit {
            path: PathBuf::from("/x"),
            kind: Kind::Album,
            files: vec![
                file("01-a.mp3", &[(Field::Title, "Alpha"), (Field::Artist, "Band"), (Field::Album, "Rec"), (Field::Date, "2020-01-01"), (Field::TrackNumber, "1/2")]),
                file("02-b.mp3", &[(Field::Title, "Beta"), (Field::Artist, "Band"), (Field::Album, "Rec"), (Field::Date, "2020"), (Field::TrackNumber, "2/2")]),
            ],
            cover_files: vec![],
            disc_folders: false,
            warnings: vec![],
        };
        let cfg = Config::default();
        let rel = release_from_unit(&unit);
        assert_eq!(rel.album.get(&Field::AlbumArtist).unwrap(), "Band");
        let plan = build_plan(&unit, &rel, &identity_match(2), &cfg);
        assert_eq!(plan.files[0].new_name.as_deref(), Some("Band - Rec - 2020 - 01 von 2 - Alpha.mp3"));
        assert_eq!(plan.files[1].new_name.as_deref(), Some("Band - Rec - 2020 - 02 von 2 - Beta.mp3"));
        let aa = plan.files[0].changes.iter().find(|c| c.field == Field::AlbumArtist).unwrap();
        assert_eq!((aa.status, aa.new.as_str()), (Status::Missing, "Band"));
        assert!(plan.files[0].removals.iter().any(|r| r.key == "COMMENT"));
        assert_eq!(plan.files[0].remove_containers, vec!["ID3v1".to_string()]);
        assert!(plan.files[0].changes.iter().any(|c| c.field == Field::Label && c.status == Status::Missing));
    }

    #[test]
    fn title_from_names() {
        assert_eq!(title_from_file_name("01-kloyd_and_fractures_will_we_ever_learn_(mixed).mp3"), "kloyd and fractures will we ever learn (mixed)");
        assert_eq!(title_from_file_name("12. Song.flac"), "Song");
        assert_eq!(title_from_file_name("2024.flac"), "2024");
    }

    #[test]
    fn compilation_and_segment_mode() {
        let mut cfg = Config::default();
        cfg.numbering.disc_mode = DiscMode::Segment;
        let unit = Unit {
            path: PathBuf::from("/x"),
            kind: Kind::Compilation,
            files: vec![
                file("a.flac", &[(Field::Title, "A"), (Field::Artist, "X"), (Field::Album, "Mix"), (Field::Date, "2019"), (Field::TrackNumber, "1"), (Field::DiscNumber, "1/2")]),
                file("b.flac", &[(Field::Title, "B"), (Field::Artist, "Y"), (Field::Album, "Mix"), (Field::Date, "2019"), (Field::TrackNumber, "1"), (Field::DiscNumber, "2/2")]),
            ],
            cover_files: vec![],
            disc_folders: false,
            warnings: vec![],
        };
        let rel = release_from_unit(&unit);
        let plan = build_plan(&unit, &rel, &identity_match(2), &cfg);
        assert_eq!(plan.files[1].new_name.as_deref(), Some("VA - Mix - 2019 - CD2 von 2 - 01 von 1 - Y - B.flac"));
        cfg.numbering.disc_mode = DiscMode::Continuous;
        let plan = build_plan(&unit, &rel, &identity_match(2), &cfg);
        assert_eq!(plan.files[1].new_name.as_deref(), Some("VA - Mix - 2019 - 02 von 2 - Y - B.flac"));
    }
}
