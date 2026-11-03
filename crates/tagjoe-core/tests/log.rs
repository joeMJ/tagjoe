//! Activity log: append, cap, tail, clear (own binary: it sets XDG_CONFIG_HOME).

use tagjoe_core::log;

#[test]
fn append_tail_cap_clear() {
    let dir = std::env::temp_dir().join(format!("tagjoe-test-log-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // only this test runs in this binary
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &dir) };
    assert!(log::tail(10).is_empty());
    log::append(&[log::line("/a", "x.mp3", "written")]).unwrap();
    let t = log::tail(10);
    assert_eq!(t.len(), 1);
    assert!(t[0].ends_with("\t/a\tx.mp3\twritten"), "{t:?}");
    assert_eq!(t[0].split('\t').next().unwrap().len(), 19, "local time stamp");
    let many: Vec<String> = (0..log::MAX_LINES + 5).map(|i| format!("line {i}")).collect();
    log::append(&many).unwrap();
    let t = log::tail(usize::MAX);
    assert_eq!(t.len(), log::MAX_LINES);
    assert_eq!(t.last().unwrap(), &format!("line {}", log::MAX_LINES + 4));
    assert_eq!(t[0], "line 5", "oldest lines dropped");
    log::clear().unwrap();
    assert!(log::tail(10).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
