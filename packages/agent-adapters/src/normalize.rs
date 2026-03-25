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

/// Pre-compiled regex for codex exec metadata lines.
/// Matches lines like "model: ...", "provider: ...", "session id: ...",
/// "tokens used: ..." and the section separator lines.
static CODEX_METADATA_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?m)^(model:\s.*|provider:\s.*|session id:\s.*|tokens used:\s.*|duration:\s.*|─+\s*$)"
    )
    .expect("codex metadata regex is valid")
});

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

/// Normalized output after applying the normalization policy.
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

    // Strip ANSI escape sequences before anything else so that
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

/// Extract the actual response content from codex exec's multi-section output.
///
/// Codex exec outputs structured text with sections like:
/// ```text
/// ──────────────────────────────
/// codex
/// <actual response content here>
/// ──────────────────────────────
/// model: gpt-5.4
/// provider: openai
/// session id: ...
/// tokens used: ...
/// duration: ...
/// ```
///
/// This function strips the metadata lines and section separators,
/// extracting only the actual response content.
pub fn extract_codex_exec_content(raw: &str) -> String {
    // Strategy: find the content between the first "codex" header and the
    // metadata block that follows.
    let lines: Vec<&str> = raw.lines().collect();

    // Find the "codex" section header (case-insensitive)
    let header_idx = lines
        .iter()
        .position(|l| l.trim().eq_ignore_ascii_case("codex"));

    if let Some(start) = header_idx {
        // Content starts after the "codex" header line
        let content_start = start + 1;

        // Find where metadata begins: look for "tokens used:" or "model:" lines
        let metadata_start = lines[content_start..]
            .iter()
            .position(|l| {
                let trimmed = l.trim();
                trimmed.starts_with("tokens used:")
                    || trimmed.starts_with("model:")
                    || trimmed.starts_with("provider:")
                    || trimmed.starts_with("session id:")
                    || trimmed.starts_with("duration:")
            })
            .map(|idx| content_start + idx)
            .unwrap_or(lines.len());

        // Also look backwards from metadata_start to skip any trailing separator lines
        let mut content_end = metadata_start;
        while content_end > content_start
            && lines[content_end - 1]
                .trim()
                .chars()
                .all(|c| c == '─' || c == '-' || c.is_whitespace())
            && !lines[content_end - 1].trim().is_empty()
        {
            content_end -= 1;
        }

        // Extract the content lines, skipping any leading separator
        let content_lines: Vec<&str> = lines[content_start..content_end]
            .iter()
            .copied()
            .skip_while(|l| {
                l.trim()
                    .chars()
                    .all(|c| c == '─' || c == '-' || c.is_whitespace())
                    && !l.trim().is_empty()
            })
            .collect();

        return content_lines.join("\n");
    }

    // If no "codex" header found, strip metadata lines as a fallback
    let stripped = CODEX_METADATA_RE.replace_all(raw, "");
    stripped.trim().to_string()
}
