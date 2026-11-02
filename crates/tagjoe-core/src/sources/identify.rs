//! Identification pipeline for music: fingerprints → AcoustID → MusicBrainz releases,
//! fallback search by tags, then file ↔ track matching and scoring.

use super::acoustid::AcoustId;
use super::caa::Caa;
use super::fingerprint;
use super::http::Http;
use super::musicbrainz::MusicBrainz;
use super::{Progress, Result};
use crate::config::Config;
use crate::model::{Field, Kind, Unit};
use crate::propose::{CoverSource, ReleaseInfo};
use std::collections::BTreeMap;

/// One identified candidate with its file→track assignment.
#[derive(Debug, Clone, PartialEq)]
pub struct Identified {
    pub release: ReleaseInfo,
    pub matched: Vec<Option<usize>>,
    /// Files matched by fingerprint / by duration+title / unmatched.
    pub by_fingerprint: usize,
    pub by_heuristic: usize,
    pub notes: Vec<String>,
}

pub struct Identifier<'a> {
    pub http: &'a Http,
    pub cfg: &'a Config,
}

/// Normalize for fuzzy title comparison.
pub fn norm_title(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if c == '&' {
            out.push_str("and");
        }
    }
    out
}

/// Similarity 0..=1 from the longest common prefix/suffix and containment (cheap, good enough).
pub fn similarity(a: &str, b: &str) -> f64 {
    let a = norm_title(a);
    let b = norm_title(b);
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    if a.contains(&b) || b.contains(&a) {
        return 0.7 + 0.3 * (a.len().min(b.len()) as f64 / a.len().max(b.len()) as f64);
    }
    // bigram overlap
    let bg = |s: &str| -> Vec<(char, char)> {
        let c: Vec<char> = s.chars().collect();
        c.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let ba = bg(&a);
    let bb = bg(&b);
    if ba.is_empty() || bb.is_empty() {
        return 0.0;
    }
    let mut used = vec![false; bb.len()];
    let mut hits = 0;
    for x in &ba {
        if let Some(i) = bb.iter().enumerate().position(|(i, y)| !used[i] && y == x) {
            used[i] = true;
            hits += 1;
        }
    }
    2.0 * hits as f64 / (ba.len() + bb.len()) as f64
}

/// Assign files to tracks of a release. `rec_hits[i]` = recording ids AcoustID returned for file i.
pub fn match_tracks(unit: &Unit, release: &ReleaseInfo, rec_hits: &[Vec<String>]) -> (Vec<Option<usize>>, usize, usize) {
    let n = unit.files.len();
    let mut matched: Vec<Option<usize>> = vec![None; n];
    let mut taken = vec![false; release.tracks.len()];
    let mut by_fp = 0;
    // 1. recording ids
    for (i, hits) in rec_hits.iter().enumerate().take(n) {
        if hits.is_empty() {
            continue;
        }
        if let Some(t) = release.tracks.iter().enumerate().position(|(ti, t)| !taken[ti] && t.fields.get(&Field::MbRecordingId).map(|r| hits.contains(r)).unwrap_or(false)) {
            matched[i] = Some(t);
            taken[t] = true;
            by_fp += 1;
        }
    }
    // 2. existing MusicBrainz recording ids in tags
    for (i, f) in unit.files.iter().enumerate() {
        if matched[i].is_some() {
            continue;
        }
        let rid = f.get(Field::MbRecordingId);
        if rid.is_empty() {
            continue;
        }
        if let Some(t) = release.tracks.iter().enumerate().position(|(ti, t)| !taken[ti] && t.fields.get(&Field::MbRecordingId).map(|r| r == rid).unwrap_or(false)) {
            matched[i] = Some(t);
            taken[t] = true;
        }
    }
    // 3. duration + title (+ position) heuristics
    let mut by_heur = 0;
    for (i, f) in unit.files.iter().enumerate() {
        if matched[i].is_some() {
            continue;
        }
        let (tn, _) = crate::propose::parse_num_pub(f.get(Field::TrackNumber));
        let mut best: Option<(f64, usize)> = None;
        for (ti, t) in release.tracks.iter().enumerate() {
            if taken[ti] {
                continue;
            }
            let mut score = 0.0;
            if let Some(d) = t.duration_secs {
                let diff = (d - f.duration_secs).abs();
                score += if diff <= 3.0 { 0.5 } else if diff <= 10.0 { 0.25 } else if diff <= 30.0 { 0.05 } else { -0.5 };
            }
            let title_sim = similarity(f.get(Field::Title), t.fields.get(&Field::Title).map(String::as_str).unwrap_or(""));
            let name_sim = similarity(&crate::propose::title_from_file_name(&f.file_name), t.fields.get(&Field::Title).map(String::as_str).unwrap_or(""));
            score += 0.4 * title_sim.max(name_sim);
            let tpos: Option<u32> = t.fields.get(&Field::TrackNumber).and_then(|s| s.parse().ok());
            if tn.is_some() && tn == tpos && release.tracks.len() == n {
                score += 0.2;
            }
            if best.map(|(b, _)| score > b).unwrap_or(true) {
                best = Some((score, ti));
            }
        }
        if let Some((score, ti)) = best {
            if score >= 0.45 {
                matched[i] = Some(ti);
                taken[ti] = true;
                by_heur += 1;
            }
        }
    }
    (matched, by_fp, by_heur)
}

fn score_candidate(unit: &Unit, release: &ReleaseInfo, matched: &[Option<usize>], by_fp: usize) -> u8 {
    let n = unit.files.len().max(1) as f64;
    let m = matched.iter().filter(|x| x.is_some()).count() as f64;
    let mut s = 70.0 * m / n + 20.0 * by_fp as f64 / n;
    if release.tracks.len() == unit.files.len() {
        s += 10.0;
    } else {
        let diff = (release.tracks.len() as f64 - n).abs() / n;
        s -= (10.0 * diff).min(20.0);
    }
    s.clamp(0.0, 100.0).round() as u8
}

impl Identifier<'_> {
    /// Identify a music unit. Returns candidates sorted by score (best first).
    pub fn identify(&self, unit: &Unit, max_candidates: usize, progress: Progress<'_>) -> Result<Vec<Identified>> {
        let mut notes = Vec::new();
        let mb = MusicBrainz { http: self.http, base: &self.cfg.sources.musicbrainz_url };
        let n = unit.files.len();
        let mut rec_hits: Vec<Vec<String>> = vec![vec![]; n];
        let mut release_votes: BTreeMap<String, usize> = BTreeMap::new();

        // 1. fingerprints + AcoustID
        let key = self.cfg.sources.acoustid_key.trim();
        if fingerprint::available() && !key.is_empty() && !self.http.offline {
            let paths: Vec<_> = unit.files.iter().map(|f| f.path.clone()).collect();
            let fps = fingerprint::compute_all(&paths, 120, progress);
            let acoust = AcoustId { http: self.http, key };
            let mut ok = 0;
            let mut errors: BTreeMap<String, usize> = BTreeMap::new();
            let no_fp = fps.iter().filter(|f| f.is_none()).count();
            for (i, fp) in fps.iter().enumerate() {
                progress("acoustid", i + 1, n);
                let Some(fp) = fp else { continue };
                match acoust.lookup(&fp.fingerprint, fp.duration) {
                    Ok(matches) => {
                        for m in matches.iter().filter(|m| m.score >= 0.5) {
                            rec_hits[i].push(m.recording_id.clone());
                            for r in &m.release_ids {
                                *release_votes.entry(r.clone()).or_default() += 1;
                            }
                        }
                        if !matches.is_empty() {
                            ok += 1;
                        }
                    }
                    Err(e) => *errors.entry(e.short()).or_default() += 1,
                }
            }
            let mut line = format!("AcoustID: {ok} of {n} files recognized");
            if no_fp > 0 {
                line.push_str(&format!(", {no_fp} without fingerprint"));
            }
            for (e, c) in errors {
                line.push_str(&format!(", {c}× {e}"));
            }
            notes.push(line);
        } else if key.is_empty() {
            notes.push("AcoustID key not configured; using tag search only".into());
        } else if !fingerprint::available() {
            notes.push("fpcalc not found; using tag search only".into());
        }

        // 2. candidate releases: voted by AcoustID, plus search by tags/folder
        let mut candidates: Vec<(String, usize)> = release_votes.into_iter().collect();
        candidates.sort_by_key(|c| std::cmp::Reverse(c.1));
        candidates.truncate(8);
        let base = crate::propose::release_from_unit(unit);
        let album = base.album.get(&Field::Album).cloned().unwrap_or_default();
        let artist = base.album.get(&Field::AlbumArtist).cloned().unwrap_or_default();
        if candidates.len() < 3 && !self.http.offline {
            let mut found = Vec::new();
            for q in [(album.as_str(), artist.as_str(), Some(n)), (album.as_str(), artist.as_str(), None), (album.as_str(), "", None)] {
                if q.0.trim().is_empty() {
                    continue;
                }
                match mb.search(q.0, q.1, q.2, 8) {
                    Ok(c) => found.extend(c),
                    Err(e) => notes.push(format!("MusicBrainz search: {}", e.short())),
                }
                if found.len() >= 5 {
                    break;
                }
            }
            for c in found {
                if !candidates.iter().any(|(id, _)| *id == c.id) {
                    candidates.push((c.id, 0));
                }
            }
        }
        candidates.truncate(max_candidates.max(1) * 2);

        // 3. fetch, match, score
        let mut out = Vec::new();
        for (i, (id, _)) in candidates.iter().enumerate() {
            progress("musicbrainz", i + 1, candidates.len());
            let mut release = match mb.release(id) {
                Ok(r) => r,
                Err(e) => {
                    notes.push(format!("MusicBrainz release {id}: {}", e.short()));
                    continue;
                }
            };
            let (matched, by_fp, by_heur) = match_tracks(unit, &release, &rec_hits);
            release.score = score_candidate(unit, &release, &matched, by_fp);
            out.push(Identified { release, matched, by_fingerprint: by_fp, by_heuristic: by_heur, notes: vec![] });
        }
        // 4. Discogs (optional, needs a token)
        if self.cfg.sources.discogs_enabled && !self.cfg.sources.discogs_token.trim().is_empty() && !self.http.offline && !album.trim().is_empty() {
            let dc = super::discogs::Discogs { http: self.http, token: &self.cfg.sources.discogs_token };
            match dc.search(&album, &artist, 5) {
                Ok(hits) => {
                    for (i, h) in hits.iter().take(2).enumerate() {
                        progress("discogs", i + 1, hits.len().min(2));
                        match dc.release(h.id) {
                            Ok(mut release) => {
                                let (matched, by_fp, by_heur) = match_tracks(unit, &release, &rec_hits);
                                release.score = score_candidate(unit, &release, &matched, by_fp).saturating_sub(5);
                                out.push(Identified { release, matched, by_fingerprint: by_fp, by_heuristic: by_heur, notes: vec![] });
                            }
                            Err(e) => notes.push(format!("Discogs release {}: {}", h.id, e.short())),
                        }
                    }
                }
                Err(e) => notes.push(format!("Discogs search: {}", e.short())),
            }
        }
        out.sort_by_key(|c| std::cmp::Reverse(c.release.score));
        out.truncate(max_candidates.max(1));
        if let Some(first) = out.first_mut() {
            first.notes = notes;
        }
        Ok(out)
    }

    /// Attach cover bytes: resolve a `CoverSource::Url`, or fetch the Cover Art Archive front.
    pub fn fetch_cover(&self, release: &mut ReleaseInfo) -> Result<bool> {
        if let Some(CoverSource::Url { url }) = release.cover.clone() {
            let bytes = self.http.get_bytes(&url, &[], "img")?;
            let mime = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) { "image/png" } else { "image/jpeg" };
            release.cover = Some(CoverSource::Bytes { bytes, mime: mime.into() });
            return Ok(true);
        }
        let rid = release.album.get(&Field::MbReleaseId).cloned().unwrap_or_default();
        if rid.is_empty() {
            return Ok(false);
        }
        let rg = release.album.get(&Field::MbReleaseGroupId).cloned();
        let caa = Caa { http: self.http };
        let size = match self.cfg.tags.cover_max_px {
            0..=250 => 250,
            251..=500 => 500,
            _ => 1200,
        };
        if let Some((bytes, mime)) = caa.front(&rid, rg.as_deref(), size)? {
            release.cover = Some(CoverSource::Bytes { bytes, mime });
            return Ok(true);
        }
        Ok(false)
    }
}

/// Kind implied by a MusicBrainz release (compilation flag) when the unit kind is unknown.
pub fn kind_from_release(release: &ReleaseInfo, current: Kind) -> Kind {
    if current != Kind::Unknown {
        return current;
    }
    if release.album.get(&Field::Compilation).map(|c| c == "1").unwrap_or(false) { Kind::Compilation } else { Kind::Album }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn similarity_basics() {
        assert_eq!(similarity("Will We Ever Learn (Mixed)", "will_we_ever_learn_(mixed)"), 1.0);
        assert!(similarity("Wherever You Go", "Wherever You Go (Mixed)") > 0.7);
        assert!(similarity("Alpha", "Omega Song") < 0.4);
        assert_eq!(similarity("", "x"), 0.0);
    }
}
