//! Deutsche Nationalbibliothek (DNB): the German national bibliography, complete for German
//! audiobooks. Public SRU interface, no key, MARC21-XML. Gives title, author, narrator, publisher,
//! year, ISBN, runtime and track count – no chapters, no covers (cover via Open Library by ISBN).

use super::http::{enc, Http};
use super::Result;
use crate::model::Field;
use crate::propose::{CoverSource, ReleaseInfo, TrackInfo};
use std::collections::BTreeMap;

pub const BASE: &str = "https://services.dnb.de/sru/dnb";

pub struct Dnb<'a> {
    pub http: &'a Http,
}

/// One MARC record, reduced to what we use.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Marc {
    pub leader: String,
    /// (tag, [(code, value)])
    pub fields: Vec<(String, Vec<(char, String)>)>,
}

fn xml_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(j) = tail.find(';') else {
            out.push_str(tail);
            return out;
        };
        let ent = &tail[1..j];
        let decoded = match ent {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "amp" => Some('&'),
            e if e.starts_with("#x") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => out.push(c),
            None => out.push_str(&tail[..=j]),
        }
        rest = &tail[j + 1..];
    }
    out.push_str(rest);
    out
}

/// Compose base letter + combining mark into one character (DNB delivers decomposed text,
/// Unicode NFD); covers the Latin accents that occur in German and European names.
pub fn compose_latin(s: &str) -> String {
    const MARKS: &[(char, &str, &str)] = &[
        ('\u{308}', "aeiouyAEIOUY", "äëïöüÿÄËÏÖÜŸ"),
        ('\u{301}', "aeiouycnszAEIOUYCNSZ", "áéíóúýćńśźÁÉÍÓÚÝĆŃŚŹ"),
        ('\u{300}', "aeiouAEIOU", "àèìòùÀÈÌÒÙ"),
        ('\u{302}', "aeiouAEIOU", "âêîôûÂÊÎÔÛ"),
        ('\u{303}', "anoANO", "ãñõÃÑÕ"),
        ('\u{30a}', "aAuU", "åÅůŮ"),
        ('\u{327}', "cCsS", "çÇşŞ"),
        ('\u{30c}', "cesrzCESRZ", "čěšřžČEŠŘŽ"),
    ];
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if let Some((_, bases, composed)) = MARKS.iter().find(|(m, _, _)| *m == c) {
            if let Some(last) = out.chars().last() {
                if let Some(i) = bases.chars().position(|b| b == last) {
                    if let Some(r) = composed.chars().nth(i) {
                        out.pop();
                        out.push(r);
                        continue;
                    }
                }
            }
        }
        out.push(c);
    }
    out
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let i = tag.find(&key)? + key.len();
    let j = tag[i..].find('"')? + i;
    Some(tag[i..j].to_string())
}

/// Minimal MARC21-XML reader (std only): records → leader + datafields/subfields.
pub fn parse_marc(xml: &str) -> Vec<Marc> {
    // SRU wraps every MARC record in its own <record>; the MARC record is the one with a <leader>
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(ls) = xml[pos..].find("<leader>") {
        let lstart = pos + ls;
        let Some(le) = xml[lstart..].find("</leader>") else { break };
        let Some(re) = xml[lstart..].find("</record>") else { break };
        let rec = &xml[lstart..lstart + re];
        pos = lstart + re + 9;
        let mut m = Marc { leader: xml[lstart + 8..lstart + le].to_string(), fields: Vec::new() };
        let mut p = 0;
        while let Some(ds) = rec[p..].find("<datafield") {
            let dstart = p + ds;
            let Some(tag_end) = rec[dstart..].find('>') else { break };
            let open = &rec[dstart..dstart + tag_end];
            let Some(de) = rec[dstart..].find("</datafield>") else { break };
            let body = &rec[dstart + tag_end + 1..dstart + de];
            p = dstart + de + 12;
            let tag = attr(open, "tag").unwrap_or_default();
            let mut subs = Vec::new();
            let mut q = 0;
            while let Some(ss) = body[q..].find("<subfield") {
                let sstart = q + ss;
                let Some(se_open) = body[sstart..].find('>') else { break };
                let sopen = &body[sstart..sstart + se_open];
                let Some(se) = body[sstart..].find("</subfield>") else { break };
                let val = &body[sstart + se_open + 1..sstart + se];
                q = sstart + se + 11;
                let code = attr(sopen, "code").and_then(|c| c.chars().next()).unwrap_or(' ');
                // strip MARC non-sorting markers (U+0098/U+009C) and whitespace
                let v: String = compose_latin(&xml_unescape(val)).chars().filter(|c| *c != '\u{98}' && *c != '\u{9c}').collect();
                subs.push((code, v.trim().to_string()));
            }
            m.fields.push((tag, subs));
        }
        out.push(m);
    }
    out
}

impl Marc {
    pub fn get(&self, tag: &str, code: char) -> Option<&str> {
        self.fields.iter().find(|(t, _)| t == tag).and_then(|(_, s)| s.iter().find(|(c, _)| *c == code)).map(|(_, v)| v.as_str())
    }
    pub fn all(&self, tag: &str) -> impl Iterator<Item = &Vec<(char, String)>> {
        self.fields.iter().filter(move |(t, _)| t == tag).map(|(_, s)| s)
    }
    /// Non-musical sound recording (audiobook).
    pub fn is_spoken(&self) -> bool {
        self.leader.chars().nth(6) == Some('i')
    }
}

/// "Nesbø, Jo" → "Jo Nesbø"
fn person(s: &str) -> String {
    let s = s.trim().trim_end_matches(',');
    match s.split_once(',') {
        Some((last, first)) => format!("{} {}", first.trim(), last.trim()),
        None => s.to_string(),
    }
}

fn digits(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_digit()).collect()
}

/// Minutes and track count from the extent field ("Online-Ressource 920 Min." / "176 Tracks" / "8 CDs (595 Min.)").
fn extent(m: &Marc) -> (Option<f64>, Option<u32>) {
    let mut minutes = None;
    let mut tracks = None;
    for subs in m.all("300") {
        for (_, v) in subs {
            let lower = v.to_lowercase();
            let words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
            for w in words.windows(2) {
                if let Ok(n) = w[0].parse::<f64>() {
                    if w[1].starts_with("min") {
                        minutes = Some(n);
                    } else if w[1].starts_with("track") || w[1].starts_with("titel") {
                        tracks = Some(n as u32);
                    } else if w[1].starts_with("std") || w[1].starts_with("h") && w[1].len() <= 2 {
                        minutes = Some(minutes.unwrap_or(0.0) + n * 60.0);
                    }
                }
            }
        }
    }
    (minutes, tracks)
}

pub fn to_release(m: &Marc) -> Option<ReleaseInfo> {
    if !m.is_spoken() {
        return None;
    }
    let title = m.get("245", 'a')?.to_string();
    if title.is_empty() {
        return None;
    }
    let mut album: BTreeMap<Field, String> = BTreeMap::new();
    album.insert(Field::Album, title.clone());
    // author: 100 $a, else 700 with $4 aut, else first name in 245 $c
    let mut author = m.get("100", 'a').map(person);
    let mut narrators = Vec::new();
    for subs in m.all("700") {
        let name = subs.iter().find(|(c, _)| *c == 'a').map(|(_, v)| person(v)).unwrap_or_default();
        let rel = subs.iter().find(|(c, _)| *c == '4').map(|(_, v)| v.as_str()).unwrap_or("");
        let role = subs.iter().find(|(c, _)| *c == 'e').map(|(_, v)| v.to_lowercase()).unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        if rel == "nrt" || role.contains("erzähler") || role.contains("sprecher") {
            narrators.push(name);
        } else if rel == "aut" && author.is_none() {
            author = Some(name);
        }
    }
    let statement = m.get("245", 'c').unwrap_or("").to_string();
    if narrators.is_empty() {
        // "… ; gelesen von Sascha Rotermund ; aus dem …"
        let l = statement.to_lowercase();
        if let Some(i) = l.find("gelesen von ") {
            let rest = &statement[i + "gelesen von ".len()..];
            let name = rest.split([';', '.', ',']).next().unwrap_or("").trim();
            if !name.is_empty() {
                narrators.push(name.to_string());
            }
        }
    }
    if author.is_none() {
        let first = statement.split([';', '.']).next().unwrap_or("").trim();
        if !first.is_empty() && !first.to_lowercase().starts_with("gelesen") {
            author = Some(first.to_string());
        }
    }
    if let Some(a) = &author {
        album.insert(Field::AlbumArtist, a.clone());
        album.insert(Field::Artist, a.clone());
    }
    if !narrators.is_empty() {
        album.insert(Field::Narrator, narrators.join(", "));
    }
    let publisher = m.all("264").chain(m.all("260")).find_map(|s| s.iter().find(|(c, _)| *c == 'b').map(|(_, v)| v.trim_end_matches([',', ' ']).to_string())).filter(|p| !p.is_empty());
    if let Some(p) = &publisher {
        album.insert(Field::Publisher, p.clone());
    }
    if let Some(y) = m.all("264").chain(m.all("260")).find_map(|s| s.iter().find(|(c, _)| *c == 'c').map(|(_, v)| digits(v))).filter(|d| d.len() >= 4) {
        album.insert(Field::Date, y[..4].to_string());
    }
    if let Some(isbn) = m.all("020").find_map(|s| s.iter().find(|(c, _)| *c == 'a').map(|(_, v)| digits(v))).filter(|d| d.len() == 13 || d.len() == 10) {
        album.insert(Field::Barcode, isbn);
    }
    let genres: Vec<String> = m.all("655").filter_map(|s| s.iter().find(|(c, _)| *c == 'a').map(|(_, v)| v.clone())).filter(|g| !["Hörbuch", "CD", "Fiktionale Darstellung", "MP3"].contains(&g.as_str())).collect();
    album.insert(Field::Genre, if genres.is_empty() { "Audiobook".into() } else { format!("Audiobook, {}", genres[0]) });
    if let Some(sub) = m.get("245", 'b') {
        if sub.to_lowercase().contains("gekürzt") {
            album.insert(Field::Description, sub.to_string());
        }
    }
    album.insert(Field::DiscTotal, "1".into());
    let (minutes, track_count) = extent(m);
    // tracks: only the count is known; one entry per track with the total runtime spread evenly
    let mut tracks = Vec::new();
    if let Some(n) = track_count {
        for i in 0..n {
            let mut f = BTreeMap::new();
            f.insert(Field::TrackNumber, (i + 1).to_string());
            f.insert(Field::TrackTotal, n.to_string());
            tracks.push(TrackInfo { fields: f, duration_secs: minutes.map(|mi| mi * 60.0 / n as f64) });
        }
    }
    let cover = album.get(&Field::Barcode).map(|isbn| CoverSource::Url { url: format!("https://covers.openlibrary.org/b/isbn/{isbn}-L.jpg?default=false") });
    let summary = [title, author.clone().unwrap_or_default(), album.get(&Field::Date).cloned().unwrap_or_default(), publisher.unwrap_or_default(), minutes.map(|m| format!("{m:.0} min")).unwrap_or_default(), track_count.map(|t| format!("{t} tracks")).unwrap_or_default(), "DNB".into()]
        .into_iter()
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    Some(ReleaseInfo { source: "DNB".into(), album, tracks, cover, score: 0, summary })
}

impl Dnb<'_> {
    /// Audiobook records for a title and (optionally) an author.
    pub fn search(&self, title: &str, author: &str, max: u32) -> Result<Vec<ReleaseInfo>> {
        let title = title.replace('"', "");
        let mut q = format!("tit=\"{title}\"");
        let author = author.trim().replace('"', "");
        if !author.is_empty() {
            // surname is enough and avoids "Nesbø" vs "Nesbo" misses
            let surname = author.split_whitespace().last().unwrap_or(&author);
            q.push_str(&format!(" and per=\"{surname}\""));
        }
        let url = format!("{BASE}?version=1.1&operation=searchRetrieve&query={}&recordSchema=MARC21-xml&maximumRecords={max}", enc(&q));
        let bytes = self.http.get_bytes(&url, &[("Accept", "application/xml")], "xml")?;
        let xml = String::from_utf8_lossy(&bytes);
        let mut out: Vec<ReleaseInfo> = parse_marc(&xml).iter().filter_map(to_release).collect();
        // prefer real publishers over the library's own digitised copies
        out.sort_by_key(|r| r.album.get(&Field::Publisher).map(|p| p.contains("Nationalbibliothek")).unwrap_or(true));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composes_decomposed_text() {
        assert_eq!(compose_latin("Ho\u{308}rbuch Nesbo\u{308} Fre\u{301}de\u{301}ric"), "Hörbuch Nesbǿ".replace("ǿ", "ö") + " Frédéric");
        assert_eq!(compose_latin("plain"), "plain");
    }

    #[test]
    fn parses_dnb_records() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/json/dnb-nesbo.xml");
        let xml = std::fs::read_to_string(p).unwrap();
        let recs = parse_marc(&xml);
        assert_eq!(recs.len(), 10);
        let spoken: Vec<ReleaseInfo> = recs.iter().filter_map(to_release).collect();
        assert!(spoken.len() >= 5, "{}", spoken.len());
        let tide = spoken.iter().find(|r| r.album.get(&Field::Publisher).map(|p| p == "TIDE exklusiv").unwrap_or(false)).expect("TIDE record");
        assert_eq!(tide.album.get(&Field::Album).unwrap(), "Der Sohn");
        assert_eq!(tide.album.get(&Field::AlbumArtist).unwrap(), "Jo Nesbø");
        assert_eq!(tide.album.get(&Field::Narrator).unwrap(), "Sascha Rotermund");
        assert_eq!(tide.album.get(&Field::Date).unwrap(), "2014");
        assert_eq!(tide.album.get(&Field::Barcode).unwrap(), "9783844911411");
        assert_eq!(tide.tracks.len(), 176);
        assert!((tide.tracks[0].duration_secs.unwrap() - 920.0 * 60.0 / 176.0).abs() < 0.01);
        assert!(tide.summary.contains("920 min"));
        // narrator from the statement of responsibility when no 700 nrt exists
        let pubs: Vec<String> = spoken.iter().map(|r| r.album.get(&Field::Publisher).cloned().unwrap_or_default()).collect();
        let cd = spoken.iter().find(|r| r.album.get(&Field::Publisher).map(|p| p.starts_with("Hörbuch Hamburg")).unwrap_or(false)).unwrap_or_else(|| panic!("publishers: {pubs:?}"));
        assert_eq!(cd.album.get(&Field::Narrator).unwrap(), "Sascha Rotermund");
    }
}
