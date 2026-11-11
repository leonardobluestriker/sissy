use sissy::*;
use std::{
    fs,
    sync::{atomic::AtomicBool, Arc},
};
fn fixture() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("sissy-test-")
        .tempdir_in(std::env::var("HOME").unwrap())
        .unwrap()
}
fn run(p: &std::path::Path, c: bool) -> Report {
    scan(
        Options {
            roots: vec![p.into(), p.into()],
            large: 1,
            old_days: 90,
        },
        Arc::new(AtomicBool::new(c)),
        |_| {},
    )
}
#[test]
fn duplicate_content_and_roots() {
    let d = fixture();
    fs::write(d.path().join("a"), b"same").unwrap();
    fs::write(d.path().join("b"), b"same").unwrap();
    fs::write(d.path().join("c"), b"diff").unwrap();
    let r = run(d.path(), false);
    assert_eq!(r.visited, 3);
    assert_eq!(r.items.iter().filter(|i| i.keeper.is_some()).count(), 1);
}
#[test]
fn cancelled() {
    let d = fixture();
    fs::write(d.path().join("a"), b"data").unwrap();
    let r = run(d.path(), true);
    assert!(r.cancelled);
    assert_eq!(r.visited, 0);
}
#[test]
fn symlink_and_system_filtered() {
    let d = fixture();
    std::os::unix::fs::symlink("/etc", d.path().join("link")).unwrap();
    assert_eq!(run(d.path(), false).visited, 0);
    assert!(protected(std::path::Path::new("/System")));
}
#[test]
fn confirmation_and_partial_failure() {
    let d = fixture();
    for n in ["a", "b"] {
        fs::write(d.path().join(n), n).unwrap();
    }
    let r = run(d.path(), false);
    let mut calls = 0;
    assert!(act(&r.items, false, |_| {
        calls += 1;
        Ok(())
    })
    .is_empty());
    assert_eq!(calls, 0);
    let results = act(&r.items, true, |_| {
        calls += 1;
        if calls == 1 {
            Err(std::io::Error::other("simulada"))
        } else {
            Ok(())
        }
    });
    assert_eq!(results.len(), 2);
    assert!(results[0].1.is_err());
    assert!(results[1].1.is_ok());
}
#[test]
fn changed_file_rejected() {
    let d = fixture();
    fs::write(d.path().join("a"), "a").unwrap();
    let r = run(d.path(), false);
    fs::write(d.path().join("a"), "changed").unwrap();
    assert!(validate(&r.items[0]).is_err());
}
