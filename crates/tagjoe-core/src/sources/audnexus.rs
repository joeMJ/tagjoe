//! Audnexus (api.audnex.us): audiobook metadata by Audible ASIN, per region.

use super::http::Http;
use super::Result;
use crate::model::Field;
use crate::propose::{CoverSource, ReleaseInfo, TrackInfo};
use crate::sources::audiobookdb::strip_html;
use std::collections::BTreeMap;

pub struct Audnexus<'a> {
    pub http: &'a Http,
    pub base: &'a str,
}

fn s(v: &serde_json::Value, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

/// Does the string look like an Audible ASIN (B0 + 8 alphanumerics, or 10 digits/X)?
pub fn looks_like_asin(s: &str) -> bool {
    let s = s.trim();
    s.len() == 10 && s.chars().all(|c| c.is_ascii_alphanumeric()) && (s.starts_with("B0") || s.chars().all(|c| c.is_ascii_digit() || c == 'X'))
}

/// Find an ASIN inside free text (file or folder names, NFO lines).
pub fn find_asin(text: &str) -> Option<String> {
    for word in text.split(|c: char| !c.is_ascii_alphanumeric()) {
        if word.len() == 10 && word.starts_with("B0") && word.chars().all(|c| c.is_ascii_alphanumeric()) && word.chars().any(|c| c.is_ascii_digit()) {
            return Some(word.to_string());
        }
    }
    None
}

impl Audnexus<'_> {
    pub fn book(&self, asin: &str, region: &str) -> Result<serde_json::Value> {
        self.http.get_json(&format!("{}/books/{asin}?region={region}", self.base), &[])
    }
    pub fn chapters(&self, asin: &str, region: &str) -> Result<serde_json::Value> {
        self.http.get_json(&format!("{}/books/{asin}/chapters?region={region}", self.base), &[])
    }
    /// Book (+ chapters when available) for the first region that has it.
    pub fn release_info(&self, asin: &str, regions: &[String]) -> Result<Option<ReleaseInfo>> {
        for region in regions {
            match self.book(asin, region) {
                Ok(v) if v.get("title").is_some() => {
                    let chapters = self.chapters(asin, region).ok();
                    return Ok(Some(parse_book(&v, chapters.as_ref())));
                }
                Ok(_) => continue,
                Err(super::SourceError::Status { status: 404, .. }) => continue,
                Err(super::SourceError::Status { status: 400, .. }) => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }
}

pub fn parse_book(v: &serde_json::Value, chapters: Option<&serde_json::Value>) -> ReleaseInfo {
    let mut album: BTreeMap<Field, String> = BTreeMap::new();
    let title = s(v, "title");
    album.insert(Field::Album, title.clone());
    let authors: Vec<String> = v.get("authors").and_then(|a| a.as_array()).into_iter().flatten().map(|a| s(a, "name")).filter(|n| !n.is_empty()).collect();
    let author = authors.join(", ");
    if !author.is_empty() {
        album.insert(Field::AlbumArtist, author.clone());
        album.insert(Field::Artist, author.clone());
    }
    let narrators: Vec<String> = v.get("narrators").and_then(|a| a.as_array()).into_iter().flatten().map(|a| s(a, "name")).filter(|n| !n.is_empty()).collect();
    if !narrators.is_empty() {
        album.insert(Field::Narrator, narrators.join(", "));
    }
    for (field, key) in [(Field::Publisher, "publisherName"), (Field::Asin, "asin"), (Field::Barcode, "isbn")] {
        let x = s(v, key);
        if !x.is_empty() {
            album.insert(field, x);
        }
    }
    let date: String = s(v, "releaseDate").chars().take(10).collect();
    if !date.is_empty() {
        album.insert(Field::Date, date);
    } else {
        let c = s(v, "copyright");
        if !c.is_empty() {
            album.insert(Field::Date, c);
        }
    }
    let summary_text = {
        let x = s(v, "summary");
        if x.is_empty() { s(v, "description") } else { x }
    };
    if !summary_text.is_empty() {
        album.insert(Field::Description, strip_html(&summary_text));
    }
    if let Some(sp) = v.get("seriesPrimary") {
        let name = s(sp, "name");
        if !name.is_empty() {
            album.insert(Field::Series, name);
        }
        let pos = s(sp, "position");
        if !pos.is_empty() {
            album.insert(Field::SeriesPart, pos);
        }
    }
    let genres: Vec<String> = v.get("genres").and_then(|g| g.as_array()).into_iter().flatten().filter(|g| s(g, "type") == "genre").map(|g| s(g, "name")).filter(|g| !g.is_empty()).collect();
    album.insert(Field::Genre, if genres.is_empty() { "Audiobook".into() } else { format!("Audiobook, {}", genres[0]) });
    album.insert(Field::DiscTotal, "1".into());
    let cover = Some(s(v, "image")).filter(|u| !u.is_empty()).map(|url| CoverSource::Url { url });
    let mut tracks: Vec<TrackInfo> = chapters
        .and_then(|c| c.get("chapters"))
        .and_then(|c| c.as_array())
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(i, c)| {
            let mut f = BTreeMap::new();
            f.insert(Field::TrackNumber, (i + 1).to_string());
            f.insert(Field::Title, s(c, "title"));
            TrackInfo { fields: f, duration_secs: c.get("lengthMs").and_then(|l| l.as_f64()).map(|ms| ms / 1000.0) }
        })
        .collect();
    let n = tracks.len();
    for t in &mut tracks {
        t.fields.insert(Field::TrackTotal, n.to_string());
    }
    let runtime = v.get("runtimeLengthMin").and_then(|r| r.as_f64());
    let summary = [title, author, album.get(&Field::Date).map(|d| d.chars().take(4).collect::<String>()).unwrap_or_default(), runtime.map(|r| format!("{r:.0} min")).unwrap_or_default(), if n > 0 { format!("{n} chapters") } else { String::new() }, format!("Audible {}", s(v, "region")).to_uppercase().replace("AUDIBLE", "Audible")]
        .into_iter()
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    ReleaseInfo { source: "Audnexus".into(), album, tracks, cover, score: 0, summary }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str) -> serde_json::Value {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/json").join(name);
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }
    #[test]
    fn parses_book_and_chapters() {
        let r = parse_book(&fixture("audnexus-book.json"), Some(&fixture("audnexus-chapters.json")));
        assert_eq!(r.album.get(&Field::Album).unwrap(), "Project Hail Mary");
        assert_eq!(r.album.get(&Field::Narrator).unwrap(), "Ray Porter");
        assert_eq!(r.album.get(&Field::Asin).unwrap(), "B08G9PRS1K");
        assert_eq!(r.album.get(&Field::Date).unwrap(), "2021-05-04");
        assert!(r.album.get(&Field::Description).unwrap().contains("BESTSELLER"));
        assert_eq!(r.tracks.len(), 33);
        assert_eq!(r.tracks[0].fields.get(&Field::TrackTotal).unwrap(), "33");
        assert!(find_asin("Daniel Silva - Ransom [B07XYZ1234]").is_some());
        assert_eq!(find_asin("Nesbo, Jo - Der Sohn"), None);
        assert!(looks_like_asin("B08G9PRS1K"));
        assert!(!looks_like_asin("ANJCD167D"));
    }
}
