# git-fad

A Git extension for fuzzy file staging. `git-fad` lets you stage files using fuzzy matching, making it quick and easy to add files without typing their full paths.

## Features

- **Fuzzy matching**: Stage files by typing partial names or patterns
- **Fast**: Built with Rust and the high-performance nucleo matcher
- **Git integration**: Works seamlessly with your existing Git workflow
- **Smart filtering**: Only shows unstaged and untracked files that can actually be staged

## Installation

### From Source

1. Make sure you have Rust installed (https://rustup.rs/)
2. Clone this repository:
   ```bash
   git clone https://github.com/your-username/git-fad.git
   cd git-fad
   ```
3. Build and install:
   ```bash
   cargo build --release
   # Copy to a directory in your PATH
   cp target/release/git-fad ~/.local/bin/
   # Or create a symlink
   ln -s $(pwd)/target/release/git-fad ~/.local/bin/git-fad
   ```

### Using as a Git Subcommand

To use `git fad` instead of `git-fad`, make sure the binary is named `git-fad` and is in your PATH. Git will automatically recognize it as a subcommand.

## Usage

```bash
# Stage a file using fuzzy matching
git-fad "partial-name"

# Or as a git subcommand (if properly installed)
git fad "partial-name"

# Examples:
git fad "cargo"        # Might match Cargo.toml or Cargo.lock
git fad "main"         # Might match src/main.rs
git fad "test"         # Might match test_file.txt
git fad "readme"       # Might match README.md
```

## How It Works

1. **Discovers candidates**: Finds all files that have unstaged changes or are untracked
2. **Fuzzy matching**: Uses the nucleo matcher (same engine as Helix editor) to find the best match for your query
3. **Stages the file**: Automatically runs `git add` on the best matching file
4. **Shows results**: Displays the matched file and its score

The tool only considers files that can actually be staged:
- Untracked files (`git status` shows `??`)
- Modified files in working tree (`git status` shows ` M`)
- Deleted files in working tree (`git status` shows ` D`)
- Renamed files in working tree
- Type-changed files in working tree

## Examples

```bash
# Your repository has these unstaged files:
# - src/main.rs (modified)
# - tests/integration_test.rs (untracked)
# - README.md (modified)
# - Cargo.toml (modified)

$ git-fad "main"
Best match: src/main.rs (score=156)
Staged src/main.rs

$ git-fad "integ"
Best match: tests/integration_test.rs (score=142)
Staged tests/integration_test.rs

$ git-fad "cargo"
Best match: Cargo.toml (score=134)
Staged Cargo.toml

# No matches
$ git-fad "nonexistent"
No matches for query: nonexistent

# No unstaged files
$ git-fad "anything"
No unstaged or untracked files found in repository /path/to/repo
```

## Dependencies

- `git2` - Git operations
- `nucleo-matcher` - High-performance fuzzy matching
- `anyhow` - Error handling

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

MIT License - see LICENSE file for details
