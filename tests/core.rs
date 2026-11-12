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
            duplicate_min: 1,
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
    let copies: Vec<_> = r
        .items
        .iter()
        .filter(|i| i.tags.contains(&Tag::Duplicate))
        .collect();
    assert_eq!(copies.len(), 2);
    assert_eq!(copies[0].keeper, copies[1].keeper);
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
fn tiny_archive_is_not_heavy() {
    let d = fixture();
    fs::write(d.path().join("note.zip"), b"tiny").unwrap();
    let r = scan(
        Options {
            roots: vec![d.path().into()],
            large: 500 * 1024 * 1024,
            old_days: 90,
            duplicate_min: 1024 * 1024,
        },
        Arc::new(AtomicBool::new(false)),
        |_| {},
    );
    assert!(r.items.is_empty());
}
#[test]
fn three_copies_are_all_choosable() {
    let d = fixture();
    fs::write(d.path().join("a"), b"same").unwrap();
    fs::write(d.path().join("b"), b"same").unwrap();
    fs::write(d.path().join("c"), b"same").unwrap();
    let r = run(d.path(), false);
    let copies: Vec<_> = r
        .items
        .iter()
        .filter(|i| i.tags.contains(&Tag::Duplicate))
        .cloned()
        .collect();
    assert_eq!(copies.len(), 3);
    assert!(copies.iter().all(|i| i.keeper == copies[0].keeper));
    let results = act(&copies[0..2], true, |_| Ok(()));
    assert!(results.iter().all(|(_, result)| result.is_ok()));
}
#[test]
fn shrink_keeps_the_file_type() {
    let name = shrink_filename("arquivaograndedocaralhopai.mp4", 24);
    assert!(name.ends_with(" .mp4"), "{name}");
    assert!(name.contains("..."), "{name}");
    assert!(name.chars().count() <= 24, "{name}");
    assert_eq!(shrink_filename("curto.mp4", 40), "curto.mp4");
    let packed = shrink_filename("backup-muito-longo-demais.tar.gz", 28);
    assert!(packed.ends_with(" .tar.gz"), "{packed}");
    assert!(shrink_filename("sem-tipo-e-um-nome-enorme", 16).ends_with("..."));
    assert!(shrink_filename("abcdefghijklmnop.mp4", 4).ends_with(".mp4"));
}
#[test]
fn changed_file_rejected() {
    let d = fixture();
    fs::write(d.path().join("a"), "a").unwrap();
    let r = run(d.path(), false);
    fs::write(d.path().join("a"), "changed").unwrap();
    assert!(validate(&r.items[0]).is_err());
}

#[test]
fn protected_root_reports_warning() {
    let report = run(std::path::Path::new("/usr"), false);
    assert_eq!(report.visited, 0);
    assert!(report.items.is_empty());
    assert!(report.errors.iter().any(|e| e.contains("Local protegido")));
}

#[test]
fn space_ranking_counts_small_files_and_overlapping_roots_once() {
    let d = fixture();
    let nested = d.path().join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(d.path().join("a"), [0; 12]).unwrap();
    fs::write(nested.join("b"), [1; 7]).unwrap();
    fs::write(nested.join("c"), [2; 5]).unwrap();
    std::os::unix::fs::symlink(&nested, d.path().join("link")).unwrap();
    let r = scan(
        Options {
            roots: vec![d.path().into(), nested.clone(), d.path().into()],
            large: 1000,
            old_days: 90,
            duplicate_min: 1000,
        },
        Arc::new(AtomicBool::new(false)),
        |_| {},
    );
    assert!(r.items.is_empty());
    assert_eq!(r.visited, 3);
    assert_eq!(
        r.top_files.iter().map(|e| e.size).collect::<Vec<_>>(),
        vec![12, 7, 5]
    );
    assert_eq!(r.top_folders.len(), 2);
    assert_eq!(r.top_folders[0].path, d.path().canonicalize().unwrap());
    assert_eq!(r.top_folders[0].size, 24);
    assert_eq!(r.top_folders[1].size, 12);
    assert!(r.top_folders.iter().all(|e| e.is_dir));
    assert!(r.top_files.iter().all(|e| !e.is_dir));
}

#[test]
fn space_ranking_limits_and_ties_are_deterministic() {
    let d = fixture();
    for i in 0..15 {
        let dir = d.path().join(format!("{i:02}"));
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("file"), [0; 3]).unwrap();
    }
    let r = run(d.path(), false);
    assert_eq!(r.top_files.len(), 10);
    assert_eq!(r.top_folders.len(), 10);
    assert_eq!(r.top_folders[0].size, 45);
    assert!(r
        .top_files
        .windows(2)
        .all(|pair| pair[0].path < pair[1].path));
    assert!(run(d.path(), true).top_files.is_empty());
}
