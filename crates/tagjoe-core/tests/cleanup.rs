//! Folder clean-up: NFO/M3U/images/TXT and extra extensions are deleted after a clean run,
//! never after errors; the progress callback reports every step.

use std::path::PathBuf;
use tagjoe_core::apply::{apply_with_progress, Progress};
use tagjoe_core::config::Config;
use tagjoe_core::journal::Journal;
use tagjoe_core::propose::{build_plan, cleanup_candidates, identity_match, release_from_unit, FilePlan};
use tagjoe_core::scan;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn temp_copy(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tagjoe-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let unit = dir.join("Fixture Album");
    std::fs::create_dir_all(&unit).unwrap();
    for e in std::fs::read_dir(fixtures()).unwrap().flatten() {
        if e.path().is_file() {
            std::fs::copy(e.path(), unit.join(e.file_name())).unwrap();
        }
    }
    for extra in ["album.nfo", "Playlist.M3U", "notes.txt", "rip.cue", "readme.pdf", ".hidden.nfo"] {
        std::fs::write(unit.join(extra), b"x").unwrap();
    }
    unit
}

fn names(dir: &std::path::Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
    v.sort();
    v
}

#[test]
fn candidates_follow_the_config() {
    let unit_path = temp_copy("cands");
    let u = &scan::scan_path(&unit_path).unwrap()[0];
    let mut cfg = Config::default();
    let c: Vec<String> = cleanup_candidates(u, &cfg).iter().map(|p| p.file_name().unwrap().to_string_lossy().to_string()).collect();
    assert_eq!(c, ["Playlist.M3U", "album.nfo", "cover.jpg", "notes.txt", "rip.cue"], "defaults: four groups plus extras, no pdf, no hidden, no audio");
    cfg.clean.images = false;
    cfg.clean.extra = vec![];
    let c: Vec<String> = cleanup_candidates(u, &cfg).iter().map(|p| p.file_name().unwrap().to_string_lossy().to_string()).collect();
    assert_eq!(c, ["Playlist.M3U", "album.nfo", "notes.txt"]);
    cfg.clean.enabled = false;
    assert!(cleanup_candidates(u, &cfg).is_empty());
    let _ = std::fs::remove_dir_all(unit_path.parent().unwrap());
}

#[test]
fn deletes_after_clean_run_and_reports_progress() {
    let unit_path = temp_copy("delete");
    let jdir = unit_path.parent().unwrap().join("journal");
    let mut cfg = Config::default();
    cfg.templates.folder_enabled = false; // keep the folder name, the test lists it afterwards
    let u = &scan::scan_path(&unit_path).unwrap()[0];
    let rel = release_from_unit(u);
    let mut plan = build_plan(u, &rel, &identity_match(u.files.len()), &cfg);
    assert_eq!(plan.deletions(), 5);
    // the user keeps the playlist
    plan.delete_files.iter_mut().find(|d| d.path.ends_with("Playlist.M3U")).unwrap().apply = false;
    let mut all: Vec<Progress> = vec![];
    let r = apply_with_progress(&plan, &cfg, &jdir, &mut |p| all.push(p.clone()));
    assert_eq!(r.errors, 0, "{:?}", r.files);
    let starts: Vec<&Progress> = all.iter().filter(|p| !p.finished).collect();
    let steps: Vec<&Progress> = all.iter().filter(|p| p.finished).collect();
    assert_eq!(starts.len(), 4, "every audio file is announced before it is written");
    assert_eq!(starts[0].done, 0);
    assert_eq!(starts[0].message, "writing");
    assert_eq!(steps.len(), 4 + 4, "four audio files and four deletions reported");
    assert!(all.iter().all(|s| s.total == 8 && s.ok));
    assert_eq!(steps.last().unwrap().done, 8);
    assert_eq!(r.deleted.len(), 4);
    let left = names(&unit_path);
    assert!(left.contains(&"Playlist.M3U".to_string()), "{left:?}");
    assert!(left.contains(&"readme.pdf".to_string()));
    assert!(left.contains(&".hidden.nfo".to_string()));
    assert!(!left.iter().any(|n| n == "album.nfo" || n == "notes.txt" || n == "rip.cue" || n == "cover.jpg"), "{left:?}");
    // cover was embedded before cover.jpg went away
    let after = scan::scan_path(&unit_path).unwrap();
    assert!(after[0].files.iter().all(|f| f.has_cover()));
    let j = Journal::load(&r.journal.clone().unwrap()).unwrap();
    assert_eq!(j.deleted.len(), 4);
    let _ = std::fs::remove_dir_all(unit_path.parent().unwrap());
}

#[test]
fn keeps_everything_after_an_error() {
    let unit_path = temp_copy("error");
    let jdir = unit_path.parent().unwrap().join("journal");
    let mut cfg = Config::default();
    cfg.templates.folder_enabled = false;
    let u = &scan::scan_path(&unit_path).unwrap()[0];
    let rel = release_from_unit(u);
    let mut plan = build_plan(u, &rel, &identity_match(u.files.len()), &cfg);
    // a file that cannot be written
    let mut broken: FilePlan = plan.files[0].clone();
    broken.path = unit_path.join("missing.mp3");
    broken.file_name = "missing.mp3".into();
    plan.files.push(broken);
    let before = names(&unit_path);
    let r = apply_with_progress(&plan, &cfg, &jdir, &mut |_| {});
    assert!(r.errors >= 1);
    assert!(r.deleted.is_empty());
    let after = names(&unit_path);
    let non_audio = |v: &Vec<String>| v.iter().filter(|n| !n.ends_with(".mp3") && !n.ends_with(".flac") && !n.ends_with(".m4a") && !n.ends_with(".opus")).cloned().collect::<Vec<_>>();
    assert_eq!(non_audio(&before), non_audio(&after), "nothing deleted after an error");
    let _ = std::fs::remove_dir_all(unit_path.parent().unwrap());
}
