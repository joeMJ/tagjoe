//! Cover Art Archive: front cover for a release or release group.

use super::http::Http;
use super::Result;

pub struct Caa<'a> {
    pub http: &'a Http,
}

impl Caa<'_> {
    /// Front image URL for the release (`size` 250, 500, 1200 or 0 = original).
    pub fn release_front_url(release_id: &str, size: u32) -> String {
        if size == 0 { format!("https://coverartarchive.org/release/{release_id}/front") } else { format!("https://coverartarchive.org/release/{release_id}/front-{size}") }
    }
    pub fn release_group_front_url(rg_id: &str, size: u32) -> String {
        if size == 0 { format!("https://coverartarchive.org/release-group/{rg_id}/front") } else { format!("https://coverartarchive.org/release-group/{rg_id}/front-{size}") }
    }

    /// Fetch the front cover bytes (JPEG/PNG), trying the release, then its release group.
    pub fn front(&self, release_id: &str, release_group_id: Option<&str>, size: u32) -> Result<Option<(Vec<u8>, String)>> {
        let mut urls = vec![Self::release_front_url(release_id, size)];
        if let Some(rg) = release_group_id {
            urls.push(Self::release_group_front_url(rg, size));
        }
        for url in urls {
            match self.http.get_bytes(&url, &[], "img") {
                Ok(b) if !b.is_empty() => {
                    let mime = if b.starts_with(&[0x89, b'P', b'N', b'G']) { "image/png" } else { "image/jpeg" };
                    return Ok(Some((b, mime.to_string())));
                }
                Ok(_) => continue,
                Err(super::SourceError::Status { status: 404, .. }) => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }
}
