//! Problems in a file, found by a plugin that declares
//! `contributes.linter` in its manifest.
//!
//! The editor asks `lint/run` with the text of a file as typed, and shows
//! the [`Diagnostic`]s of the answer with those of the language servers;
//! `lint/fix` asks for the text with the problems fixed. Lines count from
//! 0, columns are byte offsets in their line, ends are excluded.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Hint,
    Info,
    Warning,
    Error,
}

/// A replacement in the text the problem was found in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edit {
    pub line: usize,
    pub col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub text: String,
}

/// What puts a problem right.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fix {
    pub title: String,
    pub edits: Vec<Edit>,
    /// Safe to apply without looking; the others are offered one by one.
    #[serde(default)]
    pub safe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub line: usize,
    pub col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub severity: Severity,
    pub message: String,
    /// The rule: `comma-spacing`, `E501`…
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Who found it: `ESLint`, `ruff`…
    pub source: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<Fix>,
}

/// What is fixed: the errors, the warnings too, or everything.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Errors,
    #[default]
    Warnings,
    All,
}

impl Level {
    pub fn covers(self, severity: Severity) -> bool {
        match self {
            Level::Errors => severity == Severity::Error,
            Level::Warnings => severity >= Severity::Warning,
            Level::All => true,
        }
    }
}

/// `lint/run`: the problems of a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintParams {
    pub path: PathBuf,
    /// The text as typed.
    pub text: String,
    /// The text differs from the file on disk (tools that read the disk
    /// would speak of another text).
    #[serde(default)]
    pub dirty: bool,
    /// The user trusts the project: its own programs (`node_modules/.bin`,
    /// a virtualenv) may run.
    #[serde(default)]
    pub trusted: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintResult {
    pub diagnostics: Vec<Diagnostic>,
    /// What failed, as `tool: why`.
    #[serde(default)]
    pub failed: Vec<String>,
    /// The tools that looked at the file.
    #[serde(default)]
    pub ran: Vec<String>,
    /// The tools left out because the text is not saved: their earlier
    /// problems are kept.
    #[serde(default)]
    pub skipped: Vec<String>,
    /// The editor should add what its language server finds.
    #[serde(default)]
    pub language_server: bool,
}

/// `lint/fix`: the text with the problems fixed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixParams {
    pub path: PathBuf,
    pub text: String,
    #[serde(default)]
    pub dirty: bool,
    #[serde(default)]
    pub trusted: bool,
    #[serde(default)]
    pub level: Level,
    /// Only the problems of this source (a tool's name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixResult {
    /// The text fixed (`None`: nothing to fix).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The editor should apply the safe fixes of its language server after.
    #[serde(default)]
    pub language_server: bool,
}

/// The byte offset of (`line`, `col`) in `text`.
pub fn offset_of(text: &str, line: usize, col: usize) -> usize {
    let mut start = 0;
    for _ in 0..line {
        match text[start..].find('\n') {
            Some(i) => start += i + 1,
            None => return text.len(),
        }
    }
    let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
    // Never inside a character: a position from older text could land there.
    let mut at = (start + col).min(end);
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The (line, byte column) of byte `offset` in `text`.
pub fn position_of(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let before = &text.as_bytes()[..offset];
    let line = before.iter().filter(|b| **b == b'\n').count();
    let start = before
        .iter()
        .rposition(|b| *b == b'\n')
        .map_or(0, |i| i + 1);
    (line, offset - start)
}

/// `text` with the fixes applied. A fix that touches what another one
/// already changed is left out (it applies on the next run). Returns the new
/// text and how many fixes went in.
pub fn apply(text: &str, fixes: &[&Fix]) -> (String, usize) {
    let mut taken: Vec<(usize, usize, &str)> = Vec::new();
    let mut applied = 0;
    let mut ordered: Vec<&&Fix> = fixes.iter().collect();
    ordered.sort_by_key(|f| {
        f.edits
            .iter()
            .map(|e| offset_of(text, e.line, e.col))
            .min()
            .unwrap_or(usize::MAX)
    });
    for fix in ordered {
        let spans: Vec<(usize, usize, &str)> = fix
            .edits
            .iter()
            .map(|e| {
                let start = offset_of(text, e.line, e.col);
                let end = offset_of(text, e.end_line, e.end_col).max(start);
                (start, end, e.text.as_str())
            })
            .collect();
        let clash = spans.iter().any(|(s, e, _)| {
            taken
                .iter()
                .any(|(ts, te, _)| (s < te && ts < e) || (s == ts && e == te))
        });
        if clash || spans.is_empty() {
            continue;
        }
        taken.extend(spans);
        applied += 1;
    }
    taken.sort_by_key(|(s, e, _)| (*s, *e));
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (start, end, replacement) in taken {
        if start < at {
            continue;
        }
        out.push_str(&text[at..start]);
        out.push_str(replacement);
        at = end;
    }
    out.push_str(&text[at..]);
    (out, applied)
}

/// The same, for the problems that `level` shows.
pub fn fixes_at(diagnostics: &[Diagnostic], level: Level) -> Vec<&Fix> {
    diagnostics
        .iter()
        .filter(|d| level.covers(d.severity))
        .filter_map(|d| d.fixes.first().filter(|f| f.safe))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix(line: usize, col: usize, end_col: usize, text: &str) -> Fix {
        Fix {
            title: "f".into(),
            edits: vec![Edit {
                line,
                col,
                end_line: line,
                end_col,
                text: text.into(),
            }],
            safe: true,
        }
    }

    #[test]
    fn fixes_apply_from_the_end_and_overlaps_are_left_out() {
        let text = "let a=1;\nlet b=2;\n";
        let (a, b, c) = (fix(0, 5, 6, " = "), fix(1, 5, 6, " = "), fix(0, 5, 7, "x"));
        let (out, n) = apply(text, &[&a, &b, &c]);
        assert_eq!(out, "let a = 1;\nlet b = 2;\n");
        assert_eq!(n, 2);
    }

    #[test]
    fn positions_past_the_end_are_clamped() {
        assert_eq!(offset_of("ab\ncd", 1, 99), 5);
        assert_eq!(offset_of("ab\ncd", 9, 0), 5);
        assert_eq!(offset_of("é", 0, 1), 0);
    }

    #[test]
    fn results_are_plain_json() {
        let r = LintResult {
            diagnostics: vec![Diagnostic {
                line: 0,
                col: 0,
                end_line: 0,
                end_col: 1,
                severity: Severity::Warning,
                message: "m".into(),
                code: None,
                source: "t".into(),
                fixes: vec![],
            }],
            ..LintResult::default()
        };
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["diagnostics"][0]["severity"], "warning");
        assert_eq!(serde_json::from_value::<LintResult>(json).unwrap(), r);
    }
}
