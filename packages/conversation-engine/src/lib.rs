// conversation-engine: M2 Planning and Conversation Foundation
//
// This crate provides the data models and trait interfaces for:
// - Conversation extract schemas (CONV-001 to CONV-004)
// - Extraction pipeline traits (CONV-005 to CONV-010)
// - Chat session schemas and traits (CHAT-001 to CHAT-010)
//
// No AI implementation is included here; the traits define
// the contract that AI workers fulfill in later milestones.

pub mod chat;
pub mod extract;
pub mod pipeline;

// Re-export primary types for convenience.
pub use chat::{
    ChatMessage, ChatPanelModel, ChatPanelStatus, ChatPersistence, ChatProjection,
    ChatProvenance, ChatProvenanceCapture, ChatProjector, ChatSession, ChatStateUpdateRules,
    ChatToObjectiveHook, ChatToPlanHook, ChatToTaskHook, MessageRole, ObjectiveProposal,
    PlanProposal, ProjectionParams, StateUpdateKind, StateUpdateProposal, TaskProposal,
};
pub use extract::{
    BlockingStatus, ConstraintKind, ConversationExtract, EnforcementStatus,
    ExtractedConstraint, ExtractedDesignDecision, ExtractedOpenQuestion,
};
pub use pipeline::{
    BacklogDraft, BacklogDraftGenerator, BacklogDraftItem, ConstraintExtractor,
    ConversationSummarizer, DecisionExtractor, OpenQuestionExtractor, PipelineError,
    PlanUpdateProposal, PlanUpdater,
};
