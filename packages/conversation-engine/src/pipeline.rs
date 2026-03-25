// CONV-005 through CONV-010: Extraction pipeline traits
//
// These traits define the interface for AI-powered extraction
// and planning operations. Implementations will be provided by
// actual AI workers in later milestones.
//
// Design note: edition 2024 supports async fn in traits natively,
// so no async-trait crate is needed.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::chat::ChatMessage;
use crate::extract::{
    ConversationExtract, ExtractedConstraint, ExtractedDesignDecision, ExtractedOpenQuestion,
};

/// Error type for pipeline operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PipelineError {
    /// Machine-readable error code.
    pub code: String,
    /// Human-readable description.
    pub message: String,
}

impl fmt::Display for PipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for PipelineError {}

/// Summarize a sequence of chat messages into a
/// structured ConversationExtract.
///
/// This is the top-level entry point for the extraction pipeline.
/// Implementations may call the individual extractors (CONV-006
/// through CONV-008) internally or perform a single-pass extraction.
pub trait ConversationSummarizer {
    /// Produce a full extract from a chat message sequence.
    fn summarize(
        &self,
        session_id: &str,
        messages: &[ChatMessage],
    ) -> Result<ConversationExtract, PipelineError>;
}

/// Extract constraints from a conversation extract
/// or directly from messages.
pub trait ConstraintExtractor {
    /// Extract constraints from already-summarized conversation.
    fn extract_constraints(
        &self,
        extract: &ConversationExtract,
    ) -> Result<Vec<ExtractedConstraint>, PipelineError>;
}

/// Extract design decisions from a conversation extract
/// or directly from messages.
pub trait DecisionExtractor {
    /// Extract design decisions from already-summarized conversation.
    fn extract_decisions(
        &self,
        extract: &ConversationExtract,
    ) -> Result<Vec<ExtractedDesignDecision>, PipelineError>;
}

/// Extract open questions from a conversation extract
/// or directly from messages.
pub trait OpenQuestionExtractor {
    /// Extract open questions from already-summarized conversation.
    fn extract_open_questions(
        &self,
        extract: &ConversationExtract,
    ) -> Result<Vec<ExtractedOpenQuestion>, PipelineError>;
}

/// A single backlog item proposed from conversation analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BacklogDraftItem {
    /// Suggested title for the backlog item.
    pub title: String,
    /// Description of the work.
    pub description: String,
    /// Suggested priority (lower = higher priority).
    pub priority: i32,
    /// Decision IDs or constraint IDs this item addresses.
    pub source_refs: Vec<String>,
    /// Component or track this item belongs to.
    pub track: Option<String>,
}

/// A complete backlog draft generated from conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BacklogDraft {
    /// The extract this draft was generated from.
    pub source_extract_id: String,
    /// Proposed backlog items.
    pub items: Vec<BacklogDraftItem>,
    /// When this draft was generated.
    pub generated_at: DateTime<Utc>,
}

/// Generate a draft backlog from a conversation extract.
///
/// The draft is not automatically committed to the roadmap; it must
/// pass through roadmap absorption semantics (RMS pipeline) first.
pub trait BacklogDraftGenerator {
    /// Generate a backlog draft from a conversation extract.
    fn generate_backlog_draft(
        &self,
        extract: &ConversationExtract,
    ) -> Result<BacklogDraft, PipelineError>;
}

/// A proposed update to an existing plan based on conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanUpdateProposal {
    /// The plan being updated.
    pub plan_id: String,
    /// The extract driving this update.
    pub source_extract_id: String,
    /// Updated architecture summary (if changed).
    pub architecture_summary_delta: Option<String>,
    /// New constraints to add to the plan.
    pub new_constraints: Vec<ExtractedConstraint>,
    /// New decisions to record.
    pub new_decisions: Vec<ExtractedDesignDecision>,
    /// Questions that need resolution before the plan can advance.
    pub blocking_questions: Vec<ExtractedOpenQuestion>,
    /// Number of unresolved questions after this update.
    pub unresolved_question_count: i32,
    /// When this proposal was generated.
    pub proposed_at: DateTime<Utc>,
}

/// Propose plan updates from a conversation extract.
///
/// This does not directly mutate the plan; it produces a proposal
/// that must be validated and applied through the plan gate logic.
pub trait PlanUpdater {
    /// Given a conversation extract and target plan, produce an
    /// update proposal.
    fn propose_plan_update(
        &self,
        plan_id: &str,
        extract: &ConversationExtract,
    ) -> Result<PlanUpdateProposal, PipelineError>;
}
