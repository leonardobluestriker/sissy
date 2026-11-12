//! Offline scanner. Discovery never mutates files. Symlinks are always excluded.
pub mod i18n;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::SystemTime,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    Large,
    OldDownload,
    Cache,
    Package,
    Duplicate,
}
#[derive(Clone, Debug)]
pub struct Item {
    pub path: PathBuf,
    pub size: u64,
    pub modified: SystemTime,
    pub reasons: Vec<String>,
    pub tags: Vec<Tag>,
    pub keeper: Option<PathBuf>,
}
#[derive(Clone)]
pub struct Options {
    pub roots: Vec<PathBuf>,
    pub large: u64,
    pub old_days: u64,
    /// Same-size files smaller than this are not hashed.
    pub duplicate_min: u64,
}
#[derive(Clone, Debug)]
pub struct SpaceEntry {
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
}

#[derive(Default)]
pub struct Report {
    pub items: Vec<Item>,
    /// Largest entries across all examined files, independent of candidate thresholds.
    pub top_files: Vec<SpaceEntry>,
    pub top_folders: Vec<SpaceEntry>,
    pub errors: Vec<String>,
    pub visited: usize,
    pub cancelled: bool,
}
pub fn protected(p: &Path) -> bool {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    protected_in(p, home.as_deref())
}

fn protected_in(p: &Path, home: Option<&Path>) -> bool {
    // Callers canonicalize first. Reject ambiguous paths for direct callers too.
    if !p.is_absolute()
        || p.components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return true;
    }
    if let Ok(relative) = p.strip_prefix("/Volumes") {
        let mut parts = relative.components();
        // /Volumes itself must never trigger a scan of every mounted disk.
        let Some(std::path::Component::Normal(volume)) = parts.next() else {
            return true;
        };
        if volume.to_string_lossy().starts_with('.') {
            return true;
        }
        // Protect OS trees, trash, indexes and backup stores on mounted disks.
        return parts.any(|part| {
            matches!(
                part.as_os_str().to_str(),
                Some(
                    "System"
                        | "Library"
                        | "Applications"
                        | "usr"
                        | "bin"
                        | "sbin"
                        | "private"
                        | "dev"
                        | "etc"
                        | "var"
                        | "Volumes"
                        | ".Trashes"
                        | ".Trash"
                        | ".Spotlight-V100"
                        | ".fseventsd"
                        | ".TemporaryItems"
                        | ".DocumentRevisions-V100"
                        | "Backups.backupdb"
                        | ".timemachine"
                )
            )
        });
    }
    home.is_none_or(|h| !p.starts_with(h))
        || ["/System", "/Library", "/usr", "/bin", "/sbin", "/private"]
            .iter()
            .any(|x| p.starts_with(x))
        || home.is_some_and(|h| {
            p.starts_with(h.join("Library")) && !p.starts_with(h.join("Library/Caches"))
        })
}

#[cfg(test)]
mod volume_tests {
    use super::*;

    #[test]
    fn mounted_disk_scope_and_protected_trees() {
        let home = Some(Path::new("/Users/test"));
        for path in [
            "/Volumes/Meu HD",
            "/Volumes/Meu HD/Filmes/video.mov",
            "/Volumes/Meu HD/Users/test/Documents/a",
            "/Users/test/Downloads/a",
        ] {
            assert!(!protected_in(Path::new(path), home), "{path}");
        }
        for path in [
            "/Volumes",
            "/Volumes/HD/System/a",
            "/Volumes/HD/Library/a",
            "/Volumes/HD/.Trashes/501/a",
            "/Volumes/HD/.Spotlight-V100/a",
            "/Volumes/HD/Backups.backupdb/a",
            "/Volumes/HD/Users/test/Library/a",
            "/Volumes/HD/../System/a",
            "/Volumes/.hidden/a",
            "/VolumesExtra/a",
            "/etc/passwd",
            "/Users/other/a",
            "/Users/test/Library/Mail/a",
        ] {
            assert!(protected_in(Path::new(path), home), "{path}");
        }
    }
}

fn digest(p: &Path, cancel: &AtomicBool) -> io::Result<Vec<u8>> {
    let mut f = File::open(p)?;
    let mut hash = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                crate::i18n::text("Cancelado"),
            ));
        }
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(hash.finalize().to_vec())
}
pub fn scan(options: Options, cancel: Arc<AtomicBool>, progress: impl Fn(usize)) -> Report {
    let mut r = Report::default();
    let mut seen = HashSet::new();
    let selected_roots: HashSet<_> = options.roots.iter().cloned().collect();
    let mut stack = options.roots;
    let mut files = Vec::new();
    let mut directories = HashMap::<PathBuf, u64>::new();
    while let Some(p) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            r.cancelled = true;
            break;
        }
        let result = (|| -> io::Result<()> {
            let m = fs::symlink_metadata(&p)?;
            if m.file_type().is_symlink() {
                return Ok(());
            }
            let p = fs::canonicalize(&p)?;
            if protected(&p) {
                if selected_roots.contains(&p) {
                    return Err(io::Error::other(crate::i18n::text(
                        "Local protegido; escolha uma pasta pessoal ou um disco em /Volumes",
                    )));
                }
                return Ok(());
            }
            if !seen.insert(p.clone()) {
                return Ok(());
            }
            if m.is_dir() {
                directories.insert(p.clone(), 0);
                for e in fs::read_dir(p)? {
                    stack.push(e?.path())
                }
            } else if m.is_file() {
                r.visited += 1;
                progress(r.visited);
                let modified = m.modified()?;
                let mut reasons = Vec::new();
                let mut tags = Vec::new();
                let heavy = m.len() >= options.large;
                if heavy {
                    tags.push(Tag::Large);
                    reasons.push(
                        crate::i18n::text("Grande: tamanho acima do limite; pode ser importante.")
                            .into(),
                    )
                }
                let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
                if heavy
                    && p.starts_with(home.join("Downloads"))
                    && modified.elapsed().unwrap_or_default().as_secs() / 86400 >= options.old_days
                {
                    tags.push(Tag::OldDownload);
                    reasons.push(
                        crate::i18n::text(
                            "Download antigo pela modificação; idade não indica inutilidade.",
                        )
                        .into(),
                    )
                }
                if heavy && p.starts_with(home.join("Library/Caches")) {
                    tags.push(Tag::Cache);
                    reasons.push(
                        crate::i18n::text("Cache conhecido; feche o aplicativo associado e revise antes de remover.")
                            .into(),
                    )
                }
                let ext = p
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if heavy
                    && ["mov", "mp4", "mkv", "avi", "dmg", "pkg", "iso", "zip"]
                        .contains(&ext.as_str())
                {
                    tags.push(Tag::Package);
                    reasons.push(crate::i18n::text("Vídeo, instalador ou arquivo compactado acima do limite. A extensão é só uma pista.").into())
                }
                files.push(Item {
                    path: p,
                    size: m.len(),
                    modified,
                    reasons,
                    tags,
                    keeper: None,
                });
            }
            Ok(())
        })();
        if let Err(e) = result {
            r.errors.push(format!("{}: {e}", p.display()))
        }
    }
    // Aggregate once after traversal so overlapping roots never count a file twice.
    for file in &files {
        for parent in file.path.ancestors().skip(1) {
            if let Some(size) = directories.get_mut(parent) {
                *size = size.saturating_add(file.size);
            }
        }
    }
    r.top_files = files
        .iter()
        .map(|f| SpaceEntry {
            path: f.path.clone(),
            size: f.size,
            is_dir: false,
        })
        .collect();
    r.top_folders = directories
        .into_iter()
        .map(|(path, size)| SpaceEntry {
            path,
            size,
            is_dir: true,
        })
        .collect();
    for entries in [&mut r.top_files, &mut r.top_folders] {
        entries.sort_by(|a, b| b.size.cmp(&a.size).then(a.path.cmp(&b.path)));
        entries.truncate(10);
    }
    // Hash only size collisions. Every matching file stays choosable.
    let mut sizes: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, f) in files.iter().enumerate() {
        sizes.entry(f.size).or_default().push(i)
    }
    for indices in sizes
        .iter()
        .filter(|(size, v)| **size >= options.duplicate_min && v.len() > 1)
        .map(|(_, v)| v)
    {
        let mut groups: HashMap<Vec<u8>, Vec<usize>> = HashMap::new();
        for &i in indices {
            if cancel.load(Ordering::Relaxed) {
                r.cancelled = true;
                break;
            }
            match digest(&files[i].path, &cancel) {
                Ok(h) => groups.entry(h).or_default().push(i),
                Err(e) => r.errors.push(format!("{}: {e}", files[i].path.display())),
            }
        }
        for members in groups.into_values() {
            if members.len() < 2 {
                continue;
            }
            let anchor = files[members[0]].path.clone();
            let places = members.len();
            for &i in &members {
                if !files[i].tags.contains(&Tag::Duplicate) {
                    files[i].tags.push(Tag::Duplicate);
                }
                files[i].keeper = Some(anchor.clone());
                files[i].reasons.push(crate::tf!(
                    "O mesmo conteúdo está em {places} lugares. Escolha quais cópias vão ao Lixo.",
                    places = places
                ));
            }
        }
    }
    r.cancelled |= cancel.load(Ordering::Relaxed);
    r.items = files
        .into_iter()
        .filter(|f| !f.reasons.is_empty())
        .collect();
    r.items
        .sort_by(|a, b| b.size.cmp(&a.size).then(a.path.cmp(&b.path)));
    r
}
/// Revalidate immediately before an action; reject path substitution and changed content.
pub fn validate(item: &Item) -> io::Result<()> {
    let p = fs::canonicalize(&item.path)?;
    if p != item.path || protected(&p) || fs::symlink_metadata(&item.path)?.file_type().is_symlink()
    {
        return Err(io::Error::other(crate::i18n::text(
            "Caminho protegido ou alterado",
        )));
    }
    let m = fs::metadata(&p)?;
    if !m.is_file() || m.len() != item.size || m.modified()? != item.modified {
        return Err(io::Error::other(crate::i18n::text(
            "Arquivo alterado; analise novamente",
        )));
    }
    Ok(())
}
pub fn act(
    items: &[Item],
    confirmed: bool,
    mut action: impl FnMut(&Path) -> io::Result<()>,
) -> Vec<(PathBuf, Result<(), String>)> {
    if !confirmed {
        return Vec::new();
    }
    let checks: Vec<Result<(), String>> = items
        .iter()
        .map(|item| validate(item).map_err(|e| e.to_string()))
        .collect();
    items
        .iter()
        .zip(checks)
        .map(|(item, check)| {
            let result = check.and_then(|_| action(&item.path).map_err(|e| e.to_string()));
            (item.path.clone(), result)
        })
        .collect()
}
#[cfg(target_os = "macos")]
pub fn trash(p: &Path) -> io::Result<()> {
    let script="on run argv\ntell application \"Finder\" to delete (POSIX file (item 1 of argv) as alias)\nend run";
    let s = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", script, "--"])
        .arg(p)
        .status()?;
    if s.success() {
        Ok(())
    } else {
        Err(io::Error::other(crate::i18n::text(
            "Finder recusou a operação; verifique permissões",
        )))
    }
}
#[cfg(not(target_os = "macos"))]
pub fn trash(_: &Path) -> io::Result<()> {
    Err(io::Error::other(crate::i18n::text(
        "Lixo disponível apenas no macOS",
    )))
}

/// Shorten a file name without hiding its type: `ferias-2024... .mp4`.
pub fn shrink_filename(name: &str, max_chars: usize) -> String {
    if name.chars().count() <= max_chars {
        return name.to_string();
    }
    let ext = filename_suffix(name);
    if ext.is_empty() {
        return cut_end(name, max_chars.max(4));
    }
    let suffix = format!("... .{ext}");
    if suffix.chars().count() >= max_chars {
        return suffix;
    }
    let stem_len = name.chars().count() - ext.chars().count() - 1;
    let keep = max_chars - suffix.chars().count();
    let stem: String = name.chars().take(stem_len.min(keep)).collect();
    format!("{stem}{suffix}")
}

fn filename_suffix(name: &str) -> String {
    const COMPOUND: &[&str] = &["tar.gz", "tar.bz2", "tar.xz", "tar.zst"];
    let lower = name.to_ascii_lowercase();
    for compound in COMPOUND {
        let with_dot = format!(".{compound}");
        if lower.ends_with(&with_dot) && name.len() > with_dot.len() {
            return name[name.len() - compound.len()..].to_string();
        }
    }
    match name.rfind('.') {
        Some(i) if i > 0 => {
            let ext = &name[i + 1..];
            let n = ext.chars().count();
            if (1..=8).contains(&n) && !ext.contains(' ') && !ext.contains('/') {
                ext.to_string()
            } else {
                String::new()
            }
        }
        _ => String::new(),
    }
}

fn cut_end(name: &str, max_chars: usize) -> String {
    if name.chars().count() <= max_chars {
        return name.to_string();
    }
    let keep = max_chars.saturating_sub(3).max(1);
    let stem: String = name.chars().take(keep).collect();
    format!("{stem}...")
}
