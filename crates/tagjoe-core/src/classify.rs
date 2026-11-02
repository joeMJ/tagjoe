//! Guess whether a unit is an album, a compilation or an audiobook.

use crate::model::{Field, Kind, Unit};
use std::collections::BTreeSet;

pub const VARIOUS: &[&str] = &["various artists", "various", "va", "v.a.", "verschiedene interpreten", "verschiedene", "diverse interpreten", "diverse"];

pub fn is_various(s: &str) -> bool {
    VARIOUS.contains(&s.trim().to_ascii_lowercase().as_str())
}

fn norm(s: &str) -> String {
    s.trim().to_lowercase()
}

pub fn classify(unit: &Unit) -> Kind {
    if unit.files.is_empty() {
        return Kind::Unknown;
    }
    let path_l = unit.path.to_string_lossy().to_lowercase();
    let genres: Vec<String> = unit.files.iter().map(|f| norm(f.get(Field::Genre))).collect();
    let book_words = ["audiobook", "hörbuch", "hoerbuch", "hörspiel", "hoerspiel", "spoken", "speech", "lesung", "audio book", "kapitel", "chapter"];
    let genre_book = genres.iter().filter(|g| book_words.iter().any(|w| g.contains(w))).count();
    let path_book = book_words.iter().any(|w| path_l.contains(w));
    let narrator = unit.files.iter().filter(|f| !f.get(Field::Narrator).is_empty()).count();
    if genre_book * 2 >= unit.files.len() || path_book || narrator * 2 >= unit.files.len() {
        return Kind::Audiobook;
    }
    let flag = unit.files.iter().filter(|f| matches!(f.get(Field::Compilation), "1" | "true" | "yes")).count();
    let aa_various = unit.files.iter().filter(|f| is_various(f.get(Field::AlbumArtist))).count();
    let name_va = {
        let n = unit.name().to_lowercase();
        n.starts_with("va-") || n.starts_with("va_") || n.starts_with("va ") || n.starts_with("various")
    };
    if flag * 2 >= unit.files.len() || aa_various * 2 >= unit.files.len() || name_va {
        return Kind::Compilation;
    }
    // chapter-like titles ("Kapitel 03", "Chapter 3", "Track 12")
    let chapter_titles = unit.files.iter().filter(|f| looks_like_chapter(f.get(Field::Title))).count();
    if chapter_titles * 2 >= unit.files.len() && unit.files.len() >= 3 {
        return Kind::Audiobook;
    }
    // one narrator-less reading: one artist, one album, many files or very long, no MusicBrainz ids
    let has_mb = unit.files.iter().any(|f| !f.get(Field::MbReleaseId).is_empty());
    let albums: BTreeSet<String> = unit.files.iter().map(|f| norm(f.get(Field::Album))).filter(|a| !a.is_empty()).collect();
    let artists_all: BTreeSet<String> = unit.files.iter().map(|f| norm(f.get(Field::Artist))).filter(|a| !a.is_empty()).collect();
    let total_min = unit.total_secs() / 60.0;
    let avg = unit.total_secs() / unit.files.len() as f64;
    if !has_mb && albums.len() <= 1 && artists_all.len() == 1 && (unit.files.len() >= 15 || total_min > 150.0 || (unit.files.len() >= 6 && avg > 8.0 * 60.0)) {
        return Kind::Audiobook;
    }
    // varying track artists without a named album artist → compilation; with one (DJ mix) → album
    let aa_named = unit.files.iter().filter(|f| !f.get(Field::AlbumArtist).trim().is_empty()).count();
    if unit.files.len() >= 3 && artists_all.len() * 2 > unit.files.len() && aa_named * 2 < unit.files.len() {
        return Kind::Compilation;
    }
    Kind::Album
}

fn looks_like_chapter(title: &str) -> bool {
    let t = title.trim().to_lowercase();
    for w in ["kapitel", "chapter", "track", "teil", "part"] {
        if let Some(rest) = t.strip_prefix(w) {
            let rest = rest.trim_start_matches([' ', '.', '-', '_', ':']);
            let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
            if digits > 0 {
                return true;
            }
        }
    }
    // "001 - Der Sohn": leading number of 2–3 digits followed by separator
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    (2..=3).contains(&digits) && t[digits..].trim_start().starts_with(['-', '.', '_'])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chapter_titles() {
        assert!(looks_like_chapter("Kapitel 01 - Das Meisterwerk"));
        assert!(looks_like_chapter("Chapter 3"));
        assert!(looks_like_chapter("001 - Der Sohn"));
        assert!(!looks_like_chapter("Will We Ever Learn"));
        assert!(!looks_like_chapter("2024"));
    }
}
