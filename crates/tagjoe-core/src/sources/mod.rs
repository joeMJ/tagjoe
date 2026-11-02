//! Metadata sources: HTTP client with cache and rate limits, fingerprinting, AcoustID,
//! MusicBrainz, Cover Art Archive, and the identification pipeline.

pub mod acoustid;
pub mod audiobookdb;
pub mod audnexus;
pub mod books;
pub mod caa;
pub mod discogs;
pub mod dnb;
pub mod fingerprint;
pub mod http;
pub mod identify;
pub mod musicbrainz;

use std::fmt;

#[derive(Debug, Clone)]
pub enum SourceError {
    Http { url: String, msg: String },
    Status { url: String, status: u16 },
    Parse { url: String, msg: String },
    NotConfigured(String),
    Tool(String),
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceError::Http { url, msg } => write!(f, "request failed: {url}: {msg}"),
            SourceError::Status { url, status } => write!(f, "HTTP {status} for {url}"),
            SourceError::Parse { url, msg } => write!(f, "cannot parse response of {url}: {msg}"),
            SourceError::NotConfigured(what) => write!(f, "not configured: {what}"),
            SourceError::Tool(msg) => write!(f, "{msg}"),
        }
    }
}
impl std::error::Error for SourceError {}

impl SourceError {
    /// One-line description without the URL (for notes shown to the user).
    pub fn short(&self) -> String {
        match self {
            SourceError::Http { msg, .. } => format!("request failed: {msg}"),
            SourceError::Status { status: 429, .. } => "HTTP 429 (rate limit exceeded)".into(),
            SourceError::Status { status: 503, .. } => "HTTP 503 (service busy)".into(),
            SourceError::Status { status, .. } => format!("HTTP {status}"),
            SourceError::Parse { msg, .. } => format!("unreadable response: {msg}"),
            SourceError::NotConfigured(w) => format!("not configured: {w}"),
            SourceError::Tool(m) => m.clone(),
        }
    }
}

pub type Result<T> = std::result::Result<T, SourceError>;

/// Progress callback: (step, done, total).
pub type Progress<'a> = &'a (dyn Fn(&str, usize, usize) + Sync);
pub fn no_progress(_: &str, _: usize, _: usize) {}
