# claude-self

> Replace ad-hoc bash drafts of `claude-self` with a Rust CLI that maintains a stable `CLAUDE_SELF.md` (voice/values/defaults) at `~/.claude/CLAUDE_SELF.md`.

## Why

Replace ad-hoc bash drafts of `claude-self` with a Rust CLI that maintains a stable `CLAUDE_SELF.md` (voice/values/defaults) at `~/.claude/CLAUDE_SELF.md`. The lint contract (canonical sections, length cap, non-empty Aspirations, no duplicate bullets) is the load-bearing piece — without it the file rots into a bland, drift-prone catch-all and the per-session 'self model' reassembly remains lossy.

## Build

```sh
cargo build --release
```

Produces `target/release/claude-self`. Symlink into `~/.local/bin/` if you want it on `$PATH`.

## Usage

```sh
claude-self --help
```

## Audience

Single developer (Joe) co-editing a 7-section markdown file with Claude on one laptop. Runs `claude-self lint` to validate after edits, `claude-self show` to inspect, `claude-self diff`/`log` to track history, `claude-self default-restore` to recover.

## Acceptance criteria

This project was scaffolded from a PRD via the `autobuilder` pipeline. The MUST-level acceptance criteria are:

- **AC1**: `claude-self path` prints the resolved live-file path (default `$HOME/.claude/CLAUDE_SELF.md`, overridable via `CLAUDE_SELF_FILE`) to stdout and exits 0.
- **AC2**: `claude-self show` writes the live file's contents to stdout when present (exit 0); exits 2 with diagnostic on stderr when missing.
- **AC3**: `claude-self lint` enforces the four lint rules: (a) exactly the seven canonical section headers in order [Voice, Values, Defaults, Things I keep getting wrong, Aspirations, Boundaries, Changelog], (b) total length <= 200 lines, (c) Aspi...
- **AC4**: `claude-self lint --quiet` suppresses all stdout/stderr output and preserves the same exit code as non-quiet mode.
- **AC5**: `claude-self default-restore --yes` overwrites the live file from `$HOME/.claude/CLAUDE_SELF.default.md` (overridable via `CLAUDE_SELF_DEFAULT`), runs lint on the result, and exits 0 when the default file lints clean. Without `--yes` exi...
- **AC6**: `claude-self help` (and `claude-self` with no subcommand) prints usage listing all subcommands [show, edit, lint, log, diff, default-restore, path, help] and exits 0. Unknown subcommands exit 2.

Each AC has a matching integration test under `tests/acceptance_ac<n>.rs`.

## Provenance

Built via the [`autobuilder`](https://github.com/j0yen/autobuilder) pipeline (PRD intake -> intent-card -> scaffold -> iterate-and-prove). Originally consolidated as a subdir of the [`wintermute`](https://github.com/j0yen/wintermute) monorepo; this standalone repo is a fresh-init snapshot for easier consumption and distribution.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
