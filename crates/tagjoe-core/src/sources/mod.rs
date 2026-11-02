//! Metadata sources: HTTP client with cache and rate limits, fingerprinting, AcoustID,
//! MusicBrainz, Cover Art Archive, and the identification pipeline.

pub mod acoustid;
pub mod caa;
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

pub type Result<T> = std::result::Result<T, SourceError>;

/// Progress callback: (step, done, total).
pub type Progress<'a> = &'a (dyn Fn(&str, usize, usize) + Sync);
pub fn no_progress(_: &str, _: usize, _: usize) {}
