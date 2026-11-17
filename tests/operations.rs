use sissy::{
    operations::{self, Action, Stage},
    *,
};
use std::{
    fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
fn fixture() -> tempfile::TempDir {
    tempfile::tempdir_in(std::env::var("HOME").unwrap()).unwrap()
}
fn report(path: &Path) -> Report {
    scan(
        Options {
            roots: vec![path.into()],
            large: 1,
            old_days: 90,
            duplicate_min: 1,
        },
        Arc::new(AtomicBool::new(false)),
        |_| {},
    )
}
#[test]
fn parent_child_selection_is_unified_and_counts_contents() {
    let root = fixture();
    fs::create_dir(root.path().join("child")).unwrap();
    fs::write(root.path().join("child/file"), b"payload").unwrap();
    let r = report(root.path());
    let items = r.all_folders.iter().chain(&r.all_files).cloned().collect();
    let plan = operations::prepare(items, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(plan.entries.len(), 1);
    assert_eq!((plan.folders, plan.files, plan.bytes), (2, 1, 7));
    assert!(root.path().join("child/file").exists());
}
#[test]
fn changed_nested_file_never_reaches_the_backend() {
    let root = fixture();
    fs::create_dir(root.path().join("child")).unwrap();
    fs::write(root.path().join("child/file"), b"payload").unwrap();
    let r = report(root.path());
    let plan = operations::prepare(
        vec![r.all_folders[0].clone()],
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    fs::write(root.path().join("child/file"), b"changed payload").unwrap();
    let mut calls = 0;
    let results = operations::execute_with(
        &plan,
        &Action::Trash,
        true,
        &AtomicBool::new(false),
        |_| {},
        |_, _| {
            calls += 1;
            Ok(())
        },
    );
    assert_eq!(calls, 0);
    assert!(results[0].result.is_err());
    assert!(root.path().join("child/file").exists());
}
#[test]
fn no_confirmation_and_cancellation_never_move_unapproved_items() {
    let root = fixture();
    fs::write(root.path().join("a"), b"a").unwrap();
    fs::write(root.path().join("b"), b"bb").unwrap();
    let plan = operations::prepare(
        report(root.path()).all_files,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let cancel = AtomicBool::new(false);
    let mut calls = 0;
    assert!(operations::execute_with(
        &plan,
        &Action::Transfer(root.path().join("unused")),
        false,
        &cancel,
        |_| {},
        |_, _| {
            calls += 1;
            Ok(())
        }
    )
    .is_empty());
    assert_eq!(calls, 0);
    assert!(!root.path().join("unused").exists());
    let mut final_progress = operations::Progress::default();
    let results = operations::execute_with(
        &plan,
        &Action::Trash,
        true,
        &cancel,
        |state| final_progress = state,
        |_, _| {
            calls += 1;
            cancel.store(true, Ordering::Relaxed);
            Ok(())
        },
    );
    assert_eq!(calls, 1);
    assert_eq!(results.len(), 1);
    assert_eq!(final_progress.stage, Stage::Finished);
    assert_eq!((final_progress.completed, final_progress.total), (1, 2));
}
#[test]
fn symlinks_and_home_are_not_accepted_as_folder_actions() {
    let root = fixture();
    fs::write(root.path().join("file"), b"a").unwrap();
    std::os::unix::fs::symlink("/etc", root.path().join("link")).unwrap();
    let r = report(root.path());
    assert!(operations::prepare(
        vec![r.all_folders[0].clone()],
        &AtomicBool::new(false),
        |_| {}
    )
    .is_err());
}
#[test]
fn transfer_folders_and_duplicate_filenames_are_numbered() {
    let root = fixture();
    let first = operations::next_transfer_directory(root.path()).unwrap();
    assert_eq!(first.file_name().unwrap(), "Transferências do Sissy");
    fs::create_dir(&first).unwrap();
    let second = operations::next_transfer_directory(root.path()).unwrap();
    assert_eq!(second.file_name().unwrap(), "Transferências do Sissy no. 2");
    fs::create_dir(&second).unwrap();
    assert_eq!(
        operations::next_transfer_directory(root.path())
            .unwrap()
            .file_name()
            .unwrap(),
        "Transferências do Sissy no. 3"
    );
    for folder in ["one", "two"] {
        fs::create_dir(root.path().join(folder)).unwrap();
        fs::write(root.path().join(folder).join("photo.jpg"), b"data").unwrap();
    }
    let plan = operations::prepare(
        report(root.path()).all_files,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(plan.entries[0].target_name, "photo.jpg");
    assert_eq!(plan.entries[1].target_name, "photo_1.jpg");
    assert!(operations::validate_destination(&plan, &root.path().join("new")).is_err());
}
#[test]
fn ranking_drills_into_all_descendants_and_progress_reports_each_phase() {
    let root = fixture();
    fs::create_dir(root.path().join("child")).unwrap();
    fs::write(root.path().join("child/a"), b"same").unwrap();
    fs::write(root.path().join("child/b"), b"same").unwrap();
    fs::write(root.path().join("other"), b"bigger than child files").unwrap();
    let events = std::cell::RefCell::new(Vec::new());
    let r = scan_with_progress(
        Options {
            roots: vec![root.path().into(), root.path().join("child")],
            large: 1,
            old_days: 90,
            duplicate_min: 1,
        },
        Arc::new(AtomicBool::new(false)),
        |state| events.borrow_mut().push(state),
    );
    assert_eq!((r.folders, r.visited), (2, 3));
    let within = root.path().join("child");
    let ranked = r.ranking(2, Some(&within));
    assert_eq!(ranked.len(), 2);
    assert!(ranked
        .iter()
        .all(|entry| !entry.is_dir && entry.path.starts_with(&within)));
    let events = events.into_inner();
    let last = events.last().unwrap();
    assert_eq!(last.phase, ScanPhase::Complete);
    assert_eq!((last.hash_total, last.hashed, last.hash_bytes), (2, 2, 8));
    assert!(events.iter().any(|s| s.phase == ScanPhase::Summarizing));
    assert!(events.iter().any(|s| s.phase == ScanPhase::Duplicates));
    assert!(events
        .windows(2)
        .all(|p| p[1].files >= p[0].files && p[1].folders >= p[0].folders));
}
/// Run only against a disposable mounted test filesystem, never personal data.
#[test]
#[ignore = "requires SISSY_TEST_VOLUME pointing to a disposable volume"]
fn native_cross_volume_transfer_preserves_payload_and_numbers_conflicts() {
    let destination =
        std::path::PathBuf::from(std::env::var("SISSY_TEST_VOLUME").expect("SISSY_TEST_VOLUME"));
    assert!(destination
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("SissyBetaTest"));
    let source = fixture();
    for folder in ["a", "b"] {
        fs::create_dir(source.path().join(folder)).unwrap();
        fs::write(
            source.path().join(folder).join("same.txt"),
            folder.as_bytes(),
        )
        .unwrap();
    }
    fs::create_dir(source.path().join("whole")).unwrap();
    fs::write(source.path().join("whole/content"), b"folder payload").unwrap();
    fs::write(source.path().join("whole/férias.txt"), b"unicode payload").unwrap();
    let r = report(source.path());
    let mut items: Vec<_> = r
        .all_files
        .iter()
        .filter(|e| e.path.file_name().unwrap() == "same.txt")
        .cloned()
        .collect();
    items.push(
        r.all_folders
            .iter()
            .find(|e| e.path.file_name().unwrap() == "whole")
            .unwrap()
            .clone(),
    );
    let plan = operations::prepare(items, &AtomicBool::new(false), |_| {}).unwrap();
    let first = operations::next_transfer_directory(&destination).unwrap();
    fs::create_dir(&first).unwrap();
    fs::write(first.join("untouched"), b"keep me").unwrap();
    let planned = operations::next_transfer_directory(&destination).unwrap();
    let results = operations::execute(
        &plan,
        &Action::Transfer(planned.clone()),
        true,
        &AtomicBool::new(false),
        |_| {},
    );
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|r| r.result.is_ok()), "{results:?}");
    assert_eq!(fs::read(planned.join("same.txt")).unwrap(), b"a");
    assert_eq!(fs::read(planned.join("same_1.txt")).unwrap(), b"b");
    assert_eq!(
        fs::read(planned.join("whole/content")).unwrap(),
        b"folder payload"
    );
    assert_eq!(fs::read(first.join("untouched")).unwrap(), b"keep me");
    assert!(!source.path().join("a/same.txt").exists());
    assert!(!source.path().join("whole").exists());
}
