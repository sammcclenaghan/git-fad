# git-fad

Fuzzy staging for git. Type a few characters of a changed file's name and git-fad stages the file you meant — powered by [nucleo](https://github.com/helix-editor/nucleo), the same matcher as Helix.

## Install

```bash
cargo install --git https://github.com/sammcclenaghan/git-fad
```

With the binary on your PATH, git picks it up as a subcommand.

## Usage

```
$ git fad main
Best match: src/main.rs (aggregate_score=156, tokens=main)
Staged src/main.rs
```

Multiple tokens narrow the match — `git fad mod conf` finds `src/models/config.rs`.

Only files that can actually be staged are considered: modified, deleted, renamed, and untracked.
