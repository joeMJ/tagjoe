//! AcoustID lookup: fingerprint → MusicBrainz recording and release ids.

use super::http::Http;
use super::{Result, SourceError};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AcoustMatch {
    pub score: f64,
    pub recording_id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub release_ids: Vec<String>,
}

pub struct AcoustId<'a> {
    pub http: &'a Http,
    pub key: &'a str,
}

impl AcoustId<'_> {
    pub fn lookup(&self, fingerprint: &str, duration: f64) -> Result<Vec<AcoustMatch>> {
        if self.key.trim().is_empty() {
            return Err(SourceError::NotConfigured("AcoustID application key (acoustid_key)".into()));
        }
        // POST keeps the long fingerprint out of the URL (as the AcoustID docs recommend)
        let duration = (duration.round() as i64).to_string();
        let v = self.http.post_form_json("https://api.acoustid.org/v2/lookup", &[("client", self.key.trim()), ("meta", "recordings releaseids"), ("duration", &duration), ("fingerprint", fingerprint)])?;
        Ok(parse(&v))
    }
}

pub fn parse(v: &serde_json::Value) -> Vec<AcoustMatch> {
    let mut out = Vec::new();
    for r in v.get("results").and_then(|r| r.as_array()).into_iter().flatten() {
        let score = r.get("score").and_then(|s| s.as_f64()).unwrap_or(0.0);
        for rec in r.get("recordings").and_then(|x| x.as_array()).into_iter().flatten() {
            let id = rec.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
            if id.is_empty() {
                continue;
            }
            out.push(AcoustMatch {
                score,
                recording_id: id,
                title: rec.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                artists: rec.get("artists").and_then(|a| a.as_array()).into_iter().flatten().filter_map(|a| a.get("name").and_then(|n| n.as_str()).map(str::to_string)).collect(),
                release_ids: rec.get("releases").and_then(|a| a.as_array()).into_iter().flatten().filter_map(|a| a.get("id").and_then(|n| n.as_str()).map(str::to_string)).collect(),
            });
        }
    }
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_lookup() {
        let v: serde_json::Value = serde_json::from_str(r#"{"status":"ok","results":[{"id":"x","score":0.97,"recordings":[{"id":"rec1","title":"T","artists":[{"id":"a","name":"Band"}],"releases":[{"id":"rel1"},{"id":"rel2"}]}]}]}"#).unwrap();
        let m = parse(&v);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].recording_id, "rec1");
        assert_eq!(m[0].release_ids, vec!["rel1", "rel2"]);
        assert_eq!(m[0].artists, vec!["Band"]);
    }
}
