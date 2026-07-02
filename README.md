# git-fad

Fuzzy `git add`. Type a few characters, stage the file you meant:

```bash
$ git fad mcon
Best match: src/models/config.rs (aggregate_score=156, tokens=mcon)
Staged src/models/config.rs
```

Built in Rust on [nucleo-matcher](https://github.com/helix-editor/nucleo) — the same matching engine as the Helix editor — with `git2` for repository state, so nothing shells out to `git status`.

I wrote about the design decisions (scoring, tiebreaking, why multi-token means AND) in [Fuzzy Git Staging in Rust](https://smccl.ca/writing/fuzzy-matching-rust).

## Usage

```bash
git fad <query tokens...>
```

Each token narrows the result — tokens intersect rather than expand, so `mod conf` means "fuzzy-matches `mod` AND `conf`". Tokens containing `*` are treated as globs and mix freely with fuzzy tokens:

```bash
git fad main          # fuzzy: stages src/main.rs
git fad "*.rs" conf   # glob + fuzzy: Rust files that also match "conf"
```

Only files that can actually be staged are considered: modified, deleted, renamed, and type-changed files in the working tree, plus untracked files. The single best match is staged; ties break toward shorter paths, then lexicographically, so the same query always stages the same file.

## Install

```bash
git clone https://github.com/sammcclenaghan/git-fad.git
cd git-fad
cargo build --release
cp target/release/git-fad ~/.local/bin/
```

Any binary named `git-fad` on your `PATH` works as the `git fad` subcommand automatically.

## License

MIT — see [LICENSE](LICENSE).
