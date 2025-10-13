use anyhow::anyhow;
use anyhow::{Context, Result};
use git2::{Repository, Status, StatusOptions};
use globset::{Glob, GlobMatcher};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config as MatcherConfig, Matcher};
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
}

fn token_to_index_scores<'a>(
    token: &str,
    hay_refs: &[&'a str],
    index_map: &HashMap<&'a str, usize>,
    matcher: &mut Matcher,
) -> HashMap<usize, u32> {
    let is_glob = token.contains('*');

    if is_glob {
        match Glob::new(token) {
            Ok(glob) => {
                let gm: GlobMatcher = glob.compile_matcher();
                hay_refs
                    .iter()
                    .filter_map(|p| {
                        if gm.is_match(p) {
                            index_map.get(p).map(|i| (*i, 1u32))
                        } else {
                            None
                        }
                    })
                    .collect()
            }
            Err(_) => HashMap::new(),
        }
    } else {
        let pattern = Pattern::parse(token, CaseMatching::Ignore, Normalization::Smart);
        let token_matches = pattern.match_list(hay_refs, matcher);
        token_matches
            .into_iter()
            .filter_map(|(p, score)| index_map.get(p).map(|i| (*i, score)))
            .collect()
    }
}

fn collect_unstaged_and_untracked(repo_path: &Path) -> Result<Vec<FileEntry>> {
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

    let entries = statuses
        .iter()
        .filter_map(|entry| {
            let s = entry.status();
            // consider WT_* changes only
            if !s.intersects(
                Status::WT_NEW
                    | Status::WT_MODIFIED
                    | Status::WT_DELETED
                    | Status::WT_TYPECHANGE
                    | Status::WT_RENAMED,
            ) {
                return None;
            }
            entry.path().map(|p| FileEntry {
                path: PathBuf::from(p),
            })
        })
        .collect();

    Ok(entries)
}

fn stage_paths_libgit2(repo_path: &Path, paths: &[PathBuf]) -> Result<()> {
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

fn main() -> Result<()> {
    let prog = env::args().next().unwrap_or_else(|| "git-fad".into());
    let tokens: Vec<String> = env::args().skip(1).collect();
    if tokens.is_empty() {
        eprintln!("Usage: {} <query tokens...>", prog);
        return Ok(());
    }

    let repo_path = std::env::current_dir()?;
    let candidates = collect_unstaged_and_untracked(&repo_path)?;

    if candidates.is_empty() {
        println!(
            "No unstaged or untracked files found in repository {}",
            repo_path.display()
        );
        return Ok(());
    }

    // Prepare haystacks: owned strings backed by candidates
    let hay: Vec<String> = candidates
        .iter()
        .map(|c| c.path.to_string_lossy().into_owned())
        .collect();
    let hay_refs: Vec<&str> = hay.iter().map(|s| s.as_str()).collect();

    let index_map: HashMap<&str, usize> = hay
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();

    let mut matcher: Matcher = Matcher::new(MatcherConfig::DEFAULT.match_paths());

    let mut cumulative: HashMap<usize, u32> = HashMap::new();
    let mut first = true;

    for tok in &tokens {
        let tok_map = token_to_index_scores(tok, &hay_refs, &index_map, &mut matcher);

        if tok_map.is_empty() {
            // Early exit: one token matched nothing => overall no result
            println!("No matches (token '{}' matched nothing)", tok);
            return Ok(());
        }

        if first {
            cumulative = tok_map;
            first = false;
        } else {
            // intersect: keep only indices present in both maps, summing their scores
            cumulative = cumulative
                .into_iter()
                .filter_map(|(idx, total)| tok_map.get(&idx).map(|s| (idx, total + s)))
                .collect();
            if cumulative.is_empty() {
                println!("No matches after applying tokens: {}", tokens.join(" "));
                return Ok(());
            }
        }
    }

    let (best_idx, best_score) = cumulative
        .into_iter()
        .max_by(|(a_idx, a_score), (b_idx, b_score)| {
            a_score
                .cmp(b_score)
                .then_with(|| hay[*a_idx].len().cmp(&hay[*b_idx].len()).reverse()) // shorter path wins
                .then_with(|| hay[*a_idx].cmp(&hay[*b_idx]))
        })
        .expect("non-empty cumulative ensured");

    println!(
        "Best match: {} (aggregate_score={}, tokens={})",
        hay[best_idx],
        best_score,
        tokens.join("+")
    );

    let top_entry = &candidates[best_idx];
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
