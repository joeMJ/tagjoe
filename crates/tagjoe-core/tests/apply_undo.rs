//! Apply a tag-derived plan to copies of the fixtures, verify by re-reading, then undo.

use std::path::PathBuf;
use tagjoe_core::apply::{apply, undo};
use tagjoe_core::config::Config;
use tagjoe_core::journal::Journal;
use tagjoe_core::model::Field;
use tagjoe_core::propose::{build_plan, identity_match, release_from_unit};
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
    unit
}

#[test]
fn apply_then_undo_roundtrip() {
    let unit_path = temp_copy("roundtrip");
    let jdir = unit_path.parent().unwrap().join("journal");
    let mut cfg = Config::default();
    cfg.templates.folder_enabled = true;

    let units = scan::scan_path(&unit_path).unwrap();
    let u = &units[0];
    let rel = release_from_unit(u);
    let plan = build_plan(u, &rel, &identity_match(u.files.len()), &cfg);
    assert!(plan.new_folder.is_some(), "folder renaming on");
    let r = apply(&plan, &cfg, &jdir);
    assert_eq!(r.errors, 0, "{:?}", r.files);
    let journal_path = r.journal.clone().expect("journal written");
    let new_unit = r.folder.as_ref().map(|(_, b)| b.clone()).expect("folder renamed");
    assert_eq!(new_unit.file_name().unwrap().to_string_lossy(), "Test Band - Fixture Album - 2020");

    // verify by re-reading
    let after = scan::scan_path(&new_unit).unwrap();
    let a = &after[0];
    let names: Vec<&str> = a.files.iter().map(|f| f.file_name.as_str()).collect();
    assert_eq!(names[0], "Test Band - Fixture Album - 2020 - 01 von 4 - Alpha.mp3");
    assert_eq!(names[3], "Test Band - Fixture Album - 2020 - 04 von 4 - Delta.opus");
    for f in &a.files {
        assert_eq!(f.get(Field::AlbumArtist), "Test Band", "{}", f.file_name);
        assert_eq!(f.get(Field::TrackTotal), "4", "{}", f.file_name);
        assert!(f.has_cover(), "cover embedded in {}", f.file_name);
        assert!(f.foreign.is_empty(), "foreign left in {}: {:?}", f.file_name, f.foreign);
        assert!(f.extra_tag_types.is_empty(), "secondary containers left in {}: {:?}", f.file_name, f.extra_tag_types);
    }
    assert_eq!(a.files[0].get(Field::Label), "Fixture Label");
    assert_eq!(a.files[0].pictures[0].width, 64);

    // undo
    let j = Journal::load(&journal_path).unwrap();
    let r = undo(&j, &cfg);
    assert_eq!(r.errors, 0, "{:?}", r.files);
    assert!(unit_path.exists(), "folder name restored");
    let back = scan::scan_path(&unit_path).unwrap();
    let b = &back[0];
    let names: Vec<&str> = b.files.iter().map(|f| f.file_name.as_str()).collect();
    assert_eq!(names, ["01-alpha.mp3", "02-beta.flac", "03-gamma.m4a", "04-delta.opus"]);
    assert_eq!(b.files[0].get(Field::AlbumArtist), "", "albumartist absent again");
    assert_eq!(b.files[0].get(Field::TrackTotal), "2", "old total restored");
    assert!(!b.files[0].has_cover(), "cover removed again");
    assert!(b.files[0].foreign.iter().any(|r| r.key == "TXXX:UPLOADED BY"), "foreign text restored: {:?}", b.files[0].foreign);
    assert!(b.files[1].foreign.iter().any(|r| r.key.eq_ignore_ascii_case("COPYRIGHT")));
    let _ = std::fs::remove_dir_all(unit_path.parent().unwrap());
}
