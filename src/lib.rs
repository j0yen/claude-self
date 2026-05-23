//! claude-self — manage `~/.claude/CLAUDE_SELF.md`.
//!
//! Library exposing the linter and path-resolution helpers used by the
//! binary. Kept in a library so acceptance tests can exercise the lint
//! contract directly without spawning a subprocess for every case.

#![cfg_attr(not(test), forbid(unsafe_code))]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

use std::fmt;
use std::path::{Path, PathBuf};

/// The seven canonical section headers, in canonical order.
/// Lifted from the PRD-claude-self §4.4 lint contract.
pub const CANONICAL_SECTIONS: [&str; 7] = [
    "Voice",
    "Values",
    "Defaults",
    "Things I keep getting wrong",
    "Aspirations",
    "Boundaries",
    "Changelog",
];

/// Lint cap on total lines.
pub const LINT_MAX_LINES: usize = 200;

/// Lint rule identifiers; surfaced in diagnostics so failures name the rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LintRule {
    /// File exceeded `LINT_MAX_LINES` total lines.
    LineCount {
        /// Observed total line count.
        actual: usize,
        /// Configured cap (= `LINT_MAX_LINES`).
        cap: usize,
    },
    /// Section headers did not match `CANONICAL_SECTIONS` exactly, in order.
    SectionMismatch {
        /// The canonical section list the file must match.
        expected: Vec<String>,
        /// The section headers extracted from the file, in document order.
        got: Vec<String>,
    },
    /// Aspirations section had zero non-empty bullets (must be >=1).
    EmptyAspirations,
    /// One or more sections had duplicate bullet text.
    DuplicateBullets {
        /// `(section_name, bullet_text)` pairs for each duplicate.
        duplicates: Vec<(String, String)>,
    },
}

impl fmt::Display for LintRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LineCount { actual, cap } => {
                write!(f, "line-count: {actual} lines exceeds cap of {cap}")
            }
            Self::SectionMismatch { expected, got } => {
                writeln!(f, "section-mismatch: section headers do not match canonical set")?;
                writeln!(f, "  expected:")?;
                for s in expected {
                    writeln!(f, "    {s}")?;
                }
                writeln!(f, "  got:")?;
                for s in got {
                    writeln!(f, "    {s}")?;
                }
                Ok(())
            }
            Self::EmptyAspirations => {
                write!(
                    f,
                    "empty-aspirations: Aspirations section is empty (must have >=1 bullet)"
                )
            }
            Self::DuplicateBullets { duplicates } => {
                writeln!(f, "duplicate-bullets: duplicate bullets within section(s):")?;
                for (section, bullet) in duplicates {
                    writeln!(f, "  [{section}] {bullet}")?;
                }
                Ok(())
            }
        }
    }
}

/// Outcome of a lint run.
#[derive(Debug, Clone)]
pub struct LintReport {
    /// Total line count of the input.
    pub line_count: usize,
    /// All rule violations; empty means the lint passed.
    pub violations: Vec<LintRule>,
}

impl LintReport {
    /// True when the report has no rule violations.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.violations.is_empty()
    }
}

/// Lint a file's contents against the PRD-claude-self §4.4 rules.
///
/// Returns a `LintReport` listing every rule violation (or none on pass).
#[must_use]
pub fn lint_contents(contents: &str) -> LintReport {
    let lines: Vec<&str> = contents.lines().collect();
    let line_count = lines.len();
    let mut violations: Vec<LintRule> = Vec::new();

    // Rule 1: line count cap.
    if line_count > LINT_MAX_LINES {
        violations.push(LintRule::LineCount { actual: line_count, cap: LINT_MAX_LINES });
    }

    // Extract `## <name>` headers in document order.
    let got_sections: Vec<String> = lines
        .iter()
        .filter_map(|line| line.strip_prefix("## ").map(str::trim).map(str::to_string))
        .collect();
    let expected_sections: Vec<String> =
        CANONICAL_SECTIONS.iter().map(|s| (*s).to_string()).collect();

    // Rule 2: section headers must match canonical set in order.
    if got_sections != expected_sections {
        violations.push(LintRule::SectionMismatch {
            expected: expected_sections,
            got: got_sections,
        });
    }

    // Rule 3: Aspirations section has >=1 non-empty bullet.
    if aspirations_bullet_count(&lines) < 1 {
        violations.push(LintRule::EmptyAspirations);
    }

    // Rule 4: no duplicate bullets within a section.
    let dupes = find_duplicate_bullets(&lines);
    if !dupes.is_empty() {
        violations.push(LintRule::DuplicateBullets { duplicates: dupes });
    }

    LintReport { line_count, violations }
}

fn aspirations_bullet_count(lines: &[&str]) -> usize {
    let mut in_sec = false;
    let mut n: usize = 0;
    for line in lines {
        if let Some(name) = line.strip_prefix("## ") {
            in_sec = name.trim() == "Aspirations";
            continue;
        }
        if in_sec {
            if let Some(rest) = line.strip_prefix("- ") {
                // Non-empty bullet means the first char after "- " is not whitespace.
                if rest.chars().next().is_some_and(|c| !c.is_whitespace()) {
                    n = n.saturating_add(1);
                }
            }
        }
    }
    n
}

fn find_duplicate_bullets(lines: &[&str]) -> Vec<(String, String)> {
    let mut dupes: Vec<(String, String)> = Vec::new();
    let mut current_section = String::new();
    let mut seen_in_section: Vec<String> = Vec::new();
    for line in lines {
        if let Some(name) = line.strip_prefix("## ") {
            current_section = name.trim().to_string();
            seen_in_section.clear();
            continue;
        }
        // Match any "- " bullet; mirror the bash draft's awk grep on `^- `.
        if line.starts_with("- ") {
            let bullet = (*line).to_string();
            if seen_in_section.iter().any(|seen| seen == &bullet) {
                dupes.push((current_section.clone(), bullet));
            } else {
                seen_in_section.push(bullet);
            }
        }
    }
    dupes
}

/// Path resolver — looks up live-file, default-file, and dotfiles-repo paths
/// honoring environment overrides.
#[derive(Debug, Clone)]
pub struct Paths {
    /// Resolved path to the live `CLAUDE_SELF.md` file.
    pub live: PathBuf,
    /// Resolved path to the default-template `CLAUDE_SELF.default.md`.
    pub default: PathBuf,
    /// Resolved path to the dotfiles repo (for `git log` / `git diff`).
    pub dotfiles: PathBuf,
}

impl Paths {
    /// Resolve paths from environment variables, with `home` as the fallback root.
    #[must_use]
    pub fn from_env(home: &Path) -> Self {
        let live = std::env::var_os("CLAUDE_SELF_FILE")
            .map_or_else(|| home.join(".claude").join("CLAUDE_SELF.md"), PathBuf::from);
        let default = std::env::var_os("CLAUDE_SELF_DEFAULT").map_or_else(
            || home.join(".claude").join("CLAUDE_SELF.default.md"),
            PathBuf::from,
        );
        let dotfiles = std::env::var_os("CLAUDE_DOTFILES")
            .map_or_else(|| home.join("dotfiles"), PathBuf::from);
        Self { live, default, dotfiles }
    }
}

#[cfg(test)]
mod tests {
    use super::{LintRule, lint_contents};

    const GOOD_FILE: &str = "\
# Header
## Voice
- Terse.
## Values
- Honest.
## Defaults
- Parallel tool calls.
## Things I keep getting wrong
- Over-narrating.
## Aspirations
- Be a collaborator.
## Boundaries
- No irreversible ops without approval.
## Changelog
- 2026-05-23 (Claude): seed.
";

    #[test]
    fn good_file_lints_clean() {
        let report = lint_contents(GOOD_FILE);
        assert!(report.passed(), "expected pass, got {:?}", report.violations);
    }

    #[test]
    fn missing_section_fails() {
        let bad = GOOD_FILE.replace("## Boundaries\n- No irreversible ops without approval.\n", "");
        let report = lint_contents(&bad);
        assert!(!report.passed());
        assert!(
            report.violations.iter().any(|v| matches!(v, LintRule::SectionMismatch { .. })),
            "expected section-mismatch violation; got {:?}",
            report.violations
        );
    }

    #[test]
    fn empty_aspirations_fails() {
        let bad = GOOD_FILE.replace("- Be a collaborator.\n", "");
        let report = lint_contents(&bad);
        assert!(report.violations.iter().any(|v| matches!(v, LintRule::EmptyAspirations)));
    }

    #[test]
    fn duplicate_bullet_fails() {
        let bad = GOOD_FILE.replace("## Values\n- Honest.\n", "## Values\n- Honest.\n- Honest.\n");
        let report = lint_contents(&bad);
        assert!(
            report.violations.iter().any(|v| matches!(v, LintRule::DuplicateBullets { .. })),
            "expected duplicate-bullets violation; got {:?}",
            report.violations
        );
    }

    #[test]
    fn over_long_fails() {
        let mut bad = String::from("# Header\n## Voice\n");
        for i in 0..210 {
            bad.push_str(&format!("- v{i}\n"));
        }
        bad.push_str("## Values\n- v\n## Defaults\n- d\n## Things I keep getting wrong\n- t\n");
        bad.push_str("## Aspirations\n- a\n## Boundaries\n- b\n## Changelog\n- c\n");
        let report = lint_contents(&bad);
        assert!(report.violations.iter().any(|v| matches!(v, LintRule::LineCount { .. })));
    }
}
