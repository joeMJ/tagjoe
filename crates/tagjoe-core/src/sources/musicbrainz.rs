//! MusicBrainz: release search and release lookup → `ReleaseInfo`.

use super::http::{enc, Http};
use super::Result;
use crate::model::Field;
use crate::propose::{ReleaseInfo, TrackInfo};
use std::collections::BTreeMap;

pub const VARIOUS_ARTISTS_ID: &str = "89ad4ac3-39f7-470e-963a-56509c546377";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Candidate {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub date: String,
    pub track_count: u32,
    pub country: String,
    pub status: String,
    pub format: String,
    pub score: u32,
}

pub struct MusicBrainz<'a> {
    pub http: &'a Http,
    pub base: &'a str,
}

fn s(v: &serde_json::Value, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

/// Join an artist-credit array into one display string ("A feat. B").
pub fn artist_credit(v: &serde_json::Value) -> (String, Vec<String>) {
    let mut name = String::new();
    let mut ids = Vec::new();
    for ac in v.as_array().into_iter().flatten() {
        name.push_str(ac.get("name").and_then(|n| n.as_str()).unwrap_or(""));
        name.push_str(ac.get("joinphrase").and_then(|n| n.as_str()).unwrap_or(""));
        if let Some(id) = ac.get("artist").and_then(|a| a.get("id")).and_then(|i| i.as_str()) {
            ids.push(id.to_string());
        }
    }
    (name.trim().to_string(), ids)
}

impl MusicBrainz<'_> {
    /// Search releases by album title, artist and (optionally) track count.
    pub fn search(&self, album: &str, artist: &str, tracks: Option<usize>, limit: u32) -> Result<Vec<Candidate>> {
        let mut q = String::new();
        if !album.trim().is_empty() {
            q.push_str(&format!("release:\"{}\"", album.replace('"', "")));
        }
        if !artist.trim().is_empty() && !crate::classify::is_various(artist) {
            if !q.is_empty() {
                q.push_str(" AND ");
            }
            q.push_str(&format!("artist:\"{}\"", artist.replace('"', "")));
        }
        if let Some(t) = tracks {
            if !q.is_empty() {
                q.push_str(" AND ");
            }
            q.push_str(&format!("tracks:{t}"));
        }
        if q.is_empty() {
            return Ok(vec![]);
        }
        let url = format!("{}/release/?query={}&fmt=json&limit={limit}", self.base, enc(&q));
        let v = self.http.get_json(&url, &[])?;
        Ok(parse_search(&v))
    }

    /// Full release with media, tracks, recordings, artists, labels, genres.
    pub fn release(&self, id: &str) -> Result<ReleaseInfo> {
        let url = format!("{}/release/{id}?inc=recordings+artist-credits+labels+release-groups+media+isrcs+genres&fmt=json", self.base);
        let v = self.http.get_json(&url, &[])?;
        Ok(parse_release(&v))
    }

    /// Releases a recording appears on (for AcoustID hits without release ids).
    pub fn recording_releases(&self, recording_id: &str) -> Result<Vec<String>> {
        let url = format!("{}/recording/{recording_id}?inc=releases&fmt=json", self.base);
        let v = self.http.get_json(&url, &[])?;
        Ok(v.get("releases").and_then(|r| r.as_array()).into_iter().flatten().filter_map(|r| r.get("id").and_then(|i| i.as_str()).map(str::to_string)).collect())
    }
}

pub fn parse_search(v: &serde_json::Value) -> Vec<Candidate> {
    let mut out = Vec::new();
    for r in v.get("releases").and_then(|r| r.as_array()).into_iter().flatten() {
        let (artist, _) = artist_credit(r.get("artist-credit").unwrap_or(&serde_json::Value::Null));
        let format = r.get("media").and_then(|m| m.as_array()).and_then(|m| m.first()).map(|m| s(m, "format")).unwrap_or_default();
        out.push(Candidate {
            id: s(r, "id"),
            title: s(r, "title"),
            artist,
            date: s(r, "date"),
            track_count: r.get("track-count").and_then(|t| t.as_u64()).unwrap_or(0) as u32,
            country: s(r, "country"),
            status: s(r, "status"),
            format,
            score: r.get("score").and_then(|t| t.as_u64()).unwrap_or(0) as u32,
        });
    }
    out
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

pub fn parse_release(v: &serde_json::Value) -> ReleaseInfo {
    let mut album: BTreeMap<Field, String> = BTreeMap::new();
    let id = s(v, "id");
    album.insert(Field::MbReleaseId, id.clone());
    album.insert(Field::Album, s(v, "title"));
    let date = s(v, "date");
    if !date.is_empty() {
        album.insert(Field::Date, date.clone());
    }
    let barcode = s(v, "barcode");
    if !barcode.is_empty() {
        album.insert(Field::Barcode, barcode);
    }
    let (aa, aa_ids) = artist_credit(v.get("artist-credit").unwrap_or(&serde_json::Value::Null));
    let various = aa_ids.iter().any(|i| i == VARIOUS_ARTISTS_ID);
    album.insert(Field::AlbumArtist, if various { "Various Artists".into() } else { aa.clone() });
    if let Some(i) = aa_ids.first() {
        album.insert(Field::MbAlbumArtistId, i.clone());
    }
    if let Some(rg) = v.get("release-group") {
        album.insert(Field::MbReleaseGroupId, s(rg, "id"));
        let secondary: Vec<String> = rg.get("secondary-types").and_then(|t| t.as_array()).into_iter().flatten().filter_map(|t| t.as_str().map(str::to_string)).collect();
        if secondary.iter().any(|t| t == "Compilation") || various {
            album.insert(Field::Compilation, "1".into());
        }
        if secondary.iter().any(|t| t == "Audiobook" || t == "Audio drama" || t == "Spokenword") {
            album.insert(Field::Genre, "Audiobook".into());
        }
    }
    if let Some(li) = v.get("label-info").and_then(|l| l.as_array()).and_then(|l| l.first()) {
        let label = li.get("label").map(|l| s(l, "name")).unwrap_or_default();
        if !label.is_empty() {
            album.insert(Field::Label, label);
        }
        let cat = s(li, "catalog-number");
        if !cat.is_empty() {
            album.insert(Field::CatalogNumber, cat);
        }
    }
    // genre: most voted genre of the release, else of the release group
    let mut genres: Vec<(u64, String)> = Vec::new();
    for src in [v.get("genres"), v.get("release-group").and_then(|rg| rg.get("genres"))].into_iter().flatten() {
        for g in src.as_array().into_iter().flatten() {
            genres.push((g.get("count").and_then(|c| c.as_u64()).unwrap_or(0), s(g, "name")));
        }
    }
    genres.sort_by_key(|g| std::cmp::Reverse(g.0));
    if let Some((_, g)) = genres.first() {
        if !g.is_empty() && !album.contains_key(&Field::Genre) {
            album.insert(Field::Genre, capitalize(g));
        }
    }
    let media: Vec<&serde_json::Value> = v.get("media").and_then(|m| m.as_array()).map(|m| m.iter().collect()).unwrap_or_default();
    album.insert(Field::DiscTotal, media.len().max(1).to_string());
    let mut tracks = Vec::new();
    for (mi, m) in media.iter().enumerate() {
        let disc = m.get("position").and_then(|p| p.as_u64()).unwrap_or(mi as u64 + 1);
        let tl: Vec<&serde_json::Value> = m.get("tracks").and_then(|t| t.as_array()).map(|t| t.iter().collect()).unwrap_or_default();
        let total = m.get("track-count").and_then(|t| t.as_u64()).unwrap_or(tl.len() as u64);
        for (ti, t) in tl.iter().enumerate() {
            let mut f: BTreeMap<Field, String> = BTreeMap::new();
            let pos = t.get("position").and_then(|p| p.as_u64()).unwrap_or(ti as u64 + 1);
            f.insert(Field::TrackNumber, pos.to_string());
            f.insert(Field::TrackTotal, total.to_string());
            f.insert(Field::DiscNumber, disc.to_string());
            f.insert(Field::DiscTotal, media.len().max(1).to_string());
            let title = s(t, "title");
            let rec = t.get("recording");
            f.insert(Field::Title, if title.is_empty() { rec.map(|r| s(r, "title")).unwrap_or_default() } else { title });
            let (artist, ids) = artist_credit(t.get("artist-credit").or_else(|| rec.and_then(|r| r.get("artist-credit"))).unwrap_or(&serde_json::Value::Null));
            if !artist.is_empty() {
                f.insert(Field::Artist, artist);
            }
            if let Some(i) = ids.first() {
                f.insert(Field::MbArtistId, i.clone());
            }
            if let Some(r) = rec {
                f.insert(Field::MbRecordingId, s(r, "id"));
                if let Some(isrc) = r.get("isrcs").and_then(|i| i.as_array()).and_then(|i| i.first()).and_then(|i| i.as_str()) {
                    f.insert(Field::Isrc, isrc.to_string());
                }
            }
            let length = t.get("length").and_then(|l| l.as_f64()).or_else(|| rec.and_then(|r| r.get("length")).and_then(|l| l.as_f64()));
            tracks.push(TrackInfo { fields: f, duration_secs: length.map(|ms| ms / 1000.0) });
        }
    }
    let title = album.get(&Field::Album).cloned().unwrap_or_default();
    let artist = album.get(&Field::AlbumArtist).cloned().unwrap_or_default();
    let year: String = date.chars().take(4).collect();
    let fmt = media.first().map(|m| s(m, "format")).unwrap_or_default();
    let summary = [title, artist, year, format!("{} tracks", tracks.len()), fmt].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
    ReleaseInfo { source: "MusicBrainz".into(), album, tracks, cover: None, score: 0, summary }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_release() {
        let v: serde_json::Value = serde_json::from_str(r#"{"id":"rel","title":"Album","date":"2017-11-03","barcode":"123",
          "artist-credit":[{"name":"Jody Wisternoff","joinphrase":"","artist":{"id":"art1","name":"Jody Wisternoff"}}],
          "release-group":{"id":"rg","secondary-types":["DJ-mix","Compilation"]},
          "label-info":[{"label":{"name":"Anjunadeep"},"catalog-number":"ANJCD052"}],
          "genres":[{"name":"deep house","count":3},{"name":"house","count":1}],
          "media":[{"position":1,"format":"Digital Media","track-count":2,"tracks":[
            {"position":1,"title":"Void","length":86000,"artist-credit":[{"name":"Boerd","artist":{"id":"a2"}}],"recording":{"id":"r1","isrcs":["GBX1"]}},
            {"position":2,"title":"Flight","recording":{"id":"r2","title":"Flight","length":252000}}]}]}"#).unwrap();
        let r = parse_release(&v);
        assert_eq!(r.album.get(&Field::Album).unwrap(), "Album");
        assert_eq!(r.album.get(&Field::Label).unwrap(), "Anjunadeep");
        assert_eq!(r.album.get(&Field::CatalogNumber).unwrap(), "ANJCD052");
        assert_eq!(r.album.get(&Field::Genre).unwrap(), "Deep house");
        assert_eq!(r.album.get(&Field::Compilation).unwrap(), "1");
        assert_eq!(r.tracks.len(), 2);
        assert_eq!(r.tracks[0].fields.get(&Field::Artist).unwrap(), "Boerd");
        assert_eq!(r.tracks[0].fields.get(&Field::Isrc).unwrap(), "GBX1");
        assert_eq!(r.tracks[1].fields.get(&Field::Title).unwrap(), "Flight");
        assert_eq!(r.tracks[1].duration_secs, Some(252.0));
        assert_eq!(r.tracks[1].fields.get(&Field::Artist), None, "falls back to album artist in the plan");
        assert!(r.summary.starts_with("Album · Jody Wisternoff · 2017 · 2 tracks"));
    }
}
