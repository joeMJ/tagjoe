//! Audiobook identification: ASIN → Audnexus, else AudiobookDB title search; chapter mapping.

use super::audiobookdb::AudiobookDb;
use super::audnexus::{find_asin, looks_like_asin, Audnexus};
use super::http::Http;
use super::identify::{similarity, Identified};
use super::{Progress, Result};
use crate::config::Config;
use crate::model::{Field, Unit};
use crate::naming::{Context, Template};
use crate::propose::{self, ReleaseInfo, TrackInfo};
use std::collections::BTreeMap;

pub struct BookIdentifier<'a> {
    pub http: &'a Http,
    pub cfg: &'a Config,
}

/// Hints taken from tags and the folder structure (…/Author/Series/Book or …/Author/Book).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BookHints {
    pub title: String,
    pub author: String,
    pub series: String,
    pub asin: String,
}

fn clean_title(s: &str) -> String {
    // "Das Meisterwerk (Gabriel-Allon-Thriller - Packender …)" → "Das Meisterwerk"
    let t = s.split([':', '(', '[', '|']).next().unwrap_or(s).trim();
    t.to_string()
}

/// "Nesbo, Jo" → "Jo Nesbo"
fn normalize_author(s: &str) -> String {
    let s = s.trim();
    if let Some((last, first)) = s.split_once(',') {
        let (last, first) = (last.trim(), first.trim());
        if !first.is_empty() && !last.contains(' ') {
            return format!("{first} {last}");
        }
    }
    s.to_string()
}

pub fn hints(unit: &Unit) -> BookHints {
    let base = propose::release_from_unit(unit);
    let mut h = BookHints::default();
    h.title = clean_title(base.album.get(&Field::Album).map(String::as_str).unwrap_or(""));
    h.author = base.album.get(&Field::AlbumArtist).cloned().unwrap_or_default();
    h.series = base.album.get(&Field::Series).cloned().unwrap_or_default();
    h.asin = base.album.get(&Field::Asin).cloned().unwrap_or_default();
    // folder structure
    let parts: Vec<String> = unit.path.iter().map(|p| p.to_string_lossy().to_string()).collect();
    let n = parts.len();
    if n >= 2 && (h.author.is_empty() || h.author.eq_ignore_ascii_case("unknown")) {
        let parent = &parts[n - 2];
        let grand = if n >= 3 { parts[n - 3].as_str() } else { "" };
        let is_series = |s: &str| {
            let l = s.to_lowercase();
            l.starts_with("serie") || l.starts_with("series") || l.starts_with("reihe")
        };
        if is_series(parent) && !grand.is_empty() {
            h.author = normalize_author(grand);
            h.series = parent.trim_start_matches(|c: char| c.is_alphabetic()).trim().to_string();
            if h.series.is_empty() {
                h.series = parent.clone();
            }
        } else if parent.contains(',') || parent.split_whitespace().count() <= 3 {
            h.author = normalize_author(parent);
        }
    } else {
        h.author = normalize_author(&h.author);
    }
    if h.asin.is_empty() {
        for text in [unit.name()].into_iter().chain(unit.files.iter().map(|f| f.file_name.clone())) {
            if let Some(a) = find_asin(&text) {
                h.asin = a;
                break;
            }
        }
        if h.asin.is_empty() {
            // NFO / TXT files next to the audio often carry the ASIN
            if let Ok(rd) = std::fs::read_dir(&unit.path) {
                for e in rd.flatten() {
                    let p = e.path();
                    let ext = p.extension().map(|x| x.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                    if ext == "nfo" || ext == "txt" {
                        if let Ok(t) = std::fs::read_to_string(&p) {
                            if let Some(a) = find_asin(&t) {
                                h.asin = a;
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    if h.title.is_empty() {
        h.title = clean_title(&unit.name());
    }
    h
}

fn score_book(unit: &Unit, hints: &BookHints, release: &ReleaseInfo) -> u8 {
    let title = release.album.get(&Field::Album).map(String::as_str).unwrap_or("");
    let author = release.album.get(&Field::AlbumArtist).map(String::as_str).unwrap_or("");
    let mut s = 60.0 * similarity(&hints.title, &clean_title(title));
    s += 25.0 * if hints.author.is_empty() { 0.5 } else { similarity(&hints.author, author) };
    // runtime: sum of chapter lengths vs files
    let rt: f64 = release.tracks.iter().filter_map(|t| t.duration_secs).sum();
    if rt > 0.0 {
        let diff = (rt - unit.total_secs()).abs() / unit.total_secs().max(1.0);
        s += if diff < 0.03 { 15.0 } else if diff < 0.10 { 8.0 } else if diff > 0.5 { -20.0 } else { 0.0 };
    } else {
        s += 5.0;
    }
    // a different author is a different book, whatever the title says
    if !hints.author.is_empty() && !author.is_empty() && similarity(&hints.author, author) < 0.4 {
        s = s.min(40.0);
    }
    s.clamp(0.0, 100.0).round() as u8
}

/// Map the files of a unit onto chapters: 1:1 when the counts match, otherwise sequential with
/// titles kept from the files (or generated from the config template).
pub fn map_chapters(unit: &Unit, release: &mut ReleaseInfo, cfg: &Config) -> Vec<Option<usize>> {
    let n = unit.files.len();
    let exact = release.tracks.len() == n && n > 0;
    let tmpl = Template::parse(&cfg.audiobook.chapter_title).ok();
    let width = n.to_string().len().max(2);
    let book_title = release.album.get(&Field::Album).cloned().unwrap_or_default();
    let generated = |i: usize| -> String {
        let mut c = Context::new();
        c.set("chapter", format!("{:0width$}", i + 1)).set("chaptertotal", n.to_string());
        tmpl.as_ref().and_then(|t| t.render(&c).ok()).unwrap_or_else(|| format!("Chapter {}", i + 1))
    };
    let is_generic = |t: &str| t.trim().is_empty() || crate::classify::looks_like_chapter_pub(t) || similarity(t, &book_title) > 0.8;
    if exact {
        // 1:1 – keep source chapter titles, but replace generic ones (from tags or empty source titles)
        for (i, t) in release.tracks.iter_mut().enumerate() {
            let title = t.fields.get(&Field::Title).cloned().unwrap_or_default();
            if is_generic(&title) {
                t.fields.insert(Field::Title, generated(i));
            }
            t.fields.insert(Field::TrackNumber, (i + 1).to_string());
            t.fields.insert(Field::TrackTotal, n.to_string());
        }
    } else {
        // counts differ: one track per file, titles kept from the files unless generic
        let mut tracks = Vec::with_capacity(n);
        for (i, f) in unit.files.iter().enumerate() {
            let mut fields: BTreeMap<Field, String> = BTreeMap::new();
            fields.insert(Field::TrackNumber, (i + 1).to_string());
            fields.insert(Field::TrackTotal, n.to_string());
            let existing = f.get(Field::Title).trim().to_string();
            fields.insert(Field::Title, if is_generic(&existing) { generated(i) } else { existing });
            tracks.push(TrackInfo { fields, duration_secs: Some(f.duration_secs) });
        }
        release.tracks = tracks;
    }
    propose::identity_match(n)
}

impl BookIdentifier<'_> {
    pub fn identify(&self, unit: &Unit, max_candidates: usize, progress: Progress<'_>) -> Result<Vec<Identified>> {
        let h = hints(unit);
        let mut notes = vec![format!("hints: title \"{}\", author \"{}\"{}{}", h.title, h.author, if h.series.is_empty() { String::new() } else { format!(", series \"{}\"", h.series) }, if h.asin.is_empty() { String::new() } else { format!(", ASIN {}", h.asin) })];
        let mut found: Vec<ReleaseInfo> = Vec::new();
        if self.http.offline {
            return Ok(vec![]);
        }
        // 1. ASIN → Audnexus
        if looks_like_asin(&h.asin) {
            progress("audnexus", 1, 1);
            let an = Audnexus { http: self.http, base: &self.cfg.sources.audnexus_url };
            match an.release_info(&h.asin, &self.cfg.audiobook.audnexus_regions) {
                Ok(Some(r)) => found.push(r),
                Ok(None) => notes.push(format!("Audnexus: ASIN {} not found in regions {:?}", h.asin, self.cfg.audiobook.audnexus_regions)),
                Err(e) => notes.push(format!("Audnexus: {e}")),
            }
        }
        // 2. AudiobookDB title search
        let db = AudiobookDb { http: self.http, base: &self.cfg.sources.audiobookdb_url, key: &self.cfg.sources.audiobookdb_key };
        match db.search(&h.title, 10) {
            Ok(hits) => {
                let mut scored: Vec<(f64, _)> = hits
                    .into_iter()
                    .map(|hit| {
                        let ts = similarity(&h.title, &clean_title(&hit.title));
                        let as_ = if h.author.is_empty() || hit.authors.is_empty() { 0.5 } else { hit.authors.iter().map(|a| similarity(&h.author, a)).fold(0.0, f64::max) };
                        (ts * 0.7 + as_ * 0.3, hit)
                    })
                    .filter(|(s, _)| *s >= 0.4)
                    .collect();
                scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                let total = scored.len().min(max_candidates.max(1));
                for (i, (_, hit)) in scored.into_iter().take(total).enumerate() {
                    progress("audiobookdb", i + 1, total);
                    match db.release_info(&hit.id, &self.cfg.audiobook.language) {
                        Ok(Some(r)) => found.push(r),
                        Ok(None) => {}
                        Err(e) => notes.push(format!("AudiobookDB {}: {e}", hit.id)),
                    }
                }
                if total == 0 {
                    notes.push(format!("AudiobookDB: no match for \"{}\"", h.title));
                }
            }
            Err(e) => notes.push(format!("AudiobookDB search: {e}")),
        }
        // 3. follow an ASIN found via AudiobookDB to Audnexus (richer, regional data)
        if !looks_like_asin(&h.asin) {
            let asins: Vec<String> = found.iter().filter_map(|r| r.album.get(&Field::Asin).cloned()).filter(|a| looks_like_asin(a)).collect();
            let an = Audnexus { http: self.http, base: &self.cfg.sources.audnexus_url };
            for a in asins.into_iter().take(2) {
                if let Ok(Some(r)) = an.release_info(&a, &self.cfg.audiobook.audnexus_regions) {
                    found.push(r);
                }
            }
        }
        let mut out: Vec<Identified> = found
            .into_iter()
            .map(|mut r| {
                r.score = score_book(unit, &h, &r);
                let matched = map_chapters(unit, &mut r, self.cfg);
                Identified { release: r, matched, by_fingerprint: 0, by_heuristic: unit.files.len(), notes: vec![] }
            })
            .collect();
        out.sort_by_key(|c| std::cmp::Reverse(c.release.score));
        out.truncate(max_candidates.max(1));
        if let Some(first) = out.first_mut() {
            first.notes = notes;
        } else {
            // no candidate: still return the notes through an empty-score tag-derived entry? No – notes are returned via Err-free path below.
            let mut base = propose::release_from_unit(unit);
            base.score = 0;
            base.source = "tags".into();
            let matched = map_chapters(unit, &mut base, self.cfg);
            out.push(Identified { release: base, matched, by_fingerprint: 0, by_heuristic: 0, notes });
        }
        Ok(out)
    }
}

/// Offline reference for an audiobook: tags/folder hints plus generated chapter titles.
pub fn offline_release(unit: &Unit, cfg: &Config) -> (ReleaseInfo, Vec<Option<usize>>) {
    let h = hints(unit);
    let mut base = propose::release_from_unit(unit);
    if !h.author.is_empty() {
        base.album.insert(Field::AlbumArtist, h.author.clone());
        base.album.entry(Field::Artist).or_insert(h.author.clone());
    }
    if !h.series.is_empty() {
        base.album.entry(Field::Series).or_insert(h.series.clone());
    }
    if !h.title.is_empty() {
        base.album.insert(Field::Album, h.title.clone());
    }
    base.album.entry(Field::Genre).or_insert_with(|| "Audiobook".into());
    let matched = map_chapters(unit, &mut base, cfg);
    (base, matched)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn author_and_title_normalization() {
        assert_eq!(normalize_author("Nesbo, Jo"), "Jo Nesbo");
        assert_eq!(normalize_author("Daniel Silva"), "Daniel Silva");
        assert_eq!(clean_title("Das Meisterwerk (Gabriel-Allon-Thriller - Packender Spionagethriller)"), "Das Meisterwerk");
        assert_eq!(clean_title("Der Sohn: Kriminalroman"), "Der Sohn");
    }
}
