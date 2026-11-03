//! Integration test on the silent fixture files in tests/fixtures (generated with ffmpeg).

use std::path::PathBuf;
use tagjoe_core::config::Config;
use tagjoe_core::model::{Field, Kind};
use tagjoe_core::propose::{build_plan, identity_match, release_from_unit};
use tagjoe_core::scan;
use tagjoe_core::schema::{check_file, Status};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

#[test]
fn scans_all_formats() {
    let units = scan::scan_path(&fixtures()).unwrap();
    assert_eq!(units.len(), 1);
    let u = &units[0];
    assert_eq!(u.kind, Kind::Album);
    assert_eq!(u.files.len(), 4);
    assert_eq!(u.cover_files.len(), 1);
    let names: Vec<&str> = u.files.iter().map(|f| f.file_name.as_str()).collect();
    assert_eq!(names, ["01-alpha.mp3", "02-beta.flac", "03-gamma.m4a", "04-delta.opus"]);

    let mp3 = &u.files[0];
    assert_eq!(mp3.format, "MP3");
    assert_eq!(mp3.get(Field::Title), "Alpha");
    assert_eq!(mp3.get(Field::TrackNumber), "1");
    assert_eq!(mp3.get(Field::TrackTotal), "2");
    assert_eq!(mp3.get(Field::Label), "Fixture Label", "TPUB is read as label");
    assert_eq!(mp3.get(Field::Isrc), "DE1234567890");
    assert_eq!(mp3.extra_tag_types, vec!["ID3v1".to_string()]);
    let keys: Vec<&str> = mp3.foreign.iter().map(|r| r.key.as_str()).collect();
    assert!(keys.contains(&"TXXX:UPLOADED BY"), "{keys:?}");
    assert!(!keys.iter().any(|k| k.starts_with("TIT2") || k.starts_with("TPUB")), "{keys:?}");
    assert!((mp3.duration_secs - 1.2).abs() < 0.3);

    let flac = &u.files[1];
    assert_eq!(flac.format, "FLAC");
    assert_eq!(flac.get(Field::TrackTotal), "2");
    assert!(flac.foreign.iter().any(|r| r.key.eq_ignore_ascii_case("COPYRIGHT")));

    let m4a = &u.files[2];
    assert_eq!(m4a.format, "MP4");
    assert_eq!(m4a.get(Field::Album), "Fixture Album");
    let keys: Vec<&str> = m4a.foreign.iter().map(|r| r.key.as_str()).collect();
    assert!(!keys.iter().any(|k| k.contains("nam") || k.contains("alb")), "known atoms must not be foreign: {keys:?}");

    let opus = &u.files[3];
    assert_eq!(opus.format, "Opus");
    assert_eq!(opus.get(Field::Title), "Delta");
}

#[test]
fn check_and_plan() {
    let cfg = Config::default();
    let units = scan::scan_path(&fixtures()).unwrap();
    let u = &units[0];
    let r = check_file(&u.files[0], u.kind, &cfg);
    assert!(!r.complete);
    assert!(r.fields.iter().any(|c| c.field == Field::AlbumArtist && c.status == Status::Missing));
    assert!(!r.foreign.iter().any(|x| x.key == "publisher"), "publisher twin of label is not foreign");
    assert_eq!(r.containers, vec!["ID3v1".to_string()]);

    let rel = release_from_unit(u);
    assert_eq!(rel.album.get(&Field::AlbumArtist).unwrap(), "Test Band");
    let plan = build_plan(u, &rel, &identity_match(u.files.len()), &cfg);
    assert_eq!(plan.files[0].new_name.as_deref(), Some("Test Band - Fixture Album - 2020 - 01 von 4 - Alpha.mp3"));
    assert_eq!(plan.files[3].new_name.as_deref(), Some("Test Band - Fixture Album - 2020 - 04 von 4 - Delta.opus"));
    let cover = plan.files[0].cover.as_ref().expect("cover from folder image");
    assert!(cover.apply && cover.width == 64);
    assert!(plan.files[0].removals.iter().any(|r| r.key == "TXXX:UPLOADED BY" && r.apply));
}

#[test]
fn scan_reports_progress_and_units() {
    use tagjoe_core::scan::{scan_path_with, ScanEvent};
    let mut total = None;
    let mut files = vec![];
    let mut units = 0;
    scan_path_with(&fixtures(), &mut |e| match e {
        ScanEvent::Total(n) => total = Some(n),
        ScanEvent::File { done, total, path } => files.push((done, total, path)),
        ScanEvent::Unit(_) => units += 1,
    })
    .unwrap();
    assert_eq!(total, Some(4), "four audio fixtures counted before reading");
    assert_eq!(files.len(), 4);
    assert_eq!(files.last().unwrap().0, 4);
    assert!(files.iter().all(|f| f.1 == 4));
    assert_eq!(units, 1);
}
