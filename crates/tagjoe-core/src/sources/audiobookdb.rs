//! AudiobookDB (audiobookdb.org): community audiobook catalogue. Anonymous access allows a
//! title search (`GET /books?q=`), book and release details; an API key raises the limits.
//! Apps using the API must credit AudiobookDB and link to audiobookdb.org.

use super::http::{enc, Http};
use super::Result;
use crate::model::Field;
use crate::propose::{CoverSource, ReleaseInfo, TrackInfo};
use std::collections::BTreeMap;

pub const CREDIT: &str = "Audiobook data from AudiobookDB (https://audiobookdb.org)";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BookHit {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub release_count: u64,
}

pub struct AudiobookDb<'a> {
    pub http: &'a Http,
    pub base: &'a str,
    pub key: &'a str,
}

fn s(v: &serde_json::Value, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

fn people(v: &serde_json::Value, role: &str) -> Vec<String> {
    v.get("people")
        .and_then(|p| p.as_array())
        .into_iter()
        .flatten()
        .filter(|p| p.get("role").map(|r| s(r, "name")).unwrap_or_default().eq_ignore_ascii_case(role))
        .filter_map(|p| p.get("person").map(|x| s(x, "name")))
        .filter(|n| !n.is_empty())
        .collect()
}

impl AudiobookDb<'_> {
    fn headers(&self) -> Vec<(&str, &str)> {
        if self.key.trim().is_empty() { vec![] } else { vec![("X-API-Key", self.key.trim())] }
    }

    /// Title search ("contains", case-insensitive).
    pub fn search(&self, title: &str, take: u32) -> Result<Vec<BookHit>> {
        let url = format!("{}/books?q={}&take={take}", self.base, enc(title.trim()));
        let v = self.http.get_json(&url, &self.headers())?;
        Ok(parse_hits(&v))
    }

    pub fn book(&self, id: &str) -> Result<serde_json::Value> {
        self.http.get_json(&format!("{}/books/{id}", self.base), &self.headers())
    }

    pub fn release(&self, id: &str) -> Result<serde_json::Value> {
        self.http.get_json(&format!("{}/releases/{id}", self.base), &self.headers())
    }

    /// Book + chosen release (first, or the one whose language matches `lang` best) → ReleaseInfo.
    pub fn release_info(&self, book_id: &str, prefer_language: &str) -> Result<Option<ReleaseInfo>> {
        let book = self.book(book_id)?;
        let releases: Vec<&serde_json::Value> = book.get("releases").and_then(|r| r.as_array()).map(|r| r.iter().collect()).unwrap_or_default();
        let pick = releases
            .iter()
            .find(|r| r.get("language").map(|l| s(l, "name")).unwrap_or_default().eq_ignore_ascii_case(prefer_language))
            .or_else(|| releases.first())
            .map(|r| s(r, "id"));
        let Some(rid) = pick else { return Ok(None) };
        let release = self.release(&rid)?;
        Ok(Some(parse_book_release(&book, &release)))
    }
}

pub fn parse_hits(v: &serde_json::Value) -> Vec<BookHit> {
    v.as_array()
        .into_iter()
        .flatten()
        .map(|b| BookHit {
            id: s(b, "id"),
            title: s(b, "title"),
            authors: b.get("authors").and_then(|a| a.as_array()).into_iter().flatten().map(|a| if a.is_string() { a.as_str().unwrap_or("").to_string() } else { s(a, "name") }).filter(|n| !n.is_empty()).collect(),
            release_count: b.get("releaseCount").and_then(|c| c.as_u64()).unwrap_or(0),
        })
        .collect()
}

fn image_url(v: &serde_json::Value) -> Option<String> {
    // sourceUrl = full resolution; url + "/large.jpg" = 768 px (the bare url returns 404)
    let src = s(v, "sourceUrl");
    if !src.is_empty() {
        return Some(src);
    }
    let url = s(v, "url");
    if !url.is_empty() { Some(format!("{url}/large.jpg")) } else { None }
}

/// Strip simple HTML tags and collapse whitespace.
pub fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&nbsp;", " ");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn parse_book_release(book: &serde_json::Value, release: &serde_json::Value) -> ReleaseInfo {
    let mut album: BTreeMap<Field, String> = BTreeMap::new();
    let title = {
        let t = s(release, "title");
        if t.is_empty() { s(book, "title") } else { t }
    };
    album.insert(Field::Album, title.clone());
    let authors = people(book, "Author");
    let author = authors.join(", ");
    if !author.is_empty() {
        album.insert(Field::AlbumArtist, author.clone());
        album.insert(Field::Artist, author.clone());
    }
    let narrators = people(release, "Narrator");
    if !narrators.is_empty() {
        album.insert(Field::Narrator, narrators.join(", "));
    }
    if let Some(p) = release.get("publisher").map(|p| s(p, "name")).filter(|p| !p.is_empty()) {
        album.insert(Field::Publisher, p);
    }
    let date = s(release, "releaseDate");
    let date: String = date.chars().take(10).collect();
    if !date.is_empty() {
        album.insert(Field::Date, date);
    } else if let Some(y) = book.get("copyright").and_then(|c| c.as_u64()) {
        album.insert(Field::Date, y.to_string());
    }
    let desc = {
        let d = s(release, "description");
        if d.is_empty() { s(book, "description") } else { d }
    };
    if !desc.is_empty() {
        album.insert(Field::Description, strip_html(&desc));
    }
    let isbn = s(release, "isbn");
    if !isbn.is_empty() {
        album.insert(Field::Barcode, isbn);
    }
    if let Some(ext) = release.get("external").and_then(|e| e.as_array()) {
        for e in ext {
            let cat = e.get("category").map(|c| s(c, "title")).unwrap_or_default();
            if cat.eq_ignore_ascii_case("Audible") {
                let id = s(e, "itemId");
                if !id.is_empty() {
                    album.insert(Field::Asin, id);
                }
            }
        }
    }
    if let Some(series) = book.get("series").and_then(|x| x.as_array()).and_then(|x| x.first()) {
        let name = series.get("series").map(|x| s(x, "title")).unwrap_or_default();
        if !name.is_empty() {
            album.insert(Field::Series, name);
        }
        let pos = series.get("position").map(|p| s(p, "value")).filter(|p| !p.is_empty()).or_else(|| Some(s(series, "ordinal"))).unwrap_or_default();
        if !pos.is_empty() {
            album.insert(Field::SeriesPart, pos);
        }
    }
    let genres: Vec<String> = book.get("genres").and_then(|g| g.as_array()).into_iter().flatten().map(|g| s(g, "title")).filter(|g| !g.is_empty()).collect();
    album.insert(Field::Genre, if genres.is_empty() { "Audiobook".into() } else { format!("Audiobook, {}", genres[0]) });
    album.insert(Field::DiscTotal, "1".into());
    let cover = release.get("images").and_then(|i| i.as_array()).and_then(|i| i.first()).and_then(image_url).or_else(|| book.get("coverImage").and_then(image_url)).map(|url| CoverSource::Url { url });
    let runtime = release.get("runtimeLengthSec").and_then(|r| r.as_f64());
    let chapters: Vec<TrackInfo> = release
        .get("chapterDetail")
        .and_then(|c| c.get("chapters"))
        .and_then(|c| c.as_array())
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(i, c)| {
            let mut f = BTreeMap::new();
            f.insert(Field::TrackNumber, c.get("ordinal").and_then(|o| o.as_u64()).unwrap_or(i as u64 + 1).to_string());
            f.insert(Field::Title, s(c, "title"));
            TrackInfo { fields: f, duration_secs: c.get("lengthMs").and_then(|l| l.as_f64()).map(|ms| ms / 1000.0) }
        })
        .collect();
    let n = chapters.len();
    let mut tracks = chapters;
    for t in &mut tracks {
        t.fields.insert(Field::TrackTotal, n.to_string());
    }
    let summary = [title, author, album.get(&Field::Date).map(|d| d.chars().take(4).collect::<String>()).unwrap_or_default(), runtime.map(|r| format!("{:.0} min", r / 60.0)).unwrap_or_default(), if n > 0 { format!("{n} chapters") } else { String::new() }]
        .into_iter()
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    ReleaseInfo { source: "AudiobookDB".into(), album, tracks, cover, score: 0, summary }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str) -> serde_json::Value {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/json").join(name);
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }
    #[test]
    fn parses_recorded_responses() {
        let hits = parse_hits(&fixture("audiobookdb-books.json"));
        assert!(!hits.is_empty());
        assert_eq!(hits[0].title, "Project Hail Mary");
        let r = parse_book_release(&fixture("audiobookdb-book.json"), &fixture("audiobookdb-release.json"));
        assert_eq!(r.album.get(&Field::Album).unwrap(), "Project Hail Mary");
        assert_eq!(r.album.get(&Field::AlbumArtist).unwrap(), "Andy Weir");
        assert_eq!(r.album.get(&Field::Narrator).unwrap(), "Ray Porter");
        assert_eq!(r.album.get(&Field::Publisher).unwrap(), "Audible Studios");
        assert_eq!(r.album.get(&Field::Date).unwrap(), "2021-05-04");
        assert_eq!(r.album.get(&Field::Asin).unwrap(), "B08G9PRS1K");
        assert_eq!(r.tracks.len(), 33);
        assert_eq!(r.tracks[1].fields.get(&Field::Title).unwrap(), "Dedication");
        assert!(matches!(r.cover, Some(CoverSource::Url { .. })));
        assert!(r.summary.contains("33 chapters"));
        assert_eq!(strip_html("<p><b>Hi</b> &amp; bye</p>"), "Hi & bye");
    }
}
