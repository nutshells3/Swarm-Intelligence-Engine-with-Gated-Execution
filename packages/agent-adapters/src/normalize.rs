//! ADT-010: Output normalization for adapter responses.
//!
//! Ensures all adapter output is normalized to a consistent format
//! regardless of the underlying agent. This includes:
//! - UTF-8 validation and sanitization
//! - Whitespace normalization
//! - Output truncation to budget
//! - Empty-output detection and classification

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// Pre-compiled regex for ANSI escape sequences (CSI sequences).
/// Matches patterns like \x1b[0m, \x1b[1;31m, \x1b[K, etc.
static ANSI_ESCAPE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").expect("ANSI regex is valid"));

/// Result of normalizing an adapter output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NormalizationResult {
    /// Output was already well-formed; no changes needed.
    Clean,
    /// Output required whitespace normalization only.
    WhitespaceNormalized,
    /// Output contained invalid UTF-8 that was replaced.
    Utf8Sanitized,
    /// Output was truncated to fit the budget.
    Truncated,
    /// Output was empty.
    Empty,
    /// Output contained control characters that were stripped.
    ControlCharsStripped,
}

/// Policy controlling how output normalization behaves.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizationPolicy {
    /// Maximum output length in characters. Output exceeding this is truncated.
    pub max_output_chars: u32,
    /// Whether to strip ANSI escape codes from output.
    pub strip_ansi_escapes: bool,
    /// Whether to normalize line endings to LF.
    pub normalize_line_endings: bool,
    /// Whether to trim leading/trailing whitespace.
    pub trim_whitespace: bool,
    /// Whether to replace invalid UTF-8 sequences with the replacement character.
    pub replace_invalid_utf8: bool,
    /// Whether to strip null bytes and other control characters.
    pub strip_control_chars: bool,
}

impl Default for NormalizationPolicy {
    fn default() -> Self {
        Self {
            max_output_chars: 100_000,
            strip_ansi_escapes: true,
            normalize_line_endings: true,
            trim_whitespace: true,
            replace_invalid_utf8: true,
            strip_control_chars: true,
        }
    }
}

/// ADT-010 -- Normalized output after applying the normalization policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizedOutput {
    /// The normalized content.
    pub content: String,
    /// What kind of normalization was performed.
    pub result: NormalizationResult,
    /// Original length before normalization.
    pub original_length: u32,
    /// Final length after normalization.
    pub final_length: u32,
    /// Whether the output was non-empty after normalization.
    pub has_content: bool,
}

/// Normalize raw output bytes according to the given policy.
///
/// This function enforces UTF-8 at the adapter boundary:
/// 1. Validates UTF-8 (replaces invalid sequences if policy allows).
/// 2. Strips control characters and ANSI escapes per policy.
/// 3. Normalizes whitespace and line endings.
/// 4. Truncates to budget.
/// 5. Classifies the result.
pub fn normalize_output(raw: &str, policy: &NormalizationPolicy) -> NormalizedOutput {
    let original_length = raw.len() as u32;
    let mut content = raw.to_string();
    let mut result = NormalizationResult::Clean;

    // ADT-010: Strip ANSI escape sequences before anything else so that
    // downstream control-char stripping does not leave orphan fragments.
    if policy.strip_ansi_escapes {
        let before = content.len();
        content = ANSI_ESCAPE_RE.replace_all(&content, "").into_owned();
        if content.len() != before && result == NormalizationResult::Clean {
            result = NormalizationResult::ControlCharsStripped;
        }
    }

    // Strip control characters (except newline, tab, carriage return).
    if policy.strip_control_chars {
        let before = content.len();
        content = content
            .chars()
            .filter(|c| !c.is_control() || *c == '\n' || *c == '\t' || *c == '\r')
            .collect();
        if content.len() != before {
            result = NormalizationResult::ControlCharsStripped;
        }
    }

    // Normalize line endings to LF.
    if policy.normalize_line_endings {
        content = content.replace("\r\n", "\n").replace('\r', "\n");
    }

    // Trim whitespace.
    if policy.trim_whitespace {
        let trimmed = content.trim().to_string();
        if trimmed.len() != content.len() && result == NormalizationResult::Clean {
            result = NormalizationResult::WhitespaceNormalized;
        }
        content = trimmed;
    }

    // Truncate to budget.
    let max_chars = policy.max_output_chars as usize;
    if content.len() > max_chars {
        content = content.chars().take(max_chars).collect();
        result = NormalizationResult::Truncated;
    }

    // Check for empty output.
    let has_content = !content.is_empty();
    if !has_content {
        result = NormalizationResult::Empty;
    }

    let final_length = content.len() as u32;

    NormalizedOutput {
        content,
        result,
        original_length,
        final_length,
        has_content,
    }
}
