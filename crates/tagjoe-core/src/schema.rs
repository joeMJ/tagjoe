//! Check every tag field of a file against the schema; decide what is foreign.

use crate::config::Config;
use crate::model::{Field, FileInfo, Kind, RawItem};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Present and equal to the expected value.
    Ok,
    /// Required (or expected) but absent.
    Missing,
    /// Present but different from the expected value.
    Differs,
    /// Not part of the schema (or a secondary container).
    Foreign,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Missing => "missing",
            Status::Differs => "differs",
            Status::Foreign => "foreign",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldCheck {
    pub field: Field,
    pub value: String,
    pub status: Status,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileReport {
    pub path: std::path::PathBuf,
    pub fields: Vec<FieldCheck>,
    /// Foreign items that would be removed (after the keep list).
    pub foreign: Vec<RawItem>,
    /// Foreign items kept because of the keep list.
    pub kept: Vec<RawItem>,
    /// Secondary containers (ID3v1, APE) that would be removed.
    pub containers: Vec<String>,
    pub has_cover: bool,
    pub complete: bool,
}

/// Case-insensitive match with `*` as wildcard (prefix, suffix or infix).
pub fn keep_matches(pattern: &str, key: &str) -> bool {
    let p = pattern.trim().to_ascii_lowercase();
    let k = key.to_ascii_lowercase();
    if p.is_empty() {
        return false;
    }
    let parts: Vec<&str> = p.split('*').collect();
    if parts.len() == 1 {
        return k == p;
    }
    let mut pos = 0usize;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        match k[pos..].find(part) {
            Some(idx) => {
                if i == 0 && idx != 0 {
                    return false;
                }
                pos += idx + part.len();
            }
            None => return false,
        }
    }
    if let Some(last) = parts.last() {
        if !last.is_empty() && !k.ends_with(last) {
            return false;
        }
    }
    true
}

pub fn is_kept(cfg: &Config, key: &str) -> bool {
    cfg.tags.keep_fields.iter().any(|p| keep_matches(p, key))
}

/// Split foreign items into (to remove, to keep). Fields outside the kind's schema count as foreign too.
pub fn foreign_items(file: &FileInfo, kind: Kind, cfg: &Config) -> (Vec<RawItem>, Vec<RawItem>) {
    let mut remove = Vec::new();
    let mut keep = Vec::new();
    for item in &file.foreign {
        if is_kept(cfg, &item.key) { keep.push(item.clone()) } else { remove.push(item.clone()) }
    }
    let schema = Field::schema(kind);
    for (f, v) in &file.fields {
        if !schema.contains(f) {
            // ID3 stores label and publisher in one frame (TPUB): the twin of a schema field is not foreign
            let twin = match f {
                Field::Publisher => Some(Field::Label),
                Field::Label => Some(Field::Publisher),
                _ => None,
            };
            if let Some(t) = twin {
                if schema.contains(&t) && file.get(t) == v {
                    continue;
                }
            }
            let item = RawItem { key: f.key().to_string(), value: v.clone() };
            if is_kept(cfg, &item.key) { keep.push(item) } else { remove.push(item) }
        }
    }
    (remove, keep)
}

/// Check a file without an external reference: present/missing/foreign only.
pub fn check_file(file: &FileInfo, kind: Kind, cfg: &Config) -> FileReport {
    let required = Field::required(kind);
    let mut fields = Vec::new();
    for f in Field::schema(kind) {
        let v = file.get(*f);
        let req = required.contains(f);
        if v.is_empty() {
            if req {
                fields.push(FieldCheck { field: *f, value: String::new(), status: Status::Missing, required: true });
            }
        } else {
            fields.push(FieldCheck { field: *f, value: v.to_string(), status: Status::Ok, required: req });
        }
    }
    let (foreign, kept) = foreign_items(file, kind, cfg);
    let containers = if cfg.tags.remove_secondary_containers { file.extra_tag_types.clone() } else { vec![] };
    let has_cover = file.has_cover();
    let complete = fields.iter().all(|c| c.status == Status::Ok) && foreign.is_empty() && containers.is_empty() && has_cover;
    FileReport { path: file.path.clone(), fields, foreign, kept, containers, has_cover, complete }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keep_patterns() {
        assert!(keep_matches("REPLAYGAIN_*", "replaygain_track_gain"));
        assert!(keep_matches("*_gain", "REPLAYGAIN_TRACK_GAIN"));
        assert!(keep_matches("comment", "COMMENT"));
        assert!(!keep_matches("comment", "COMMENTS"));
        assert!(keep_matches("TXXX:*rating*", "TXXX:MyRating"));
        assert!(!keep_matches("", "x"));
    }
}
