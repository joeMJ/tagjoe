//! Discogs: optional second music source (personal access token required).
//! Images and some data are "restricted data" under the Discogs API terms: used only to
//! tag the user's own files, never redistributed.

use super::http::{enc, Http};
use super::{Result, SourceError};
use crate::model::Field;
use crate::propose::{CoverSource, ReleaseInfo, TrackInfo};
use std::collections::BTreeMap;

pub struct Discogs<'a> {
    pub http: &'a Http,
    pub token: &'a str,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hit {
    pub id: u64,
    pub title: String,
    pub year: String,
    pub format: String,
    pub label: String,
    pub catno: String,
}

fn s(v: &serde_json::Value, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

fn parse_duration(d: &str) -> Option<f64> {
    let parts: Vec<&str> = d.trim().split(':').collect();
    match parts.as_slice() {
        [m, sec] => Some(m.parse::<f64>().ok()? * 60.0 + sec.parse::<f64>().ok()?),
        [h, m, sec] => Some(h.parse::<f64>().ok()? * 3600.0 + m.parse::<f64>().ok()? * 60.0 + sec.parse::<f64>().ok()?),
        _ => None,
    }
}

/// Discogs appends " (2)" to disambiguate artist names; strip it for tags.
fn clean_artist(name: &str) -> String {
    let t = name.trim();
    if let Some(i) = t.rfind(" (") {
        if t.ends_with(')') && t[i + 2..t.len() - 1].chars().all(|c| c.is_ascii_digit()) {
            return t[..i].to_string();
        }
    }
    t.to_string()
}

fn artists(v: &serde_json::Value) -> String {
    let mut out = String::new();
    for a in v.as_array().into_iter().flatten() {
        out.push_str(&clean_artist(&s(a, "name")));
        let join = s(a, "join");
        if !join.is_empty() {
            if join == "," {
                out.push_str(", ");
            } else {
                out.push_str(&format!(" {} ", join.trim()));
            }
        }
    }
    out.trim().to_string()
}

impl Discogs<'_> {
    fn headers(&self) -> Vec<(&str, String)> {
        vec![("Authorization", format!("Discogs token={}", self.token.trim()))]
    }
    fn get(&self, url: &str) -> Result<serde_json::Value> {
        if self.token.trim().is_empty() {
            return Err(SourceError::NotConfigured("Discogs token".into()));
        }
        let h = self.headers();
        let hs: Vec<(&str, &str)> = h.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.http.get_json(url, &hs)
    }

    pub fn search(&self, album: &str, artist: &str, limit: u32) -> Result<Vec<Hit>> {
        let mut url = format!("https://api.discogs.com/database/search?type=release&per_page={limit}&release_title={}", enc(album.trim()));
        if !artist.trim().is_empty() && !crate::classify::is_various(artist) {
            url.push_str(&format!("&artist={}", enc(artist.trim())));
        }
        let v = self.get(&url)?;
        Ok(v.get("results")
            .and_then(|r| r.as_array())
            .into_iter()
            .flatten()
            .map(|r| Hit {
                id: r.get("id").and_then(|i| i.as_u64()).unwrap_or(0),
                title: s(r, "title"),
                year: s(r, "year"),
                format: r.get("format").and_then(|f| f.as_array()).map(|f| f.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default(),
                label: r.get("label").and_then(|f| f.as_array()).and_then(|f| f.first()).and_then(|x| x.as_str()).unwrap_or("").to_string(),
                catno: s(r, "catno"),
            })
            .filter(|h| h.id > 0)
            .collect())
    }

    pub fn release(&self, id: u64) -> Result<ReleaseInfo> {
        let v = self.get(&format!("https://api.discogs.com/releases/{id}"))?;
        Ok(parse_release(&v))
    }
}

pub fn parse_release(v: &serde_json::Value) -> ReleaseInfo {
    let mut album: BTreeMap<Field, String> = BTreeMap::new();
    let title = s(v, "title");
    album.insert(Field::Album, title.clone());
    let aa = artists(v.get("artists").unwrap_or(&serde_json::Value::Null));
    let various = aa.eq_ignore_ascii_case("various") || crate::classify::is_various(&aa);
    album.insert(Field::AlbumArtist, if various { "Various Artists".into() } else { aa.clone() });
    if various {
        album.insert(Field::Compilation, "1".into());
    }
    let released = s(v, "released");
    let year = v.get("year").and_then(|y| y.as_u64()).map(|y| y.to_string()).unwrap_or_default();
    let date = if released.len() >= 4 && !released.starts_with('0') { released.replace("-00", "") } else { year };
    if !date.is_empty() {
        album.insert(Field::Date, date);
    }
    if let Some(l) = v.get("labels").and_then(|l| l.as_array()).and_then(|l| l.first()) {
        let name = s(l, "name");
        if !name.is_empty() {
            album.insert(Field::Label, clean_artist(&name));
        }
        let catno = s(l, "catno");
        if !catno.is_empty() && catno != "none" {
            album.insert(Field::CatalogNumber, catno);
        }
    }
    for ident in v.get("identifiers").and_then(|i| i.as_array()).into_iter().flatten() {
        if s(ident, "type").eq_ignore_ascii_case("Barcode") {
            let b: String = s(ident, "value").chars().filter(|c| c.is_ascii_digit()).collect();
            if !b.is_empty() {
                album.insert(Field::Barcode, b);
                break;
            }
        }
    }
    let genres: Vec<String> = v.get("styles").or(v.get("genres")).and_then(|g| g.as_array()).into_iter().flatten().filter_map(|g| g.as_str().map(str::to_string)).collect();
    if let Some(g) = genres.first() {
        album.insert(Field::Genre, g.clone());
    }
    let cover = v.get("images").and_then(|i| i.as_array()).and_then(|imgs| imgs.iter().find(|i| s(i, "type") == "primary").or(imgs.first())).map(|i| s(i, "uri")).filter(|u| !u.is_empty()).map(|url| CoverSource::Url { url });
    // tracks: positions like "1", "A1", "1-3" (disc-track)
    let mut tracks = Vec::new();
    let mut disc_count = 1u32;
    for t in v.get("tracklist").and_then(|t| t.as_array()).into_iter().flatten() {
        if s(t, "type_") != "track" {
            continue;
        }
        let pos = s(t, "position");
        let (disc, num) = match pos.split_once(['-', '.']) {
            Some((d, n)) if d.chars().all(|c| c.is_ascii_digit()) && !d.is_empty() => (d.parse().unwrap_or(1), n.to_string()),
            _ => (1u32, pos.clone()),
        };
        disc_count = disc_count.max(disc);
        let mut f = BTreeMap::new();
        f.insert(Field::Title, s(t, "title"));
        f.insert(Field::DiscNumber, disc.to_string());
        let n: String = num.chars().filter(|c| c.is_ascii_digit()).collect();
        f.insert(Field::TrackNumber, if n.is_empty() { (tracks.len() + 1).to_string() } else { n });
        let ta = artists(t.get("artists").unwrap_or(&serde_json::Value::Null));
        if !ta.is_empty() {
            f.insert(Field::Artist, ta);
        }
        tracks.push(TrackInfo { fields: f, duration_secs: parse_duration(&s(t, "duration")) });
    }
    // totals per disc
    let mut per_disc: BTreeMap<String, u32> = BTreeMap::new();
    for t in &tracks {
        *per_disc.entry(t.fields.get(&Field::DiscNumber).cloned().unwrap_or_default()).or_default() += 1;
    }
    for t in &mut tracks {
        let d = t.fields.get(&Field::DiscNumber).cloned().unwrap_or_default();
        t.fields.insert(Field::TrackTotal, per_disc.get(&d).copied().unwrap_or(1).to_string());
        t.fields.insert(Field::DiscTotal, disc_count.to_string());
    }
    album.insert(Field::DiscTotal, disc_count.to_string());
    let fmt = v.get("formats").and_then(|f| f.as_array()).and_then(|f| f.first()).map(|f| s(f, "name")).unwrap_or_default();
    let summary = [title, album.get(&Field::AlbumArtist).cloned().unwrap_or_default(), album.get(&Field::Date).map(|d| d.chars().take(4).collect::<String>()).unwrap_or_default(), format!("{} tracks", tracks.len()), fmt, "Discogs".into()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
    ReleaseInfo { source: "Discogs".into(), album, tracks, cover, score: 0, summary }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_release() {
        let v: serde_json::Value = serde_json::from_str(r#"{"title":"Mix","year":2019,"released":"2019-04-05",
          "artists":[{"name":"Various","join":""}],"labels":[{"name":"Anjunadeep (2)","catno":"ANJDEE398D"}],
          "identifiers":[{"type":"Barcode","value":"5 039060 327593"}],"genres":["Electronic"],"styles":["Deep House"],
          "images":[{"type":"primary","uri":"https://i.discogs.com/x.jpg"}],"formats":[{"name":"File"}],
          "tracklist":[{"type_":"track","position":"1","title":"A","duration":"3:34","artists":[{"name":"Tomas Barfod","join":"feat."},{"name":"Louise Foo","join":""}]},
                       {"type_":"heading","position":"","title":"x"},
                       {"type_":"track","position":"2","title":"B","duration":"4:10"}]}"#).unwrap();
        let r = parse_release(&v);
        assert_eq!(r.album.get(&Field::AlbumArtist).unwrap(), "Various Artists");
        assert_eq!(r.album.get(&Field::Compilation).unwrap(), "1");
        assert_eq!(r.album.get(&Field::Label).unwrap(), "Anjunadeep");
        assert_eq!(r.album.get(&Field::Barcode).unwrap(), "5039060327593");
        assert_eq!(r.album.get(&Field::Genre).unwrap(), "Deep House");
        assert_eq!(r.tracks.len(), 2);
        assert_eq!(r.tracks[0].fields.get(&Field::Artist).unwrap(), "Tomas Barfod feat. Louise Foo");
        assert_eq!(r.tracks[0].duration_secs, Some(214.0));
        assert_eq!(r.tracks[1].fields.get(&Field::TrackTotal).unwrap(), "2");
        assert!(matches!(r.cover, Some(CoverSource::Url { .. })));
        assert_eq!(parse_duration("1:02:03"), Some(3723.0));
    }
}
