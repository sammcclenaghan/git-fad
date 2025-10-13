use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use git2::{IndexEntry, Repository};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileMode {
    Regular,
    Executable,
    Symlink,
    Submodule,
    Other(u32),
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub mode: FileMode,
}

pub fn list_files_libgit2(repo_path: &Path) -> Result<Vec<FileEntry>> {
    let repo = Repository::open(repo_path)
        .with_context(|| format!("opening git repository at {}", repo_path.display()))?;
    let index = repo
        .index()
        .with_context(|| format!("reading index for repo {}", repo_path.display()))?;

    let mut out = Vec::with_capacity(index.len() as usize);

    for i in 0..index.len() {
        if let Some(entry) = index.get(i) {
            let path = path_from_index_entry(&entry)?;
            let mode = mode_from_index_entry(&entry);
            out.push(FileEntry { path, mode });
        }
    }

    Ok(out)
}

fn path_from_index_entry(entry: &IndexEntry) -> Result<PathBuf> {
    // IndexEntry stores a path as a byte vector internally. Try to decode as UTF-8
    // directly first; if that fails, fall back to a lossy conversion so we can
    // handle non-UTF8 paths gracefully.
    let s = match String::from_utf8(entry.path.clone()) {
        Ok(s) => s,
        Err(_) => String::from_utf8_lossy(&entry.path).into_owned(),
    };
    Ok(PathBuf::from(s))
}

fn mode_from_index_entry(entry: &IndexEntry) -> FileMode {
    match entry.mode {
        0o100755 => FileMode::Executable,
        0o100644 => FileMode::Regular,
        0o120000 => FileMode::Symlink,
        0o160000 => FileMode::Submodule,
        other => FileMode::Other(other),
    }
}

pub fn stage_paths_libgit2(repo_path: &Path, paths: &[PathBuf]) -> Result<()> {
    let repo = Repository::open(repo_path)
        .with_context(|| format!("opening git repository at {}", repo_path.display()))?;
    let mut index = repo
        .index()
        .with_context(|| format!("reading index for repo {}", repo_path.display()))?;

    for p in paths {
        let rel = if p.is_absolute() {
            p.strip_prefix(repo_path).map(PathBuf::from).map_err(|_| {
                anyhow!(
                    "path {} is not inside repository {}",
                    p.display(),
                    repo_path.display()
                )
            })?
        } else {
            p.clone()
        };

        index
            .add_path(&rel)
            .with_context(|| format!("adding {} to index", rel.display()))?;
    }

    index.write().context("writing index after staging paths")?;

    Ok(())
}
