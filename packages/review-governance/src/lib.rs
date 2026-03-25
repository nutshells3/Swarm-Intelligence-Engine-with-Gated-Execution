//! Review governance.
//!
//! This crate provides the types and schemas for scheduling and recording
//! review artifacts so humans do not need to replay full context.
//!
//! Key design rules:
//! - Do not silently auto-approve without leaving a durable review artifact.
//! - Every review produces an inspectable, persistent record.
//! - Review scheduling is policy-driven, not ad hoc.
//! - Human digest summaries consolidate accumulated reviews.

pub mod artifacts;
pub mod scheduling;
pub mod templates;

// Re-export primary types for ergonomic imports.
pub use artifacts::{ReviewArtifactRecord, ReviewKind, ReviewOutcome, ReviewStatus};
pub use scheduling::{
    AutoApprovalDecision, AutoApprovalThreshold, HeartbeatReviewTrigger, ReviewResultIngestion,
    ReviewSchedulerSnapshot, ReviewSchedulingPolicy, ReviewTriggerKind,
};
pub use templates::{
    HumanDigestSummary, ReviewArtifactSummary, ReviewPageProjection, ReviewPlanGateEffect,
    ReviewQueueEntry, ReviewStorageRecord, ReviewWorkerTemplate,
    architecture_review_template, direction_review_template, generate_review_digest,
    milestone_review_template, planning_review_template, template_for_kind,
};
