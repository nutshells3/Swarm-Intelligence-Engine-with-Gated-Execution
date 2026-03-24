//! Conflict system (CNF-001 through CNF-010).
//!
//! This crate handles incompatible worker outputs without silent overwrite.
//! Conflicts are first-class records with full history retention.
//!
//! Key design rules:
//! - Do not merge conflicting edits automatically.
//! - Do not lose conflict history (even after resolution).
//! - Same conflict trigger on same artifact set must not create duplicates
//!   (idempotency via `conflict_fingerprint`).
//!
//! Items: CNF-001 through CNF-010.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── CNF-001: Conflict classes ───────────────────────────────────────────
//
// CSV guardrail: "Define conflict classes."
// Acceptance: classes are typed enum, not string.

/// Classification of a conflict between concurrent worker outputs.
///
/// Each class maps to a distinct detection and resolution path so the
/// control plane can route conflicts without guessing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ConflictClass {
    /// Two branches produced divergent outputs for the same target
    /// (e.g., different implementations of the same function).
    Divergence,
    /// Two workers decomposed the same unit of work differently
    /// (e.g., conflicting milestone decompositions).
    Decomposition,
    /// Two workers produced contradictory evidence about the same claim
    /// (e.g., conflicting test results).
    Evidence,
    /// Two reviewers reached incompatible conclusions about the same
    /// artifact.
    ReviewDisagreement,
    /// A branch integration conflicts with the current mainline state.
    MainlineIntegration,
}

// ── CNF-002: Conflict creation triggers ─────────────────────────────────
//
// CSV guardrail: "Define conflict creation triggers."
// Acceptance: triggers are explicit and typed.

/// What triggered the creation of a conflict record.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ConflictTrigger {
    /// Symbol-level overlap detected (links to ROB-016).
    SymbolOverlapDetected,
    /// Semantic conflict detected (links to ROB-017).
    SemanticConflictDetected,
    /// Two workers completed the same task with different outputs.
    DuplicateTaskCompletion,
    /// Mainline integration pre-check failed.
    MainlinePreCheckFailed,
    /// Review outcomes disagree on the same artifact.
    ReviewOutcomeDisagreement,
    /// Decomposition produced incompatible task trees.
    DecompositionMismatch,
    /// Evidence artifacts contradict each other.
    EvidenceContradiction,
    /// Manual conflict report by a human reviewer.
    ManualReport,
}

// ── CNF-003: Competing artifact linking rules ───────────────────────────
//
// CSV guardrail: "Define competing-artifact linking rules."
// Acceptance: links are explicit, bidirectional, and typed.

/// A reference to one side of a competing artifact pair.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompetingArtifactLink {
    /// The node that produced this artifact.
    pub node_id: String,
    /// The task that produced this artifact.
    pub task_id: String,
    /// Hash of the artifact content for comparison.
    pub artifact_hash: String,
    /// Human-readable summary of what this artifact contains.
    pub artifact_summary: String,
    /// When this artifact was produced.
    pub produced_at: DateTime<Utc>,
}

/// Lifecycle status of a conflict record.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStatus {
    /// Conflict detected and awaiting triage.
    Open,
    /// Under active adjudication.
    UnderAdjudication,
    /// Adjudication complete; one side chosen.
    Resolved,
    /// Both sides superseded by a third output.
    Superseded,
    /// Dismissed as a false positive (kept for history).
    Dismissed,
}

/// Full conflict record (CNF-001 through CNF-003 combined).
///
/// This is the first-class conflict object. It is never silently
/// overwritten or deleted. Even after resolution, the record is retained
/// for conflict history analysis (CNF-009).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConflictRecord {
    /// Unique conflict identifier (UUIDv7 recommended).
    pub conflict_id: String,
    /// Fingerprint for idempotency: same trigger on same artifact set
    /// must not create duplicates.
    pub conflict_fingerprint: String,
    /// Classification of the conflict.
    pub conflict_class: ConflictClass,
    /// What triggered this conflict.
    pub trigger: ConflictTrigger,
    /// Current lifecycle status.
    pub status: ConflictStatus,
    /// The competing artifacts (at least two).
    pub competing_artifacts: Vec<CompetingArtifactLink>,
    /// Human-readable description of the conflict.
    pub description: String,
    /// Whether this conflict blocks promotion of affected nodes.
    pub blocks_promotion: bool,
    /// Optional link to a semantic conflict artifact (ROB-018).
    pub semantic_conflict_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ── CNF-004 to CNF-007: Conflict creation by class ──────────────────────
//
// CSV guardrails:
//   CNF-004: "Implement divergence conflict creation."
//   CNF-005: "Implement decomposition conflict creation."
//   CNF-006: "Implement evidence conflict creation."
//   CNF-007: "Implement review disagreement conflict creation."
// Acceptance: each class has an explicit creation payload.

/// Payload for creating a divergence conflict (CNF-004).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DivergenceConflictPayload {
    /// The nodes whose outputs diverged.
    pub diverging_node_ids: Vec<String>,
    /// The target entity (function, module, etc.) both nodes attempted to produce.
    pub target_ref: String,
    /// Symbol-level overlap details (from ROB-016), if any.
    pub overlap_details: Option<String>,
}

/// Payload for creating a decomposition conflict (CNF-005).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecompositionConflictPayload {
    /// The source unit of work that was decomposed differently.
    pub source_milestone_id: String,
    /// The competing decomposition results (milestone tree refs).
    pub competing_tree_refs: Vec<String>,
}

/// Payload for creating an evidence conflict (CNF-006).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceConflictPayload {
    /// The claim that received contradictory evidence.
    pub claim_ref: String,
    /// The competing evidence artifact hashes.
    pub competing_evidence_hashes: Vec<String>,
    /// Human-readable description of the contradiction.
    pub contradiction_summary: String,
}

/// Payload for creating a review disagreement conflict (CNF-007).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewDisagreementPayload {
    /// The artifact that received conflicting reviews.
    pub artifact_ref: String,
    /// The review artifact IDs that disagree.
    pub disagreeing_review_ids: Vec<String>,
    /// Summary of the disagreement.
    pub disagreement_summary: String,
}

// ── CNF-008: Adjudication task generation ───────────────────────────────
//
// CSV guardrail: "Implement adjudication task generation."
// Acceptance: adjudication is an explicit task, not a silent merge.

/// Adjudication urgency level.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AdjudicationUrgency {
    /// Normal priority; can wait for the next review cycle.
    Normal,
    /// Elevated priority; blocking downstream work.
    Elevated,
    /// Critical; multiple downstream tasks are blocked.
    Critical,
}

/// An adjudication task generated from a conflict record. This task is
/// dispatched to a qualified reviewer (human or supervisor agent) who
/// must pick a resolution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdjudicationTask {
    /// Unique adjudication task identifier.
    pub adjudication_id: String,
    /// The conflict being adjudicated.
    pub conflict_id: String,
    /// Urgency level.
    pub urgency: AdjudicationUrgency,
    /// The qualified reviewer role required.
    pub required_reviewer_role: String,
    /// Context summary for the adjudicator (bounded, not a full dump).
    pub context_summary: String,
    /// The competing artifact links for side-by-side comparison.
    pub competing_artifacts: Vec<CompetingArtifactLink>,
    /// Whether the adjudication was assigned to a specific worker.
    pub assigned_worker_id: Option<String>,
    /// Current status of the adjudication.
    pub adjudication_status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ── CNF-009: Conflict history retention ─────────────────────────────────
//
// CSV guardrail: "Implement conflict history retention."
//   "Do not lose conflict history."
//   "Keep semantic conflict history even after resolution for future
//   drift analysis."
// Acceptance: conflict history is never deleted.

/// A snapshot of a conflict at a point in its lifecycle, used for
/// history retention and drift analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConflictHistoryEntry {
    /// Unique history entry identifier.
    pub history_entry_id: String,
    /// The conflict this entry belongs to.
    pub conflict_id: String,
    /// The status at the time of this snapshot.
    pub status_at_snapshot: ConflictStatus,
    /// What triggered this history entry (status change, detail update, etc.).
    pub change_description: String,
    /// Full conflict record snapshot as JSON for auditability.
    pub snapshot: serde_json::Value,
    /// When this history entry was recorded.
    pub recorded_at: DateTime<Utc>,
}

// ── CNF-010: Conflict resolution projection ─────────────────────────────
//
// CSV guardrail: "Implement conflict resolution projection."
// Acceptance: resolution is explicit and projected into local state.

/// How a conflict was resolved.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStrategy {
    /// One side was chosen as the winner.
    PickWinner,
    /// Both sides were merged by a qualified reviewer.
    ManualMerge,
    /// Both sides were superseded by a new output.
    Supersede,
    /// The conflict was dismissed as a false positive.
    Dismiss,
}

/// Resolution record for a conflict.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConflictResolution {
    /// Unique resolution identifier.
    pub resolution_id: String,
    /// The conflict being resolved.
    pub conflict_id: String,
    /// How the conflict was resolved.
    pub strategy: ResolutionStrategy,
    /// The winning node ID (when strategy is PickWinner).
    pub winner_node_id: Option<String>,
    /// Rationale for the resolution decision.
    pub rationale: String,
    /// The adjudication task that produced this resolution (if any).
    pub adjudication_id: Option<String>,
    /// Who/what resolved the conflict (worker ID or "human").
    pub resolved_by: String,
    /// Node lifecycle effects applied as a result of the resolution.
    pub lifecycle_effects: Vec<ConflictResolutionEffect>,
    pub resolved_at: DateTime<Utc>,
}

/// A single lifecycle effect applied to a node as a result of conflict
/// resolution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConflictResolutionEffect {
    /// The node affected.
    pub node_id: String,
    /// The lane transition applied.
    pub lane_effect: String,
    /// The lifecycle transition applied.
    pub lifecycle_effect: String,
    /// Human-readable description of the effect.
    pub description: String,
}
