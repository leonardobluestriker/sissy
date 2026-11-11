//! Offline scanner. Discovery never mutates files. Symlinks are always excluded.
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
#[derive(Clone, Debug)]
pub struct Item {
    pub path: PathBuf,
    pub size: u64,
    pub modified: SystemTime,
    pub reasons: Vec<String>,
    pub keeper: Option<PathBuf>,
}
#[derive(Clone)]
pub struct Options {
    pub roots: Vec<PathBuf>,
    pub large: u64,
    pub old_days: u64,
}
#[derive(Default)]
pub struct Report {
    pub items: Vec<Item>,
    pub errors: Vec<String>,
    pub visited: usize,
    pub cancelled: bool,
}
pub fn protected(p: &Path) -> bool {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    // Only user-owned trees are actionable. Other roots are never scanned.
    home.as_ref().is_none_or(|h| !p.starts_with(&h))
        || ["/System", "/Library", "/usr", "/bin", "/sbin", "/private"]
            .iter()
            .any(|x| p.starts_with(x))
        || home.as_ref().is_some_and(|h| {
            p.starts_with(h.join("Library")) && !p.starts_with(h.join("Library/Caches"))
        })
}
fn digest(p: &Path, cancel: &AtomicBool) -> io::Result<Vec<u8>> {
    let mut f = File::open(p)?;
    let mut hash = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "Cancelado"));
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
    let mut stack = options.roots;
    let mut files = Vec::new();
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
            if protected(&p) || !seen.insert(p.clone()) {
                return Ok(());
            }
            if m.is_dir() {
                for e in fs::read_dir(p)? {
                    stack.push(e?.path())
                }
            } else if m.is_file() {
                r.visited += 1;
                progress(r.visited);
                let modified = m.modified()?;
                let mut reasons = Vec::new();
                if m.len() >= options.large {
                    reasons.push("Grande: tamanho acima do limite; pode ser importante.".into())
                }
                let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
                if p.starts_with(home.join("Downloads"))
                    && modified.elapsed().unwrap_or_default().as_secs() / 86400 >= options.old_days
                {
                    reasons.push(
                        "Download antigo pela modificação; idade não indica inutilidade.".into(),
                    )
                }
                if p.starts_with(home.join("Library/Caches")) {
                    reasons.push(
                        "Cache conhecido; feche o aplicativo associado e revise antes de remover."
                            .into(),
                    )
                }
                let ext = p
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if ["mov", "mp4", "mkv", "avi", "dmg", "pkg", "iso", "zip"].contains(&ext.as_str())
                {
                    reasons.push("Vídeo, instalador ou arquivo compactado: revisar utilidade; extensão é apenas uma pista.".into())
                }
                files.push(Item {
                    path: p,
                    size: m.len(),
                    modified,
                    reasons,
                    keeper: None,
                });
            }
            Ok(())
        })();
        if let Err(e) = result {
            r.errors.push(format!("{}: {e}", p.display()))
        }
    }
    // Hash only size collisions. Never designate the retained copy for removal.
    let mut sizes: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, f) in files.iter().enumerate() {
        sizes.entry(f.size).or_default().push(i)
    }
    for indices in sizes.values().filter(|v| v.len() > 1) {
        let mut hashes: HashMap<Vec<u8>, usize> = HashMap::new();
        for &i in indices {
            if cancel.load(Ordering::Relaxed) {
                r.cancelled = true;
                break;
            }
            match digest(&files[i].path, &cancel) {
                Ok(h) => {
                    if let Some(&j) = hashes.get(&h) {
                        files[i].keeper = Some(files[j].path.clone());
                        files[i].reasons.push("Conteúdo SHA-256 idêntico; esta cópia é candidata, a cópia indicada será preservada. Revise vínculos e uso.".into())
                    } else {
                        hashes.insert(h, i);
                    }
                }
                Err(e) => r.errors.push(format!("{}: {e}", files[i].path.display())),
            }
        }
    }
    r.cancelled |= cancel.load(Ordering::Relaxed);
    r.items = files
        .into_iter()
        .filter(|f| !f.reasons.is_empty())
        .collect();
    r
}
/// Revalidate immediately before an action; reject path substitution and changed content.
pub fn validate(item: &Item) -> io::Result<()> {
    let p = fs::canonicalize(&item.path)?;
    if p != item.path || protected(&p) || fs::symlink_metadata(&item.path)?.file_type().is_symlink()
    {
        return Err(io::Error::other("Caminho protegido ou alterado"));
    }
    let m = fs::metadata(&p)?;
    if !m.is_file() || m.len() != item.size || m.modified()? != item.modified {
        return Err(io::Error::other("Arquivo alterado; analise novamente"));
    }
    if let Some(k) = &item.keeper {
        let c = AtomicBool::new(false);
        if k == &p || digest(k, &c)? != digest(&p, &c)? {
            return Err(io::Error::other("Cópia preservada ausente ou diferente"));
        }
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
    items
        .iter()
        .map(|i| {
            let result = if items
                .iter()
                .any(|other| i.keeper.as_ref() == Some(&other.path))
            {
                Err(io::Error::other(
                    "A cópia preservada também foi selecionada",
                ))
            } else {
                validate(i).and_then(|_| action(&i.path))
            };
            (i.path.clone(), result.map_err(|e| e.to_string()))
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
        Err(io::Error::other(
            "Finder recusou a operação; verifique permissões",
        ))
    }
}
#[cfg(not(target_os = "macos"))]
pub fn trash(_: &Path) -> io::Result<()> {
    Err(io::Error::other("Lixo disponível apenas no macOS"))
}
