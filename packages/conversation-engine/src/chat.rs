// CHAT-001 through CHAT-010: Chat schemas and traits
//
// These types mirror the DB schema in 0002_m1_complete.sql
// (chat_sessions, chat_messages tables) and define the traits
// for chat persistence, extraction hooks, and state integration.
//
// Caution (from CSV): Chat must not bypass control-plane state
// or certification boundaries. All state mutations go through
// proposals, never direct writes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::extract::ConversationExtract;
use crate::pipeline::PipelineError;

// ── CHAT-001: Chat session schema ────────────────────────────────────

/// CHAT-001: A chat session groups related messages together.
///
/// Maps to the `chat_sessions` table. A session may optionally
/// be linked to an objective for scoped conversations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatSession {
    /// Unique session identifier (maps to session_id column).
    pub session_id: String,
    /// Optional objective this session is scoped to.
    pub objective_id: Option<String>,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last updated.
    pub updated_at: DateTime<Utc>,
}

// ── CHAT-002: Chat message schema ────────────────────────────────────

/// The role of the message sender.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// Message from the human user.
    User,
    /// Message from the AI assistant/system.
    Assistant,
    /// System-level message (instructions, context).
    System,
}

/// CHAT-002: A single message within a chat session.
///
/// Maps to the `chat_messages` table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessage {
    /// Unique message identifier (maps to message_id column).
    pub message_id: String,
    /// Session this message belongs to.
    pub session_id: String,
    /// Who sent the message.
    pub role: MessageRole,
    /// The message content.
    pub content: String,
    /// When the message was created.
    pub created_at: DateTime<Utc>,
}

// ── CHAT-003: Chat-to-objective extraction hook ──────────────────────

/// A proposed objective derived from chat.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectiveProposal {
    /// Summary of the proposed objective.
    pub summary: String,
    /// Success metric if identified.
    pub success_metric: Option<String>,
    /// Message IDs that motivated this proposal.
    pub source_message_ids: Vec<String>,
}

/// CHAT-003: Hook for extracting objective proposals from chat.
///
/// Does not create objectives directly; produces proposals that
/// feed into the control-plane objective creation flow.
pub trait ChatToObjectiveHook {
    /// Analyze messages and propose objectives.
    fn extract_objectives(
        &self,
        session: &ChatSession,
        messages: &[ChatMessage],
    ) -> Result<Vec<ObjectiveProposal>, PipelineError>;
}

// ── CHAT-004: Chat-to-task extraction hook ───────────────────────────

/// A proposed task derived from chat.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskProposal {
    /// Description of the proposed task.
    pub description: String,
    /// Suggested worker role for this task.
    pub worker_role: Option<String>,
    /// Suggested skill pack for this task.
    pub skill_pack_id: Option<String>,
    /// Message IDs that motivated this proposal.
    pub source_message_ids: Vec<String>,
}

/// CHAT-004: Hook for extracting task proposals from chat.
///
/// Does not create tasks directly; produces proposals that
/// must go through decomposition and dispatch phases.
pub trait ChatToTaskHook {
    /// Analyze messages and propose tasks.
    fn extract_tasks(
        &self,
        session: &ChatSession,
        messages: &[ChatMessage],
    ) -> Result<Vec<TaskProposal>, PipelineError>;
}

// ── CHAT-005: Chat-to-plan extraction hook ───────────────────────────

/// A proposed plan element derived from chat.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanProposal {
    /// Architecture summary gleaned from conversation.
    pub architecture_summary: Option<String>,
    /// Milestone or phase suggestions.
    pub milestone_suggestions: Vec<String>,
    /// Message IDs that motivated this proposal.
    pub source_message_ids: Vec<String>,
}

/// CHAT-005: Hook for extracting plan proposals from chat.
///
/// Does not create plans directly; produces proposals that
/// feed into plan elaboration and validation phases.
pub trait ChatToPlanHook {
    /// Analyze messages and propose plan elements.
    fn extract_plan_elements(
        &self,
        session: &ChatSession,
        messages: &[ChatMessage],
    ) -> Result<Vec<PlanProposal>, PipelineError>;
}

// ── CHAT-006: Chat session persistence (trait) ───────────────────────

/// CHAT-006: Persistence interface for chat sessions and messages.
///
/// Implementations will provide the actual storage backend
/// (e.g., PostgreSQL via sqlx). This trait ensures the conversation
/// engine does not depend on a specific storage technology.
pub trait ChatPersistence {
    /// Store a new chat session.
    fn save_session(&self, session: &ChatSession) -> Result<(), PipelineError>;

    /// Store a new message within an existing session.
    fn save_message(&self, message: &ChatMessage) -> Result<(), PipelineError>;

    /// Load a session by ID.
    fn load_session(&self, session_id: &str) -> Result<Option<ChatSession>, PipelineError>;

    /// Load all messages for a session, ordered by created_at.
    fn load_messages(&self, session_id: &str) -> Result<Vec<ChatMessage>, PipelineError>;

    /// Store a conversation extract.
    fn save_extract(&self, extract: &ConversationExtract) -> Result<(), PipelineError>;

    /// Load extracts for a session.
    fn load_extracts(
        &self,
        session_id: &str,
    ) -> Result<Vec<ConversationExtract>, PipelineError>;
}

// ── CHAT-007: Chat history projection ────────────────────────────────

/// CHAT-007: A projected view of chat history.
///
/// Provides a read-optimized view of a session's messages,
/// including optional filtering and windowing for context
/// management in AI interactions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatProjection {
    /// The session this projection covers.
    pub session_id: String,
    /// The objective (if any) the session is scoped to.
    pub objective_id: Option<String>,
    /// Messages in chronological order.
    pub messages: Vec<ChatMessage>,
    /// Total message count in the session (may exceed messages.len()
    /// if a window was applied).
    pub total_message_count: usize,
    /// The extract most recently produced for this session, if any.
    pub latest_extract: Option<ConversationExtract>,
    /// When this projection was created.
    pub projected_at: DateTime<Utc>,
}

/// Parameters for building a chat projection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionParams {
    /// Maximum number of recent messages to include.
    pub max_messages: Option<usize>,
    /// Only include messages with these roles.
    pub role_filter: Option<Vec<MessageRole>>,
    /// Only include messages after this timestamp.
    pub after: Option<DateTime<Utc>>,
    /// Whether to include the latest extract.
    pub include_extract: bool,
}

impl Default for ProjectionParams {
    fn default() -> Self {
        Self {
            max_messages: None,
            role_filter: None,
            after: None,
            include_extract: true,
        }
    }
}

/// Trait for building chat projections from persisted data.
pub trait ChatProjector {
    /// Build a projection for the given session with the specified params.
    fn project(
        &self,
        session_id: &str,
        params: &ProjectionParams,
    ) -> Result<ChatProjection, PipelineError>;
}

// ── CHAT-008: Chat-to-state update rules ─────────────────────────────

/// The kind of state mutation a chat update rule proposes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StateUpdateKind {
    /// Create or update an objective.
    Objective,
    /// Create or update a plan.
    Plan,
    /// Create or update a task.
    Task,
    /// Add a roadmap node.
    RoadmapNode,
    /// Record a constraint.
    Constraint,
    /// Record a decision.
    Decision,
}

/// A proposed state update derived from chat.
///
/// All proposed updates carry provenance and must be validated
/// before application to the control-plane state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateUpdateProposal {
    /// What kind of state entity to update.
    pub kind: StateUpdateKind,
    /// The proposed payload (type depends on kind).
    pub payload: serde_json::Value,
    /// Source extract or message IDs for provenance.
    pub source_refs: Vec<String>,
    /// Whether this update requires human approval.
    pub requires_approval: bool,
}

/// CHAT-008: Rules engine for determining which state updates
/// should be proposed from a conversation extract.
///
/// Caution: implementations must not bypass control-plane state
/// or certification boundaries.
pub trait ChatStateUpdateRules {
    /// Given an extract, determine what state updates to propose.
    fn evaluate(
        &self,
        extract: &ConversationExtract,
    ) -> Result<Vec<StateUpdateProposal>, PipelineError>;
}

// ── CHAT-009: Chat provenance capture ────────────────────────────────

/// CHAT-009: Provenance record linking a state change back to
/// the chat interaction that caused it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatProvenance {
    /// Unique provenance record ID.
    pub provenance_id: String,
    /// The chat session that originated the change.
    pub session_id: String,
    /// Specific message IDs that triggered the change.
    pub message_ids: Vec<String>,
    /// The extract ID (if the change went through extraction).
    pub extract_id: Option<String>,
    /// The aggregate kind that was affected (e.g., "objective", "plan").
    pub target_aggregate_kind: String,
    /// The aggregate ID that was affected.
    pub target_aggregate_id: String,
    /// When this provenance was recorded.
    pub recorded_at: DateTime<Utc>,
}

/// Trait for recording provenance of chat-driven state changes.
pub trait ChatProvenanceCapture {
    /// Record that a state change was driven by chat interaction.
    fn record_provenance(
        &self,
        provenance: &ChatProvenance,
    ) -> Result<(), PipelineError>;

    /// Look up provenance records for a given state entity.
    fn lookup_provenance(
        &self,
        aggregate_kind: &str,
        aggregate_id: &str,
    ) -> Result<Vec<ChatProvenance>, PipelineError>;
}

// ── CHAT-010: UI chat panel data model ───────────────────────────────

/// The status of a chat panel in the UI.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatPanelStatus {
    /// Panel is idle, waiting for user input.
    Idle,
    /// AI is processing / generating a response.
    Processing,
    /// Extraction pipeline is running on the conversation.
    Extracting,
    /// An error occurred.
    Error,
}

/// CHAT-010: Data model for a UI chat panel.
///
/// This is the data-only model; no actual UI rendering is included.
/// UI layers consume this model to display chat state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatPanelModel {
    /// The underlying session.
    pub session: ChatSession,
    /// Current panel status.
    pub status: ChatPanelStatus,
    /// The projected message history for display.
    pub projection: ChatProjection,
    /// Pending state update proposals (if any) awaiting user action.
    pub pending_proposals: Vec<StateUpdateProposal>,
    /// Error message if status is Error.
    pub error_message: Option<String>,
}
