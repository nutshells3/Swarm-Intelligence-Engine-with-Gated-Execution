//! REC-002: Repo-target policy for self-modification.
//!
//! CSV guardrail: "Define repo-target policy for self-modification
//!   (allowlist/denylist, per-task target binding, worktree isolation)."
//! Caution: "Do not allow hidden self-modification."
//! auto_approval_policy: never_silent
//!
//! Acceptance: self-modification is confined to declared repo targets,
//! governed by allowlist/denylist rules, bound per-task, and isolated
//! via worktree.  Hidden modifications outside declared targets are
//! blocked.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Whether a repo path is allowed or denied for self-modification.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AllowDenyRule {
    /// Path is explicitly allowed for self-modification.
    Allow,
    /// Path is explicitly denied -- modifications are blocked.
    Deny,
}

/// Scope of a target rule (package, file glob, or directory subtree).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TargetScope {
    /// A specific workspace package name.
    Package,
    /// A file glob pattern (e.g., "src/**/*.rs").
    FileGlob,
    /// A directory subtree.
    DirectorySubtree,
}

/// Worktree isolation requirement for self-modification tasks.
/// CSV: "worktree isolation".
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeIsolationRequirement {
    /// Task must run in an isolated worktree -- never on the main checkout.
    Required,
    /// Worktree isolation is recommended but not enforced.
    Recommended,
    /// No worktree isolation required (low-risk internal changes).
    NotRequired,
}

/// Repo target policy.
///
/// Governs which repository paths a self-improvement task may modify.
/// Combines allowlist/denylist rules with per-task target binding and
/// worktree isolation requirements.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoTargetPolicy {
    /// Unique policy identifier.
    pub policy_id: String,
    /// The self-improvement objective this policy is bound to.
    pub objective_id: String,
    /// Ordered list of allow/deny rules evaluated top-to-bottom.
    pub rules: Vec<RepoTargetRule>,
    /// Worktree isolation requirement for tasks under this policy.
    pub worktree_isolation: WorktreeIsolationRequirement,
    /// Whether modifications outside declared targets are hard-blocked
    /// (true) or flagged for review (false).  CSV caution: "Do not allow
    /// hidden self-modification" -- default is true.
    pub hard_block_undeclared: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A single allow/deny rule within a repo target policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoTargetRule {
    /// The scope this rule applies to.
    pub scope: TargetScope,
    /// The pattern (package name, glob, or directory path).
    pub pattern: String,
    /// Whether this pattern is allowed or denied.
    pub rule: AllowDenyRule,
    /// Human-readable justification for this rule.
    pub justification: String,
}
