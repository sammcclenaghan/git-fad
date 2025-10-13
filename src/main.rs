use anyhow::{Context, Result};
use git2::{Repository, Status, StatusOptions};

use std::env;
use std::path::PathBuf;

// Use the lib we added earlier which lists index entries and stages via libgit2.
mod git;

use git::{FileEntry, FileMode, stage_paths_libgit2};

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

        // Only include files that can be staged:
        // - WT_NEW: untracked files
        // - WT_MODIFIED: modified files in working tree
        // - WT_DELETED: deleted files in working tree
        // - WT_TYPECHANGE: type changed files in working tree
        // - WT_RENAMED: renamed files in working tree
        if s.intersects(
            Status::WT_NEW
                | Status::WT_MODIFIED
                | Status::WT_DELETED
                | Status::WT_TYPECHANGE
                | Status::WT_RENAMED,
        ) {
            if let Some(p) = entry.path() {
                // Treat all as Regular files for simplicity - git2 will handle the actual staging correctly
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
    let mut args = env::args().skip(1);
    let prog = env::args().next().unwrap_or_else(|| "git-fad".into());

    // Expect a single query argument (it may contain spaces if quoted)
    let query = match args.next() {
        Some(q) => q,
        None => {
            eprintln!("Usage: {} <query>", prog);
            eprintln!("Example: {} Cargo", prog);
            return Ok(());
        }
    };

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

    // 4) Parse the query into a Pattern (word segmentation etc.)
    let pattern = Pattern::parse(&query, CaseMatching::Ignore, Normalization::Smart);

    // 5) Run matches over the haystack using the matcher
    // The `match_list` convenience returns a Vec<(&str, score)> for matches
    let matches = pattern.match_list(&hay_refs, &mut matcher);

    if matches.is_empty() {
        println!("No matches for query: {}", query);
        return Ok(());
    }

    // 6) The first element is the best match (Pattern::match_list returns in descending score)
    let &(best_path_str, best_score) = &matches[0];
    println!("Best match: {} (score={})", best_path_str, best_score);

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
