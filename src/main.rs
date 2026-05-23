//! claude-self — CLI entry point.
//!
//! Subcommands: show, edit, lint, log, diff, default-restore, path, help.
//! Manages `~/.claude/CLAUDE_SELF.md`; supersedes the legacy bash draft at
//! `~/dotfiles/.local/bin/claude-self`.

#![cfg_attr(not(test), forbid(unsafe_code))]
// The whole purpose of the binary is to write to stdout/stderr; permit the
// macros here without sprinkling allow attributes on every call site.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use claude_self::{LINT_MAX_LINES, Paths, lint_contents};

const USAGE: &str = "claude-self — manage ~/.claude/CLAUDE_SELF.md

Subcommands:
  show                cat the live file
  edit                $EDITOR with lint-on-save
  lint [--quiet]      validate canonical structure + length cap
  log                 git log restricted to CLAUDE_SELF.md (via dotfiles)
  diff [REF]          git diff vs REF (default HEAD)
  default-restore [--yes]
                      overwrite live file with default template (lints result)
  path                print the live-file path
  help                print this message

Environment overrides:
  CLAUDE_SELF_FILE    live file path (default $HOME/.claude/CLAUDE_SELF.md)
  CLAUDE_SELF_DEFAULT default template path (default $HOME/.claude/CLAUDE_SELF.default.md)
  CLAUDE_DOTFILES     dotfiles repo path (default $HOME/dotfiles)
  EDITOR              editor invoked by `edit` (default vim)
";

fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().collect();
    let home: PathBuf = env::var_os("HOME").map_or_else(PathBuf::new, PathBuf::from);
    let paths = Paths::from_env(&home);
    let sub = args.get(1).and_then(|s| s.to_str()).unwrap_or("help");
    let rest: Vec<&OsString> = args.iter().skip(2).collect();

    let code: i32 = match sub {
        "path" => cmd_path(&paths),
        "show" => cmd_show(&paths),
        "lint" => cmd_lint(&paths, &rest),
        "default-restore" => cmd_default_restore(&paths, &rest),
        "log" => cmd_log(&paths, &rest),
        "diff" => cmd_diff(&paths, &rest),
        "edit" => cmd_edit(&paths),
        "help" | "-h" | "--help" => {
            print!("{USAGE}");
            0
        }
        other => {
            eprintln!("claude-self: unknown subcommand `{other}`");
            eprint!("{USAGE}");
            2
        }
    };
    ExitCode::from(u8::try_from(code.clamp(0, 125)).unwrap_or(2))
}

fn cmd_path(p: &Paths) -> i32 {
    println!("{}", p.live.display());
    0
}

fn cmd_show(p: &Paths) -> i32 {
    let Ok(contents) = fs::read_to_string(&p.live) else {
        eprintln!("claude-self: {} not found", p.live.display());
        return 2;
    };
    print!("{contents}");
    0
}

fn cmd_lint(p: &Paths, rest: &[&OsString]) -> i32 {
    let quiet = rest.iter().any(|a| a.to_str() == Some("--quiet"));
    let Ok(contents) = fs::read_to_string(&p.live) else {
        if !quiet {
            eprintln!("lint: {} missing", p.live.display());
        }
        return 2;
    };
    let report = lint_contents(&contents);
    if report.passed() {
        if !quiet {
            println!(
                "lint: OK ({}/{} lines, all sections present)",
                report.line_count, LINT_MAX_LINES
            );
        }
        0
    } else {
        if !quiet {
            for v in &report.violations {
                eprintln!("lint: {v}");
            }
        }
        1
    }
}

fn cmd_default_restore(p: &Paths, rest: &[&OsString]) -> i32 {
    let assume_yes = rest.iter().any(|a| a.to_str() == Some("--yes"));
    if !assume_yes {
        eprintln!(
            "default-restore: about to overwrite {} with the default template.",
            p.live.display()
        );
        eprintln!("default-restore: pass --yes to confirm non-interactively.");
        return 1;
    }
    let Ok(default_contents) = fs::read_to_string(&p.default) else {
        eprintln!("default-restore: default file missing at {}", p.default.display());
        return 2;
    };
    if let Some(parent) = p.live.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("default-restore: cannot create parent {}: {e}", parent.display());
            return 2;
        }
    }
    if let Err(e) = fs::write(&p.live, &default_contents) {
        eprintln!("default-restore: cannot write {}: {e}", p.live.display());
        return 2;
    }
    let report = lint_contents(&default_contents);
    if report.passed() {
        println!(
            "default-restore: restored {} ({}/{} lines)",
            p.live.display(),
            report.line_count,
            LINT_MAX_LINES
        );
        0
    } else {
        eprintln!("default-restore: restored file FAILS lint:");
        for v in &report.violations {
            eprintln!("  {v}");
        }
        1
    }
}

fn cmd_log(p: &Paths, rest: &[&OsString]) -> i32 {
    if !p.dotfiles.join(".git").is_dir() {
        eprintln!("log: dotfiles repo not found at {}", p.dotfiles.display());
        return 2;
    }
    let rel = relative_self_path(p);
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(&p.dotfiles).arg("log").arg("--oneline");
    for extra in rest {
        cmd.arg(extra);
    }
    cmd.arg("--").arg(rel);
    run_status(&mut cmd)
}

fn cmd_diff(p: &Paths, rest: &[&OsString]) -> i32 {
    if !p.dotfiles.join(".git").is_dir() {
        eprintln!("diff: dotfiles repo not found at {}", p.dotfiles.display());
        return 2;
    }
    let rel = relative_self_path(p);
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(&p.dotfiles).arg("diff");
    let default_ref = OsString::from("HEAD");
    let r = rest.first().copied().unwrap_or(&default_ref);
    cmd.arg(r);
    cmd.arg("--").arg(rel);
    run_status(&mut cmd)
}

fn cmd_edit(p: &Paths) -> i32 {
    if !p.live.exists() {
        eprintln!(
            "claude-self: {} does not exist; create it first or run `claude-self default-restore --yes`",
            p.live.display()
        );
        return 2;
    }
    let editor = env::var_os("EDITOR").unwrap_or_else(|| OsString::from("vim"));
    let mut cmd = Command::new(&editor);
    cmd.arg(&p.live);
    let status = match cmd.status() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("edit: failed to launch editor {editor:?}: {e}");
            return 2;
        }
    };
    if !status.success() {
        eprintln!("edit: editor exited non-zero; aborting lint pass");
        return 1;
    }
    // After edit, run lint synchronously. The bash draft prompted for
    // re-edit/accept/quit; we keep the lint-as-feedback behavior but stay
    // non-interactive at this layer (the orchestrator decides next steps).
    let rest: [&OsString; 0] = [];
    cmd_lint(p, &rest)
}

fn relative_self_path(p: &Paths) -> PathBuf {
    // Per PRD §4.3 the canonical path inside the dotfiles repo is
    // `.claude/CLAUDE_SELF.md`. Strip a `<dotfiles>/` prefix if the live
    // path actually lives under the dotfiles tree; otherwise fall back to
    // the canonical relative path.
    if let Ok(stripped) = p.live.strip_prefix(&p.dotfiles) {
        return stripped.to_path_buf();
    }
    Path::new(".claude").join("CLAUDE_SELF.md")
}

fn run_status(cmd: &mut Command) -> i32 {
    match cmd.status() {
        Ok(s) => i32::from(!s.success()),
        Err(e) => {
            eprintln!("claude-self: failed to spawn git: {e}");
            2
        }
    }
}
