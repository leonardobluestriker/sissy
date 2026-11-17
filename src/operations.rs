//! Explicit, revalidated Finder actions. No recursive deletion or overwrite fallback.
use crate::{protected, SpaceEntry};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::SystemTime,
};

fn error(message: &str) -> io::Error {
    io::Error::other(crate::i18n::text(message).to_owned())
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Node {
    relative: PathBuf,
    relative_key: String,
    size: u64,
    modified: SystemTime,
    dir: bool,
    identity: (u64, u64),
}
#[derive(Clone, Debug)]
pub struct PreparedEntry {
    pub item: SpaceEntry,
    pub target_name: std::ffi::OsString,
    nodes: Vec<Node>,
}
#[derive(Clone, Debug, Default)]
pub struct PreparedBatch {
    pub entries: Vec<PreparedEntry>,
    pub files: usize,
    pub folders: usize,
    pub bytes: u64,
}
#[derive(Clone, Debug)]
pub enum Action {
    Trash,
    Transfer(PathBuf),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    #[default]
    Preparing,
    Validating,
    Trashing,
    Moving,
    Verifying,
    Finished,
}
#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub stage: Stage,
    pub completed: usize,
    pub total: usize,
    pub files: usize,
    pub folders: usize,
}
#[derive(Debug)]
pub struct Outcome {
    pub source: PathBuf,
    pub target: Option<PathBuf>,
    pub result: Result<(), String>,
}

pub fn next_transfer_directory(base: &Path) -> io::Result<PathBuf> {
    let base = fs::canonicalize(base)?;
    for number in 1..100_000 {
        let name = if number == 1 {
            "Transferências do Sissy".to_owned()
        } else {
            format!("Transferências do Sissy no. {number}")
        };
        let path = base.join(name);
        match path.symlink_metadata() {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(path),
            Err(e) => return Err(e),
            Ok(_) => {}
        }
    }
    Err(error(
        "Não foi possível reservar um nome para a transferência.",
    ))
}
fn numbered_name(name: &std::ffi::OsStr, dir: bool, number: usize) -> std::ffi::OsString {
    if number == 0 {
        return name.to_os_string();
    }
    let name = Path::new(name);
    let stem = if dir {
        name.as_os_str()
    } else {
        name.file_stem().unwrap_or(name.as_os_str())
    };
    let mut result = stem.to_os_string();
    result.push(format!("_{number}"));
    if !dir {
        if let Some(extension) = name.extension() {
            result.push(".");
            result.push(extension);
        }
    }
    result
}

fn identity(meta: &fs::Metadata) -> (u64, u64) {
    #[cfg(unix)]
    {
        (meta.dev(), meta.ino())
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        (0, 0)
    }
}
fn no_symlinks(path: &Path) -> io::Result<()> {
    for ancestor in path.ancestors() {
        if fs::symlink_metadata(ancestor)?.file_type().is_symlink() {
            return Err(error("Caminho protegido ou alterado"));
        }
    }
    Ok(())
}
fn safe_root(path: &Path) -> io::Result<()> {
    let canonical = fs::canonicalize(path)?;
    let home = crate::user_home();
    if protected(&canonical)
        || home.as_deref() == Some(canonical.as_path())
        || canonical.parent() == Some(Path::new("/Volumes"))
        || canonical
            .components()
            .any(|p| matches!(p.as_os_str().to_str(), Some(".Trash" | ".Trashes")))
    {
        return Err(error("Local protegido para operações."));
    }
    no_symlinks(path)
}
fn normalized_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    #[cfg(target_os = "macos")]
    {
        let pointer = unsafe { sissy_normalized_path(text.as_ptr(), text.len()) };
        if !pointer.is_null() {
            let normalized = unsafe { std::ffi::CStr::from_ptr(pointer) }
                .to_string_lossy()
                .into_owned();
            unsafe {
                sissy_move_error_free(pointer);
            }
            return normalized;
        }
    }
    text.into_owned()
}
fn snapshot(
    root: &Path,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(usize, usize),
) -> io::Result<Vec<Node>> {
    safe_root(root)?;
    let device = identity(&fs::symlink_metadata(root)?).0;
    let mut stack = vec![root.to_path_buf()];
    let mut nodes = Vec::new();
    let (mut files, mut folders) = (0, 0);
    while let Some(path) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            return Err(error("Cancelado"));
        }
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink()
            || protected(&path)
            || (!meta.is_dir() && !meta.is_file())
            || identity(&meta).0 != device
            || path
                .components()
                .any(|p| matches!(p.as_os_str().to_str(), Some(".Trash" | ".Trashes")))
        {
            return Err(error("A pasta contém links, locais protegidos ou outro volume. Escolha os arquivos diretamente."));
        }
        if meta.is_dir() {
            folders += 1;
            for child in fs::read_dir(&path)? {
                stack.push(child?.path());
            }
        } else {
            files += 1;
        }
        nodes.push(Node {
            relative: path.strip_prefix(root).unwrap().into(),
            relative_key: normalized_path(path.strip_prefix(root).unwrap()),
            size: if meta.is_file() { meta.len() } else { 0 },
            modified: meta.modified()?,
            dir: meta.is_dir(),
            identity: identity(&meta),
        });
        progress(folders, files);
    }
    nodes.sort_by(|a, b| a.relative_key.cmp(&b.relative_key));
    Ok(nodes)
}
/// Normalize parent/child selections before collecting the confirmation inventory.
pub fn prepare(
    mut items: Vec<SpaceEntry>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> io::Result<PreparedBatch> {
    items.sort_by(|a, b| a.path.cmp(&b.path));
    items.dedup_by(|a, b| a.path == b.path);
    let mut roots: Vec<SpaceEntry> = Vec::new();
    for item in items {
        if !roots
            .iter()
            .any(|parent| parent.is_dir && item.path.starts_with(&parent.path))
        {
            roots.push(item);
        }
    }
    let mut batch = PreparedBatch::default();
    let total = roots.len();
    let mut names = std::collections::HashSet::new();
    for item in roots {
        let nodes = snapshot(&item.path, cancel, &mut |folders, files| {
            progress(Progress {
                stage: Stage::Preparing,
                total,
                completed: batch.entries.len(),
                folders: batch.folders + folders,
                files: batch.files + files,
            })
        })?;
        let meta = fs::symlink_metadata(&item.path)?;
        let bytes: u64 = nodes.iter().map(|n| n.size).sum();
        if meta.is_dir() != item.is_dir || meta.modified()? != item.modified || bytes != item.size {
            return Err(error(
                "Item alterado; analise novamente antes de continuar.",
            ));
        }
        batch.files += nodes.iter().filter(|n| !n.dir).count();
        batch.folders += nodes.iter().filter(|n| n.dir).count();
        batch.bytes = batch.bytes.saturating_add(bytes);
        let original_name = item
            .path
            .file_name()
            .ok_or_else(|| error("Local protegido para operações."))?;
        let mut number = 0;
        let target_name = loop {
            let name = numbered_name(original_name, item.is_dir, number);
            if names.insert(name.to_string_lossy().to_lowercase()) {
                break name;
            }
            number += 1;
        };
        batch.entries.push(PreparedEntry {
            item,
            target_name,
            nodes,
        });
    }
    Ok(batch)
}
fn validate(entry: &PreparedEntry, cancel: &AtomicBool) -> io::Result<()> {
    if snapshot(&entry.item.path, cancel, &mut |_, _| {})? != entry.nodes {
        return Err(error(
            "Item alterado; analise novamente antes de continuar.",
        ));
    }
    Ok(())
}
/// Destination must be a real directory on a different mounted filesystem.
pub fn validate_destination(batch: &PreparedBatch, directory: &Path) -> io::Result<PathBuf> {
    let parent = directory
        .parent()
        .ok_or_else(|| error("Local protegido para operações."))?;
    let base = fs::canonicalize(parent)?;
    let destination = base.join(
        directory
            .file_name()
            .ok_or_else(|| error("Local protegido para operações."))?,
    );
    no_symlinks(parent)?;
    if !base.starts_with("/Volumes")
        || base == Path::new("/Volumes")
        || protected(&base)
        || !fs::metadata(&base)?.is_dir()
    {
        return Err(error(
            "Escolha uma pasta em outro disco ou partição montado em /Volumes.",
        ));
    }
    if let Ok(meta) = destination.symlink_metadata() {
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(error("Caminho protegido ou alterado"));
        }
    }
    let device = identity(&fs::metadata(&base)?).0;
    for entry in &batch.entries {
        let source = &entry.item.path;
        if device == identity(&fs::metadata(source)?).0 || destination.starts_with(source) {
            return Err(error("O destino precisa estar em outro disco ou partição."));
        }
    }
    // Logical size is conservative for sparse files and APFS clones.
    let output = std::process::Command::new("/bin/df")
        .arg("-Pk")
        .arg(&base)
        .output()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let available = text
        .lines()
        .nth(1)
        .and_then(|line| line.split_whitespace().nth(3))
        .and_then(|n| n.parse::<u64>().ok())
        .ok_or_else(|| error("Não foi possível conferir o espaço livre do destino."))?
        .saturating_mul(1024);
    if available < batch.bytes {
        return Err(error("Espaço livre insuficiente no destino."));
    }
    Ok(destination)
}
/// No action runs without confirmation. Backend injection allows failure/cancellation tests.
pub fn execute_with(
    batch: &PreparedBatch,
    action: &Action,
    confirmed: bool,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
    mut backend: impl FnMut(&Path, &Action) -> io::Result<()>,
) -> Vec<Outcome> {
    if !confirmed {
        return Vec::new();
    }
    let total = batch.entries.len();
    let mut actual_action = action.clone();
    if let Action::Transfer(directory) = action {
        let reservation = (|| {
            validate_destination(batch, directory)?;
            // Reserve the batch directory atomically. A simultaneous creator wins
            // its own name; this batch advances to the next suffix.
            let base = directory.parent().unwrap();
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Err(error("Cancelado"));
                }
                let candidate = next_transfer_directory(base)?;
                match fs::create_dir(&candidate) {
                    Ok(()) => return Ok(candidate),
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(e) => return Err(e),
                }
            }
        })();
        match reservation {
            Ok(destination) => actual_action = Action::Transfer(destination),
            Err(e) => {
                progress(Progress {
                    stage: Stage::Finished,
                    completed: 0,
                    total,
                    files: batch.files,
                    folders: batch.folders,
                });
                return batch
                    .entries
                    .iter()
                    .map(|entry| Outcome {
                        source: entry.item.path.clone(),
                        target: None,
                        result: Err(e.to_string()),
                    })
                    .collect();
            }
        }
    }
    let mut outcomes = Vec::new();
    for (index, entry) in batch.entries.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let emit = |stage| Progress {
            stage,
            completed: index,
            total,
            files: batch.files,
            folders: batch.folders,
        };
        progress(emit(Stage::Validating));
        let mut target = match &actual_action {
            Action::Transfer(dir) => Some(dir.join(&entry.target_name)),
            _ => None,
        };
        let result = (|| {
            validate(entry, cancel)?;
            if let Action::Transfer(destination) = &actual_action {
                let one = PreparedBatch {
                    entries: vec![entry.clone()],
                    bytes: entry.item.size,
                    ..Default::default()
                };
                validate_destination(&one, destination)?;
                let original = entry.item.path.file_name().unwrap();
                let mut number = 0;
                while target.as_ref().unwrap().symlink_metadata().is_ok() {
                    number += 1;
                    target =
                        Some(destination.join(numbered_name(original, entry.item.is_dir, number)));
                }
            }
            if cancel.load(Ordering::Relaxed) {
                return Err(error("Cancelado"));
            }
            progress(emit(match &actual_action {
                Action::Trash => Stage::Trashing,
                Action::Transfer(_) => Stage::Moving,
            }));
            let backend_action = target
                .as_ref()
                .map_or(Action::Trash, |path| Action::Transfer(path.clone()));
            backend(&entry.item.path, &backend_action)?;
            if let Some(target) = &target {
                progress(emit(Stage::Verifying));
                let copied = snapshot(target, &AtomicBool::new(false), &mut |_, _| {})?;
                // The OS move changes inode/device; compare the complete tree structure and file lengths.
                if entry.item.path.try_exists()?
                    || copied.len() != entry.nodes.len()
                    || copied.iter().zip(&entry.nodes).any(|(a, b)| {
                        a.relative_key != b.relative_key || a.dir != b.dir || a.size != b.size
                    })
                {
                    return Err(error("Confira origem e destino: a verificação da transferência encontrou diferenças."));
                }
            }
            Ok(())
        })();
        outcomes.push(Outcome {
            source: entry.item.path.clone(),
            target,
            result: result.map_err(|e: io::Error| e.to_string()),
        });
    }
    if let Action::Transfer(directory) = &actual_action {
        // rmdir only removes this batch's own directory if it is empty. Any
        // partial or successful destination contents are preserved for review.
        let _ = fs::remove_dir(directory);
    }
    progress(Progress {
        stage: Stage::Finished,
        completed: outcomes.len(),
        total,
        files: batch.files,
        folders: batch.folders,
    });
    outcomes
}
pub fn execute(
    batch: &PreparedBatch,
    action: &Action,
    confirmed: bool,
    cancel: &AtomicBool,
    progress: impl FnMut(Progress),
) -> Vec<Outcome> {
    execute_with(batch, action, confirmed, cancel, progress, finder_action)
}
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn sissy_move_item(
        source: *const u8,
        source_len: usize,
        target: *const u8,
        target_len: usize,
    ) -> *mut std::ffi::c_char;
    fn sissy_move_error_free(error: *mut std::ffi::c_char);
    fn sissy_normalized_path(path: *const u8, len: usize) -> *mut std::ffi::c_char;
}
#[cfg(target_os = "macos")]
fn finder_action(source: &Path, action: &Action) -> io::Result<()> {
    match action {
        Action::Trash => crate::trash(source),
        Action::Transfer(target) => {
            let from = source
                .to_str()
                .ok_or_else(|| error("Caminho protegido ou alterado"))?;
            let to = target
                .to_str()
                .ok_or_else(|| error("Caminho protegido ou alterado"))?;
            let failure =
                unsafe { sissy_move_item(from.as_ptr(), from.len(), to.as_ptr(), to.len()) };
            if failure.is_null() {
                return Ok(());
            }
            let message = unsafe { std::ffi::CStr::from_ptr(failure) }
                .to_string_lossy()
                .into_owned();
            unsafe {
                sissy_move_error_free(failure);
            }
            Err(io::Error::other(message))
        }
    }
}
#[cfg(not(target_os = "macos"))]
fn finder_action(_: &Path, _: &Action) -> io::Result<()> {
    Err(error("Lixo disponível apenas no macOS"))
}
