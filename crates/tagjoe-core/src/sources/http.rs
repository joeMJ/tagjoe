//! Blocking HTTP client with a per-host rate limit and an on-disk cache.

use super::{Result, SourceError};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct Http {
    client: reqwest::blocking::Client,
    /// Minimum interval between requests per host.
    intervals: HashMap<String, Duration>,
    last: Mutex<HashMap<String, Instant>>,
    cache_dir: Option<PathBuf>,
    cache_ttl: Duration,
    pub offline: bool,
}

fn cache_key(url: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn host_of(url: &str) -> String {
    url.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or("").to_string()
}

impl Http {
    pub fn new(user_agent: &str, cache_dir: Option<PathBuf>, cache_days: u32) -> Self {
        let client = reqwest::blocking::Client::builder().user_agent(user_agent).timeout(Duration::from_secs(30)).build().expect("http client");
        let mut intervals = HashMap::new();
        intervals.insert("musicbrainz.org".to_string(), Duration::from_millis(1100));
        // AcoustID allows 3/s but answers bursts with 429; one per second is reliable
        intervals.insert("api.acoustid.org".to_string(), Duration::from_millis(1000));
        intervals.insert("api.discogs.com".to_string(), Duration::from_millis(1100));
        intervals.insert("api.audnex.us".to_string(), Duration::from_millis(250));
        intervals.insert("audiobookdb.org".to_string(), Duration::from_millis(500));
        Self { client, intervals, last: Mutex::new(HashMap::new()), cache_dir, cache_ttl: Duration::from_secs(cache_days as u64 * 86400), offline: false }
    }

    fn wait_turn(&self, host: &str) {
        let interval = self.intervals.get(host).copied().unwrap_or(Duration::from_millis(200));
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(t) = last.get(host) {
            let elapsed = t.elapsed();
            if elapsed < interval {
                std::thread::sleep(interval - elapsed);
            }
        }
        last.insert(host.to_string(), Instant::now());
    }

    fn cache_path(&self, url: &str, ext: &str) -> Option<PathBuf> {
        let dir = self.cache_dir.as_ref()?;
        Some(dir.join(format!("{}.{ext}", cache_key(url))))
    }

    fn cache_read(&self, url: &str, ext: &str) -> Option<Vec<u8>> {
        let p = self.cache_path(url, ext)?;
        let meta = std::fs::metadata(&p).ok()?;
        let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
        if now.saturating_sub(modified) > self.cache_ttl && !self.offline {
            return None;
        }
        std::fs::read(&p).ok()
    }

    fn cache_write(&self, url: &str, ext: &str, data: &[u8]) {
        if let Some(p) = self.cache_path(url, ext) {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(p, data);
        }
    }

    /// GET with optional extra headers; cached by URL.
    pub fn get_bytes(&self, url: &str, headers: &[(&str, &str)], ext: &str) -> Result<Vec<u8>> {
        if let Some(b) = self.cache_read(url, ext) {
            return Ok(b);
        }
        if self.offline {
            return Err(SourceError::Http { url: url.into(), msg: "offline".into() });
        }
        let host = host_of(url);
        let mut attempt = 0;
        loop {
            self.wait_turn(&host);
            let mut req = self.client.get(url);
            for (k, v) in headers {
                req = req.header(*k, *v);
            }
            let resp = req.send().map_err(|e| SourceError::Http { url: url.into(), msg: e.to_string() })?;
            let status = resp.status().as_u16();
            if (status == 503 || status == 429) && attempt < 4 {
                // rate limited: back off (2, 4, 6, 8 s) and retry
                attempt += 1;
                std::thread::sleep(Duration::from_millis(2000 * attempt as u64));
                continue;
            }
            if !(200..300).contains(&status) {
                return Err(SourceError::Status { url: url.into(), status });
            }
            let b = resp.bytes().map_err(|e| SourceError::Http { url: url.into(), msg: e.to_string() })?.to_vec();
            self.cache_write(url, ext, &b);
            return Ok(b);
        }
    }

    /// POST a form (cached by url + body like a GET; same rate limit and retries).
    pub fn post_form_json(&self, url: &str, form: &[(&str, &str)]) -> Result<serde_json::Value> {
        let body: String = form.iter().map(|(k, v)| format!("{}={}", enc(k), enc(v))).collect::<Vec<_>>().join("&");
        let cache_url = format!("{url}#{}", cache_key(&body));
        if let Some(b) = self.cache_read(&cache_url, "json") {
            return serde_json::from_slice(&b).map_err(|e| SourceError::Parse { url: url.into(), msg: e.to_string() });
        }
        if self.offline {
            return Err(SourceError::Http { url: url.into(), msg: "offline".into() });
        }
        let host = host_of(url);
        let mut attempt = 0;
        loop {
            self.wait_turn(&host);
            let resp = self.client.post(url).header("Accept", "application/json").header("Content-Type", "application/x-www-form-urlencoded").body(body.clone()).send().map_err(|e| SourceError::Http { url: url.into(), msg: e.to_string() })?;
            let status = resp.status().as_u16();
            if (status == 503 || status == 429) && attempt < 4 {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(2000 * attempt as u64));
                continue;
            }
            if !(200..300).contains(&status) {
                return Err(SourceError::Status { url: url.into(), status });
            }
            let b = resp.bytes().map_err(|e| SourceError::Http { url: url.into(), msg: e.to_string() })?.to_vec();
            self.cache_write(&cache_url, "json", &b);
            return serde_json::from_slice(&b).map_err(|e| SourceError::Parse { url: url.into(), msg: e.to_string() });
        }
    }

    pub fn get_json(&self, url: &str, headers: &[(&str, &str)]) -> Result<serde_json::Value> {
        let mut h: Vec<(&str, &str)> = vec![("Accept", "application/json")];
        h.extend_from_slice(headers);
        let b = self.get_bytes(url, &h, "json")?;
        serde_json::from_slice(&b).map_err(|e| SourceError::Parse { url: url.into(), msg: e.to_string() })
    }
}

/// Percent-encode a query component.
pub fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encoding_and_host() {
        assert_eq!(enc("a b&c=d/é"), "a%20b%26c%3Dd%2F%C3%A9");
        assert_eq!(host_of("https://musicbrainz.org/ws/2/release"), "musicbrainz.org");
        assert_eq!(cache_key("x").len(), 16);
    }
}
