use anyhow::anyhow;
use anyhow::{Context, Result};
use git2::{Repository, Status, StatusOptions};

use std::env;
use std::path::{Path, PathBuf};

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

// Use the high-performance matcher crate
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config as MatcherConfig, Matcher};

fn collect_unstaged_and_untracked(repo_path: &std::path::Path) -> Result<Vec<FileEntry>> {
    let repo = Repository::open(repo_path)
        .with_context(|| format!("opening git repository at {}", repo_path.display()))?;

    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .include_ignored(false)
        .renames_head_to_index(true)
        .renames_from_rewrites(true)
        .include_unmodified(false)
        .exclude_submodules(true);

    let statuses = repo
        .statuses(Some(&mut opts))
        .with_context(|| format!("collecting git statuses for {}", repo_path.display()))?;

    let mut entries = Vec::new();

    for entry in statuses.iter() {
        let s = entry.status();

        if s.intersects(
            Status::WT_NEW
                | Status::WT_MODIFIED
                | Status::WT_DELETED
                | Status::WT_TYPECHANGE
                | Status::WT_RENAMED,
        ) {
            if let Some(p) = entry.path() {
                entries.push(FileEntry {
                    path: PathBuf::from(p),
                    mode: FileMode::Regular,
                });
            }
        }
    }

    Ok(entries)
}

fn main() -> Result<()> {
    let prog = env::args().next().unwrap_or_else(|| "git-fad".into());
    // Collect all remaining CLI args as independent fuzzy tokens
    let tokens: Vec<String> = env::args().skip(1).collect();
    if tokens.is_empty() {
        eprintln!("Usage: {} <query tokens...>", prog);
        eprintln!("Examples:");
        eprintln!("  {} cargo", prog);
        eprintln!("  {} packages book type spec", prog);
        eprintln!("  {} src main rs", prog);
        return Ok(());
    }

    let repo_path = std::env::current_dir()?;

    // 1) collect candidates: only unstaged and untracked files
    let candidates = collect_unstaged_and_untracked(&repo_path)?;

    if candidates.is_empty() {
        println!(
            "No unstaged or untracked files found in repository {}",
            repo_path.display()
        );
        return Ok(());
    }

    // 2) Prepare haystacks: we match on file path strings
    // Keep owned Strings so references remain valid during matching
    let mut hay: Vec<String> = Vec::with_capacity(candidates.len());
    for c in &candidates {
        hay.push(c.path.to_string_lossy().into_owned());
    }
    let hay_refs: Vec<&str> = hay.iter().map(|s| s.as_str()).collect();

    // 3) Create a matcher with path-friendly config
    let mut matcher: Matcher = Matcher::new(MatcherConfig::DEFAULT.match_paths());

    // 4) Multi-token fuzzy matching:
    // We treat each CLI token as a required fuzzy pattern. A candidate must match ALL tokens.
    // We sum (aggregate) the individual token scores, and finally break ties by preferring
    // shorter paths (heuristic for "more specific").
    //
    // Algorithm:
    //   cumulative = empty map
    //   for each token:
    //       run fuzzy over full haystack -> map_this
    //       if first token: cumulative = map_this
    //       else: cumulative = intersection(cumulative, map_this) with scores added
    //   pick max score; tie -> shorter path; next tie -> lexical
    use std::collections::HashMap;

    let mut cumulative: HashMap<&str, u32> = HashMap::new();
    let mut first = true;

    for tok in &tokens {
        let pattern = Pattern::parse(tok, CaseMatching::Ignore, Normalization::Smart);
        let token_matches = pattern.match_list(&hay_refs, &mut matcher);

        if token_matches.is_empty() {
            // Early exit: one token matched nothing => overall no result
            println!("No matches (token '{}' matched nothing)", tok);
            return Ok(());
        }

        if first {
            for (p, score) in token_matches {
                cumulative.insert(p, score);
            }
            first = false;
        } else {
            // Build lookup for this token
            let mut this_map: HashMap<&str, u32> = HashMap::with_capacity(token_matches.len());
            for (p, score) in token_matches {
                this_map.insert(p, score);
            }
            // Retain only candidates also matched by this token; add their score
            cumulative.retain(|p, total_score| {
                if let Some(s) = this_map.get(p) {
                    *total_score += *s;
                    true
                } else {
                    false
                }
            });
            if cumulative.is_empty() {
                println!("No matches after applying tokens: {}", tokens.join(" "));
                return Ok(());
            }
        }
    }

    if cumulative.is_empty() {
        println!("No matches for query tokens: {}", tokens.join(" "));
        return Ok(());
    }

    // Select best (score desc, then shorter path, then lexical)
    let (best_path_str, best_score) = cumulative
        .into_iter()
        .max_by(|(pa, sa), (pb, sb)| {
            // Order by:
            // 1. Higher aggregate score
            // 2. Shorter path
            // 3. Lexicographical order
            sa.cmp(sb)
                .then_with(|| pb.len().cmp(&pa.len())) // shorter path wins
                .then_with(|| pa.cmp(pb))
        })
        .expect("non-empty cumulative map just ensured");

    println!(
        "Best match: {} (aggregate_score={}, tokens={})",
        best_path_str,
        best_score,
        tokens.join("+")
    );

    // 7) Convert the matched string back to a repository-relative PathBuf and stage it
    // Build a small lookup map from hay path -> index so we can reliably find the matched index
    // without running into reference-level comparison issues.
    let mut index_map: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::with_capacity(hay.len());
    for (i, s) in hay.iter().enumerate() {
        // store &str from the owned `hay` Strings so the references remain valid
        index_map.insert(s.as_str(), i);
    }
    let top_index = *index_map
        .get(best_path_str)
        .expect("matched path must exist in haystack");

    let top_entry = &candidates[top_index];

    // Stage via our git module using libgit2 (this will add the path to the index)
    stage_paths_libgit2(&repo_path, &[top_entry.path.clone()]).with_context(|| {
        format!(
            "staging {} in repo {}",
            top_entry.path.display(),
            repo_path.display()
        )
    })?;

    println!("Staged {}", top_entry.path.display());

    Ok(())
}
