//! Git and worktree control (GIT-001 through GIT-010).
//!
//! Goal: Govern code isolation so workers do not stomp each other.
//! Caution: Do not merge conflicting edits automatically.
//!
//! This crate provides typed schemas and traits for:
//! - Repository targeting and branch ownership (GIT-001 to GIT-004)
//! - Worker-to-worktree binding and detection traits (GIT-005 to GIT-008)
//! - Review-before-merge and cleanup rules (GIT-009, GIT-010)
//!
//! All types are serializable for persistence and audit.

pub mod worktree;

pub use worktree::{
    WorktreeError as OpWorkTreeError,
    WorktreeInfo as OpWorktreeInfo,
    WorktreeManager,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── GIT-001: Repo target schema ───────────────────────────────────────────
//
// Identifies the repository and ref a worker operates against.

/// GIT-001 -- Repository target.
///
/// Every worker operation is scoped to a specific repo + ref.
/// The control plane uses this to enforce isolation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoTarget {
    /// Path to the repository root (absolute).
    pub repo_path: String,
    /// The base branch (e.g. "main").
    pub base_branch: String,
    /// Optional remote name (defaults to "origin").
    pub remote_name: Option<String>,
    /// Repository identifier for cross-repo references.
    pub repo_id: String,
}

// ── GIT-002: Branch ownership rules ───────────────────────────────────────
//
// Defines which workers are allowed to operate on which branches.

/// Ownership scope for a branch rule.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BranchOwnershipScope {
    /// Only one worker may use this branch at a time.
    Exclusive,
    /// Multiple workers of the same role may share.
    SharedByRole,
    /// Any worker may use (e.g. read-only branches).
    Open,
}

/// GIT-002 -- Branch ownership rule.
///
/// Controls which workers can operate on a branch pattern.
/// Patterns use glob syntax (e.g. "worker/*", "feature/impl-*").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BranchOwnershipRule {
    /// Unique rule identifier.
    pub rule_id: String,
    /// Glob pattern for matching branch names.
    pub branch_pattern: String,
    /// Ownership scope.
    pub scope: BranchOwnershipScope,
    /// Optional: worker role that owns this pattern.
    pub owning_role: Option<String>,
    /// Optional: specific worker ID that owns this pattern.
    pub owning_worker_id: Option<String>,
    /// Whether the rule is currently active.
    pub active: bool,
}

// ── GIT-003: Worktree assignment rules ────────────────────────────────────
//
// Maps workers to isolated git worktrees.

/// GIT-003 -- Worktree assignment.
///
/// Each worker gets its own worktree to prevent file contention.
/// The control plane tracks which worktrees are in use.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorktreeAssignment {
    /// Unique assignment identifier.
    pub assignment_id: String,
    /// Worker assigned to this worktree.
    pub worker_id: String,
    /// Task being executed in this worktree.
    pub task_id: String,
    /// Absolute path to the worktree directory.
    pub worktree_path: String,
    /// Branch the worktree is on.
    pub branch_name: String,
    /// When the assignment was made.
    pub assigned_at: DateTime<Utc>,
    /// When the assignment was released (None if still active).
    pub released_at: Option<DateTime<Utc>>,
    /// Whether the worktree is currently active.
    pub active: bool,
}

// ── GIT-004: File ownership hints ─────────────────────────────────────────
//
// Advisory hints about which files "belong" to which work items.

/// GIT-004 -- File ownership hint.
///
/// Advisory (not enforced) hints about file ownership. The conflict
/// detector (GIT-008) uses these to flag likely conflicts before
/// merge.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileOwnershipHint {
    /// File path pattern (glob).
    pub file_pattern: String,
    /// Node that "owns" this file area.
    pub owning_node_id: Option<String>,
    /// Worker currently modifying files matching this pattern.
    pub active_worker_id: Option<String>,
    /// Human-readable rationale for the ownership claim.
    pub rationale: String,
}

// ── GIT-005: Worker-to-worktree binding ───────────────────────────────────
//
// Trait for binding and unbinding workers to worktrees.

/// GIT-005 -- Worker-to-worktree binding trait.
///
/// Implementors manage the lifecycle of worktree assignments:
/// creating worktrees, assigning workers, and releasing on completion.
pub trait WorkerToWorktreeBinding {
    /// Assign a worktree to a worker for a task.
    /// Returns the worktree assignment on success.
    fn bind_worker(
        &self,
        worker_id: &str,
        task_id: &str,
        repo_target: &RepoTarget,
        branch_name: &str,
    ) -> Result<WorktreeAssignment, WorktreeError>;

    /// Release a worktree assignment.
    fn unbind_worker(
        &self,
        assignment_id: &str,
    ) -> Result<(), WorktreeError>;

    /// List active worktree assignments.
    fn active_assignments(&self) -> Vec<WorktreeAssignment>;
}

/// Error type for worktree operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorktreeError {
    pub kind: WorktreeErrorKind,
    pub message: String,
}

/// Classification of worktree errors.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeErrorKind {
    /// The worktree path already exists.
    PathConflict,
    /// The branch is already owned by another worker.
    BranchOwnershipViolation,
    /// The assignment was not found.
    AssignmentNotFound,
    /// Git operation failed.
    GitOperationFailed,
    /// The worktree has uncommitted changes.
    DirtyWorktree,
}

// ── GIT-006: Dirty worktree detection ─────────────────────────────────────
//
// Detects worktrees with uncommitted changes.

/// GIT-006 -- Dirty worktree detection result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirtyWorktreeReport {
    /// Worktree path.
    pub worktree_path: String,
    /// Worker assigned to this worktree.
    pub worker_id: String,
    /// Modified files.
    pub modified_files: Vec<String>,
    /// Untracked files.
    pub untracked_files: Vec<String>,
    /// Whether the worktree has staged but uncommitted changes.
    pub has_staged_changes: bool,
}

/// GIT-006 -- Dirty worktree detector trait.
///
/// Implementors check worktrees for uncommitted changes and report
/// them for operator attention.
pub trait DirtyWorktreeDetector {
    /// Check a single worktree for uncommitted changes.
    fn check_worktree(&self, worktree_path: &str) -> Result<Option<DirtyWorktreeReport>, WorktreeError>;

    /// Check all active worktrees.
    fn check_all_active(&self) -> Vec<DirtyWorktreeReport>;
}

// ── GIT-007: Merge candidate detection ────────────────────────────────────
//
// Identifies branches ready to merge into the base branch.

/// GIT-007 -- Merge candidate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergeCandidate {
    /// Branch name.
    pub branch_name: String,
    /// Node ID that produced this branch.
    pub node_id: String,
    /// Number of commits ahead of base.
    pub commits_ahead: u32,
    /// Whether the branch can be fast-forwarded.
    pub fast_forward_possible: bool,
    /// Whether there are known conflicts with the base.
    pub has_conflicts: bool,
    /// Whether all tasks on the node are complete.
    pub all_tasks_complete: bool,
    /// Whether the node has passed review.
    pub review_passed: bool,
    /// Whether the node has passed certification.
    pub certification_passed: bool,
}

/// GIT-007 -- Merge candidate detector trait.
///
/// Implementors scan branches and identify those eligible for merge.
pub trait MergeCandidateDetector {
    /// Find all branches that are candidates for merging into base.
    fn detect_candidates(&self, repo_target: &RepoTarget) -> Vec<MergeCandidate>;
}

// ── GIT-008: Conflicting edit detection ───────────────────────────────────
//
// Detects when multiple workers have edited the same files.
// Caution: Do not merge conflicting edits automatically.

/// GIT-008 -- Conflicting edit report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConflictingEditReport {
    /// File path with conflicting edits.
    pub file_path: String,
    /// Workers that have modified this file.
    pub worker_ids: Vec<String>,
    /// Branch names with conflicting changes.
    pub branch_names: Vec<String>,
    /// Node IDs involved.
    pub node_ids: Vec<String>,
    /// Whether the conflict is textual (git-level) or semantic.
    pub conflict_kind: ConflictKind,
}

/// Classification of a conflict.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    /// Git-level textual conflict (overlapping hunks).
    Textual,
    /// Same file modified but non-overlapping hunks.
    NonOverlapping,
    /// Semantic conflict (e.g. incompatible API changes).
    Semantic,
}

/// GIT-008 -- Conflicting edit detector trait.
///
/// Implementors scan active branches for overlapping edits.
/// The control plane must never auto-merge conflicting edits.
pub trait ConflictingEditDetector {
    /// Detect conflicting edits across active branches.
    fn detect_conflicts(&self, repo_target: &RepoTarget) -> Vec<ConflictingEditReport>;
}

// ── GIT-009: Review-before-merge rule ─────────────────────────────────────
//
// Ensures that branches are not merged without review.

/// GIT-009 -- Review-before-merge rule.
///
/// Defines the conditions that must be met before a branch can
/// be merged. The control plane blocks merge operations until
/// all conditions are satisfied.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewBeforeMergeRule {
    /// Unique rule identifier.
    pub rule_id: String,
    /// Branch pattern this rule applies to (glob).
    pub branch_pattern: String,
    /// Whether review is required.
    pub require_review: bool,
    /// Whether certification is required.
    pub require_certification: bool,
    /// Whether all tasks on the node must be complete.
    pub require_all_tasks_complete: bool,
    /// Whether the branch must have no conflicts with base.
    pub require_no_conflicts: bool,
    /// Minimum number of approvals needed.
    pub min_approvals: u32,
    /// Whether the rule is currently active.
    pub active: bool,
}

// ── GIT-010: Safe cleanup rules ───────────────────────────────────────────
//
// Governs when and how merged branches and worktrees are cleaned up.

/// GIT-010 -- Safe cleanup rules.
///
/// Controls the lifecycle of branches and worktrees after merge
/// or abandonment. The control plane uses these rules to safely
/// remove resources without losing unmerged work.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafeCleanupRules {
    /// Unique rule set identifier.
    pub rule_id: String,
    /// Whether to delete branches after successful merge.
    pub delete_merged_branches: bool,
    /// Whether to remove worktrees after task completion.
    pub remove_completed_worktrees: bool,
    /// Grace period in seconds before cleanup (allows rollback).
    pub cleanup_grace_seconds: u64,
    /// Whether to archive (not delete) abandoned branches.
    pub archive_abandoned: bool,
    /// Maximum number of archived branches to retain.
    pub max_archived_branches: u32,
    /// Whether to require dirty-worktree check before cleanup.
    pub require_clean_before_delete: bool,
}

impl Default for SafeCleanupRules {
    fn default() -> Self {
        Self {
            rule_id: String::new(),
            delete_merged_branches: true,
            remove_completed_worktrees: true,
            cleanup_grace_seconds: 300,
            archive_abandoned: true,
            max_archived_branches: 50,
            require_clean_before_delete: true,
        }
    }
}
