# git-watch

A macOS-only terminal UI that continuously monitors multiple local Git repositories with very low idle resource usage.

## Features

- **Event-driven**: Uses macOS FSEvents (not polling) as the primary trigger
- **Compact UI**: Two lines per repo, designed for ~30 character wide terminal panels
- **Color-enhanced**: Colored status symbols with monochrome fallback
- **Low resource usage**: ~741KB binary, near-zero idle CPU, minimal memory footprint
- **Per-repo debounce**: Collapses rapid filesystem events into single recomputation
- **Periodic reconciliation**: Safety scan every 60s to catch missed events

## UI

```
●· api-server
  refactor auth middleware  5m

·● web-app
  add billing settings...   8s

●● cli-tool
  initial fs watcher impl   1m

·· docs
  update onboarding         2h

✖✖ old-repo
  repository unavailable    --
```

| Symbol | Meaning |
|--------|---------|
| `●·` | Staged changes only |
| `·●` | Unstaged changes only |
| `●●` | Both staged and unstaged |
| `··` | Clean |
| `✖✖` | Error / unavailable |

## Install

```bash
# Clone and build
git clone <repo-url>
cd git-watch
cargo build --release

# Copy binary to PATH
cp target/release/git-watch /usr/local/bin/
```

Requires: Rust toolchain, macOS, Git.

## Usage

```bash
# Create a config file
cp config.sample.yml config.yml
# Edit repos list to point to your repositories
vim config.yml

# Run
git-watch --config config.yml
```

### Keyboard

- `q` — quit
- `Ctrl-C` — quit
- `r` — force refresh all repos

## Configuration

```yaml
repos:
  - ~/work/api-server
  - ~/work/web-app
  - ~/work/cli-tool
  - ~/work/docs

watch:
  debounce_ms: 200              # Per-repo debounce window
  reconcile_interval_sec: 60    # Periodic full refresh interval

git:
  command_timeout_sec: 3        # Timeout for git subprocess calls
  max_concurrent_checks: 4      # (reserved for future use)

ui:
  width: 30                     # Target terminal width
  color: true                   # Enable color output
  show_clean: true              # Show clean repos
  blank_line_between_repos: true
```

All config sections except `repos` are optional and have sensible defaults.

## Architecture

- **config.rs** — YAML config parsing with tilde expansion
- **model.rs** — Core data types (RepoState, StatusKind, Message)
- **git.rs** — Git subprocess execution, status parsing, age formatting
- **watcher.rs** — FSEvents watcher via `notify` crate, per-repo debounce, reconciliation timer
- **ui.rs** — Terminal rendering with `crossterm`, two-line layout, color styling
- **main.rs** — Orchestration loop, CLI args, signal handling

## How it works

1. On startup: loads config, validates repos, computes initial state via git commands, renders UI
2. FSEvents watcher monitors all repo directories recursively
3. Filesystem events are debounced per-repo (200ms default) and trigger git recomputation
4. Git commands (`git diff --quiet`, `git diff --cached --quiet`, `git log`) determine truth
5. A reconciliation timer fires every 60s as a safety net for missed events

## Known Limitations

- **macOS only** — uses FSEvents via the `notify` crate's `recommended_watcher()`
- **No interactive selection** — read-only display, no repo selection or actions
- **No nested repo support** — repos should not be subdirectories of each other
- **Git subprocess overhead** — runs 3-4 git commands per repo per refresh (~5-20ms each)
- **No git timeout enforcement** — the `command_timeout_sec` config is reserved but not yet enforced
- **No sorting** — repos are displayed in config file order
- **Terminal resize** — not dynamically handled; restart to pick up new dimensions
