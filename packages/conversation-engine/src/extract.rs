// CONV-001 through CONV-004: Conversation extract schemas
//
// These types mirror the DB schema in 0002_m1_complete.sql
// (conversation_extracts table) and provide typed Rust models
// for the JSON fields stored in extracted_constraints,
// extracted_decisions, and extracted_open_questions columns.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── CONV-002: Extracted constraint schema ────────────────────────────

/// The kind of constraint extracted from conversation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintKind {
    /// Hard requirement that must be satisfied.
    Requirement,
    /// Performance or resource boundary.
    ResourceBound,
    /// Compatibility constraint with external system.
    Compatibility,
    /// Regulatory or policy constraint.
    Regulatory,
    /// User-stated preference (softer than requirement).
    Preference,
}

/// Whether the constraint is currently being enforced.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementStatus {
    /// Not yet checked or enforced.
    Pending,
    /// Actively enforced in the current plan.
    Active,
    /// Verified as satisfied.
    Satisfied,
    /// Known to be violated; needs resolution.
    Violated,
    /// Deferred to a later milestone.
    Deferred,
}

/// CONV-002: A single constraint extracted from design conversation.
///
/// Typed with source references back to the originating message(s)
/// and an enforcement status tracking whether the constraint is
/// reflected in the current plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtractedConstraint {
    /// Unique identifier for this constraint.
    pub constraint_id: String,
    /// Human-readable statement of the constraint.
    pub statement: String,
    /// Classification of the constraint.
    pub kind: ConstraintKind,
    /// Message IDs from which this constraint was derived.
    pub source_message_ids: Vec<String>,
    /// Current enforcement status.
    pub enforcement_status: EnforcementStatus,
    /// When this constraint was extracted.
    pub extracted_at: DateTime<Utc>,
}

// ── CONV-003: Extracted design-decision schema ───────────────────────

/// CONV-003: A design decision captured from conversation.
///
/// Records what was decided, why, and which components are affected
/// so downstream planning can reference the rationale.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtractedDesignDecision {
    /// Unique identifier for this decision.
    pub decision_id: String,
    /// Concise statement of the decision.
    pub decision: String,
    /// Why this decision was made (alternatives considered, trade-offs).
    pub rationale: String,
    /// Component or module names affected by this decision.
    pub affected_components: Vec<String>,
    /// Message IDs from which this decision was derived.
    pub source_message_ids: Vec<String>,
    /// When this decision was extracted.
    pub extracted_at: DateTime<Utc>,
}

// ── CONV-004: Extracted open-question schema ─────────────────────────

/// Whether an open question blocks progress.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlockingStatus {
    /// Does not block any current work.
    NonBlocking,
    /// Blocks one or more tasks or decisions.
    Blocking,
    /// Was blocking but has been resolved.
    Resolved,
}

/// CONV-004: An open question extracted from conversation.
///
/// Tracks unresolved questions that may block planning or execution,
/// along with a suggested resolution path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtractedOpenQuestion {
    /// Unique identifier for this question.
    pub question_id: String,
    /// The question text.
    pub question: String,
    /// Whether this question blocks progress.
    pub blocking_status: BlockingStatus,
    /// Suggested path to resolution (e.g., "ask user", "research", "prototype").
    pub resolution_path: Option<String>,
    /// Message IDs from which this question was derived.
    pub source_message_ids: Vec<String>,
    /// When this question was extracted.
    pub extracted_at: DateTime<Utc>,
}

// ── CONV-001: Conversation extract schema ────────────────────────────

/// CONV-001: Full conversation extract.
///
/// Aggregates the summarized intent of a conversation along with
/// all extracted constraints, design decisions, and open questions.
/// Maps to the `conversation_extracts` table in the database.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationExtract {
    /// Unique identifier for this extract (maps to extract_id column).
    pub extract_id: String,
    /// The chat session this extract was derived from.
    pub session_id: String,
    /// High-level summary of what the conversation was trying to achieve.
    pub summarized_intent: String,
    /// All constraints identified in the conversation (CONV-002).
    pub extracted_constraints: Vec<ExtractedConstraint>,
    /// All design decisions captured (CONV-003).
    pub extracted_decisions: Vec<ExtractedDesignDecision>,
    /// All open questions identified (CONV-004).
    pub extracted_open_questions: Vec<ExtractedOpenQuestion>,
    /// When this extract was created.
    pub created_at: DateTime<Utc>,
}
