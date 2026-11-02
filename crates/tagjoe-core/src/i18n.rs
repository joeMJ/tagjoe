//! UI languages. English is the base language: every text is written in English in the source
//! and doubles as the `msgid` of the gettext catalogues in `po/<lang>.po`. Catalogues are
//! embedded and read here without libintl (ported from the user's tvjoe project); a
//! `<lang>.po` in the `languages/` folder of the configuration directory takes precedence.
//!
//! - [`tr`] for fixed texts, [`tf!`](crate::tf) for texts with named placeholders (`{name}`),
//!   [`trn`] for singular/plural. The language is chosen once at start ([`init`]).

use std::collections::HashMap;
use std::fmt::Display;
use std::path::Path;
use std::sync::OnceLock;

/// Embedded catalogues (language code, contents of the `.po`).
const BUILTIN: &[(&str, &str)] = &[("de", include_str!("../../../po/de.po"))];

/// Selectable languages (code, name in its own language); empty code = system language.
pub const LANGUAGES: &[(&str, &str)] = &[("en", "English"), ("de", "Deutsch")];

const BASE: &str = "en";

struct Catalog {
    lang: String,
    map: HashMap<String, Vec<String>>,
}

static CATALOG: OnceLock<Catalog> = OnceLock::new();

/// Choose the language. `choice`: setting (empty = system), `system`: the system's preferred
/// languages (e.g. `de_DE.UTF-8`, `en`), `override_dir`: folder with own catalogues.
/// Only the first call takes effect; returns the chosen code.
pub fn init(choice: &str, system: &[String], override_dir: Option<&Path>) -> String {
    let lang = pick(choice, system, override_dir);
    let mut map = HashMap::new();
    if lang != BASE {
        let own = override_dir.map(|d| d.join(format!("{lang}.po"))).and_then(|p| std::fs::read_to_string(p).ok());
        let builtin = BUILTIN.iter().find(|(l, _)| *l == lang).map(|(_, s)| *s);
        if let Some(b) = builtin {
            map = parse_po(b);
        }
        if let Some(o) = own {
            map.extend(parse_po(&o));
        }
    }
    CATALOG.get_or_init(|| Catalog { lang, map }).lang.clone()
}

/// Initialize from the environment and the config value (convenience for the binaries).
pub fn init_from_env(choice: &str) -> String {
    init(choice, &env_languages(), Some(&crate::config::config_dir().join("languages")))
}

/// Chosen language (before [`init`]: English).
pub fn lang() -> &'static str {
    CATALOG.get().map(|c| c.lang.as_str()).unwrap_or(BASE)
}

fn pick(choice: &str, system: &[String], override_dir: Option<&Path>) -> String {
    let available = |l: &str| l == BASE || BUILTIN.iter().any(|(b, _)| *b == l) || override_dir.is_some_and(|d| d.join(format!("{l}.po")).is_file());
    let choice = choice.trim();
    if !choice.is_empty() && available(choice) {
        return choice.to_string();
    }
    for s in system {
        let l = s.split(['_', '.', '@', '-']).next().unwrap_or("").to_ascii_lowercase();
        if l.len() >= 2 && l != "c" && l != "posix" && available(&l) {
            return l;
        }
    }
    BASE.into()
}

/// Preferred languages from the environment (`LANGUAGE`, `LC_ALL`, `LC_MESSAGES`, `LANG`).
pub fn env_languages() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(v) = std::env::var("LANGUAGE") {
        out.extend(v.split(':').filter(|s| !s.is_empty()).map(String::from));
    }
    for k in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(k) {
            if !v.is_empty() {
                out.push(v);
                break;
            }
        }
    }
    out
}

/// Translation of a fixed text.
pub fn tr(s: &str) -> String {
    match CATALOG.get().and_then(|c| c.map.get(s)).and_then(|v| v.first()) {
        Some(t) if !t.is_empty() => t.clone(),
        _ => s.to_string(),
    }
}

/// Singular or plural depending on `n`; `{n}` is replaced.
pub fn trn(singular: &str, plural: &str, n: u64) -> String {
    let form = plural_form(lang(), n);
    let text = CATALOG.get().and_then(|c| c.map.get(singular)).and_then(|v| v.get(form)).filter(|t| !t.is_empty()).cloned().unwrap_or_else(|| if n == 1 { singular.to_string() } else { plural.to_string() });
    text.replace("{n}", &n.to_string())
}

fn plural_form(lang: &str, n: u64) -> usize {
    match lang {
        "fr" => usize::from(n > 1),
        _ => usize::from(n != 1),
    }
}

/// Replace named placeholders `{name}` (for [`tf!`](crate::tf)).
pub fn fill(text: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = text.to_string();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), &value.to_string());
    }
    out
}

/// Translated text with named placeholders: `tf!("Written: {n} files", n = count)`.
#[macro_export]
macro_rules! tf {
    ($text:literal $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::i18n::fill(
            &$crate::i18n::tr($text),
            &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),*],
        )
    };
}

/// Read a gettext catalogue: `msgid` → `msgstr` / `msgstr[i]`. Fuzzy and empty entries and the
/// header are skipped.
fn parse_po(text: &str) -> HashMap<String, Vec<String>> {
    #[derive(PartialEq)]
    enum Field {
        None,
        Ctxt,
        Id,
        IdPlural,
        Str(usize),
    }
    let mut map = HashMap::new();
    let mut fuzzy = false;
    let mut id = String::new();
    let mut strs: Vec<String> = Vec::new();
    let mut field = Field::None;
    let mut flush = |id: &mut String, strs: &mut Vec<String>, fuzzy: &mut bool| {
        if !id.is_empty() && !*fuzzy && strs.iter().any(|s| !s.is_empty()) {
            map.insert(std::mem::take(id), std::mem::take(strs));
        }
        id.clear();
        strs.clear();
        *fuzzy = false;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(c) = line.strip_prefix("#,") {
            if field != Field::None {
                flush(&mut id, &mut strs, &mut fuzzy);
                field = Field::None;
            }
            fuzzy |= c.split(',').any(|f| f.trim() == "fuzzy");
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        let (key, rest) = match line.find(' ') {
            Some(i) if !line.starts_with('"') => (&line[..i], line[i..].trim()),
            _ => ("", line),
        };
        let value = unquote(rest);
        match key {
            "msgctxt" => {
                if field != Field::None {
                    flush(&mut id, &mut strs, &mut fuzzy);
                }
                field = Field::Ctxt;
            }
            "msgid" => {
                if field != Field::Ctxt && field != Field::None {
                    flush(&mut id, &mut strs, &mut fuzzy);
                }
                id = value;
                field = Field::Id;
            }
            "msgid_plural" => field = Field::IdPlural,
            k if k.starts_with("msgstr") => {
                let i = k.strip_prefix("msgstr[").and_then(|r| r.strip_suffix(']')).and_then(|n| n.parse().ok()).unwrap_or(0);
                if strs.len() <= i {
                    strs.resize(i + 1, String::new());
                }
                strs[i] = value;
                field = Field::Str(i);
            }
            "" => match field {
                Field::Id => id.push_str(&value),
                Field::Str(i) => strs[i].push_str(&value),
                _ => {}
            },
            _ => {}
        }
    }
    flush(&mut id, &mut strs, &mut fuzzy);
    map
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')).unwrap_or(s);
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(o) => out.push(o),
            None => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PO: &str = r#"
msgid ""
msgstr ""
"Content-Type: text/plain; charset=UTF-8\n"

msgid "Written: {n} files"
msgstr "Geschrieben: {n} Dateien"

msgid ""
"Two "
"lines"
msgstr "Zwei\n"
"Zeilen"

#, fuzzy
msgid "Fuzzy"
msgstr "Unscharf"

msgid "Empty"
msgstr ""

msgid "one file"
msgid_plural "{n} files"
msgstr[0] "eine Datei"
msgstr[1] "{n} Dateien"
"#;

    #[test]
    fn reads_po() {
        let m = parse_po(PO);
        assert_eq!(m["Written: {n} files"], vec!["Geschrieben: {n} Dateien"]);
        assert_eq!(m["Two lines"], vec!["Zwei\nZeilen"]);
        assert!(!m.contains_key("Fuzzy"));
        assert!(!m.contains_key("Empty"));
        assert_eq!(m["one file"][1], "{n} Dateien");
    }

    #[test]
    fn builtin_catalogs_parse() {
        for (_, text) in BUILTIN {
            assert!(!parse_po(text).is_empty());
        }
    }

    #[test]
    fn picks_language() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(pick("", &s(&["de_DE.UTF-8"]), None), "de");
        assert_eq!(pick("", &s(&["en_GB.UTF-8"]), None), "en");
        assert_eq!(pick("", &s(&["fr_FR.UTF-8"]), None), "en");
        assert_eq!(pick("", &s(&["C"]), None), "en");
        assert_eq!(pick("de", &s(&["en_US"]), None), "de");
        assert_eq!(pick("xx", &s(&["de_AT"]), None), "de");
        assert_eq!(fill("{a} and {b}", &[("a", &1), ("b", &"x")]), "1 and x");
    }
}
