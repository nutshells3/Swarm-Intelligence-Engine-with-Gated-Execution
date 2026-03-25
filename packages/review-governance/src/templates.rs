//! Review worker templates and UI/storage projections.
//!
//! Each review kind has an explicit worker template that defines what the
//! review worker receives, what it must produce, and how results are
//! integrated.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::artifacts::{ReviewKind, ReviewOutcome};

/// A review worker template defining the inputs, expected outputs, and
/// integration rules for a specific review kind.
///
/// Templates for REV-007 (Planning), REV-008 (Architecture),
/// REV-009 (Direction), and REV-010 (Milestone) all share this schema
/// but differ in their `review_kind`, `required_context_refs`, and
/// `expected_output_sections`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewWorkerTemplate {
    /// Unique template identifier.
    pub template_id: String,
    /// Which review kind this template serves.
    pub review_kind: ReviewKind,
    /// The worker role required to execute this review.
    pub required_worker_role: String,
    /// The skill pack ID the worker needs.
    pub skill_pack_id: String,
    /// Context references the review worker receives (bounded, not a
    /// full-project dump).
    pub required_context_refs: Vec<String>,
    /// Sections the review worker must produce in its output.
    pub expected_output_sections: Vec<String>,
    /// Maximum input tokens for context assembly.
    pub max_input_tokens: u32,
    /// Maximum output tokens the reviewer may produce.
    pub max_output_tokens: u32,
    /// Instructions for how the review result integrates with local state.
    pub integration_instructions: String,
    /// Default interval in minutes between periodic reviews of this kind.
    pub default_interval_minutes: u32,
    /// Whether this review kind is eligible for auto-approval.
    pub auto_approval_eligible: bool,
    pub created_at: DateTime<Utc>,
}

/// Create the planning-review worker template.
///
/// Reviews the planning phase: objective clarity, milestone definitions,
/// dependency correctness, and resource allocation.
pub fn planning_review_template() -> ReviewWorkerTemplate {
    ReviewWorkerTemplate {
        template_id: "tpl-planning-review-v1".to_string(),
        review_kind: ReviewKind::Planning,
        required_worker_role: "review-worker".to_string(),
        skill_pack_id: "sp-planning-review".to_string(),
        required_context_refs: vec![
            "objective_summary".to_string(),
            "milestone_definitions".to_string(),
            "dependency_graph".to_string(),
            "resource_allocation".to_string(),
        ],
        expected_output_sections: vec![
            "objective_clarity".to_string(),
            "milestone_feasibility".to_string(),
            "dependency_completeness".to_string(),
            "risk_assessment".to_string(),
            "recommendation".to_string(),
        ],
        max_input_tokens: 16_000,
        max_output_tokens: 4_000,
        integration_instructions: "Update plan gate review_satisfied. \
            If rejected, transition cycle to plan_elaboration phase."
            .to_string(),
        default_interval_minutes: 60,
        auto_approval_eligible: false,
        created_at: Utc::now(),
    }
}

/// Create the architecture-review worker template.
///
/// Reviews architecture drafts: structural soundness, abstraction boundaries,
/// contract coverage, and scaling considerations.
pub fn architecture_review_template() -> ReviewWorkerTemplate {
    ReviewWorkerTemplate {
        template_id: "tpl-architecture-review-v1".to_string(),
        review_kind: ReviewKind::Architecture,
        required_worker_role: "review-worker".to_string(),
        skill_pack_id: "sp-architecture-review".to_string(),
        required_context_refs: vec![
            "architecture_draft".to_string(),
            "module_boundaries".to_string(),
            "contract_definitions".to_string(),
            "integration_points".to_string(),
        ],
        expected_output_sections: vec![
            "structural_soundness".to_string(),
            "abstraction_quality".to_string(),
            "contract_coverage".to_string(),
            "scaling_assessment".to_string(),
            "recommendation".to_string(),
        ],
        max_input_tokens: 24_000,
        max_output_tokens: 6_000,
        integration_instructions: "Update architecture gate. \
            If rejected, transition to architecture_revision phase."
            .to_string(),
        default_interval_minutes: 120,
        auto_approval_eligible: false,
        created_at: Utc::now(),
    }
}

/// Create the direction-review worker template.
///
/// Reviews development direction and strategic choices: alignment with
/// objectives, trade-off justifications, and pivot criteria.
pub fn direction_review_template() -> ReviewWorkerTemplate {
    ReviewWorkerTemplate {
        template_id: "tpl-direction-review-v1".to_string(),
        review_kind: ReviewKind::Direction,
        required_worker_role: "review-worker".to_string(),
        skill_pack_id: "sp-direction-review".to_string(),
        required_context_refs: vec![
            "objective_summary".to_string(),
            "strategic_choices".to_string(),
            "trade_off_log".to_string(),
            "pivot_criteria".to_string(),
        ],
        expected_output_sections: vec![
            "objective_alignment".to_string(),
            "trade_off_justification".to_string(),
            "pivot_readiness".to_string(),
            "recommendation".to_string(),
        ],
        max_input_tokens: 12_000,
        max_output_tokens: 4_000,
        integration_instructions: "Update direction gate. \
            If rejected, flag for human escalation."
            .to_string(),
        default_interval_minutes: 180,
        auto_approval_eligible: true,
        created_at: Utc::now(),
    }
}

/// Create the milestone-review worker template.
///
/// Reviews a specific milestone's deliverables: completeness, acceptance
/// criteria satisfaction, and quality thresholds.
pub fn milestone_review_template() -> ReviewWorkerTemplate {
    ReviewWorkerTemplate {
        template_id: "tpl-milestone-review-v1".to_string(),
        review_kind: ReviewKind::Milestone,
        required_worker_role: "review-worker".to_string(),
        skill_pack_id: "sp-milestone-review".to_string(),
        required_context_refs: vec![
            "milestone_definition".to_string(),
            "deliverable_list".to_string(),
            "acceptance_criteria".to_string(),
            "task_results".to_string(),
        ],
        expected_output_sections: vec![
            "deliverable_completeness".to_string(),
            "criteria_satisfaction".to_string(),
            "quality_assessment".to_string(),
            "recommendation".to_string(),
        ],
        max_input_tokens: 20_000,
        max_output_tokens: 5_000,
        integration_instructions: "Update milestone gate review_satisfied. \
            If approved, allow milestone to transition to completed."
            .to_string(),
        default_interval_minutes: 60,
        auto_approval_eligible: true,
        created_at: Utc::now(),
    }
}

/// Look up the canonical template for a given review kind.
pub fn template_for_kind(kind: ReviewKind) -> ReviewWorkerTemplate {
    match kind {
        ReviewKind::Planning => planning_review_template(),
        ReviewKind::Architecture => architecture_review_template(),
        ReviewKind::Direction => direction_review_template(),
        ReviewKind::Milestone => milestone_review_template(),
        ReviewKind::Implementation => {
            // Implementation reviews reuse the milestone template with
            // adjusted context refs. A dedicated template can be added
            // when REV items for implementation review are defined.
            let mut t = milestone_review_template();
            t.template_id = "tpl-implementation-review-v1".to_string();
            t.review_kind = ReviewKind::Implementation;
            t.skill_pack_id = "sp-implementation-review".to_string();
            t.required_context_refs = vec![
                "code_diff".to_string(),
                "test_results".to_string(),
                "contract_checks".to_string(),
            ];
            t
        }
    }
}

/// Storage metadata for a review artifact, tracking where it is persisted
/// and how it can be retrieved.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewStorageRecord {
    /// The review artifact this record tracks.
    pub review_id: String,
    /// Storage backend used (e.g., "postgres", "s3").
    pub storage_backend: String,
    /// Storage key/path for retrieval.
    pub storage_key: String,
    /// Content hash for integrity verification.
    pub content_hash: String,
    /// Size in bytes.
    pub size_bytes: i64,
    /// When the review was stored.
    pub stored_at: DateTime<Utc>,
}

/// A single entry in the review queue, projected for UI consumption.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewQueueEntry {
    /// The review artifact.
    pub review_id: String,
    /// Review kind.
    pub review_kind: ReviewKind,
    /// Target entity being reviewed.
    pub target_ref: String,
    /// Current review status.
    pub status: crate::artifacts::ReviewStatus,
    /// Assigned reviewer (if any).
    pub assigned_reviewer: Option<String>,
    /// Whether this review is overdue.
    pub is_overdue: bool,
    /// Priority (lower = higher priority).
    pub priority: i32,
    /// When the review was scheduled.
    pub scheduled_at: DateTime<Utc>,
}

/// Projection for a review page. The `page_kind` determines which
/// review-specific data is included.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewPageProjection {
    /// Which review kind this page displays.
    pub page_kind: ReviewKind,
    /// The review artifact being displayed.
    pub review_id: String,
    /// Target entity identifier.
    pub target_ref: String,
    /// Findings summary (the human-readable digest).
    pub findings_summary: String,
    /// Conditions (for ApprovedWithConditions outcomes).
    pub conditions: Vec<String>,
    /// Review outcome.
    pub outcome: Option<crate::artifacts::ReviewOutcome>,
    /// Historical review count for this target.
    pub historical_review_count: i32,
    /// Related review IDs (previous reviews of the same target).
    pub related_review_ids: Vec<String>,
}

/// A human-readable digest summarizing accumulated reviews for a target
/// entity. This is the key artifact that prevents humans from needing
/// to replay full context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HumanDigestSummary {
    /// The target entity this digest covers.
    pub target_ref: String,
    /// The kind of target entity.
    pub target_kind: String,
    /// Total number of reviews summarized.
    pub total_reviews: i32,
    /// Number of approvals.
    pub approval_count: i32,
    /// Number of rejections.
    pub rejection_count: i32,
    /// Number of inconclusive reviews.
    pub inconclusive_count: i32,
    /// Consolidated findings summary across all reviews.
    pub consolidated_summary: String,
    /// Outstanding conditions from ApprovedWithConditions outcomes.
    pub outstanding_conditions: Vec<String>,
    /// Key decisions made across reviews.
    pub key_decisions: Vec<String>,
    /// When this digest was generated.
    pub generated_at: DateTime<Utc>,
}

/// A lightweight summary of a single review artifact, used as input to
/// `generate_review_digest`. Callers construct these from
/// `ReviewArtifactRecord` or from SQL query results.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewArtifactSummary {
    /// The review artifact identifier.
    pub review_id: String,
    /// Which review kind this was.
    pub review_kind: ReviewKind,
    /// Target entity that was reviewed.
    pub target_ref: String,
    /// Outcome of the review (None if still in progress).
    pub outcome: Option<ReviewOutcome>,
    /// Human-readable findings summary.
    pub findings_summary: String,
    /// Conditions attached to the outcome.
    pub conditions: Vec<String>,
    /// When the review was created.
    pub created_at: DateTime<Utc>,
}

/// Generate a human-readable digest from multiple review artifacts.
///
/// Consolidates findings, tallies outcomes, and collects outstanding
/// conditions so that a human can understand the review history without
/// replaying full context.
pub fn generate_review_digest(reviews: &[ReviewArtifactSummary]) -> String {
    if reviews.is_empty() {
        return "No reviews to summarize.".to_string();
    }

    let total = reviews.len();
    let mut approved = 0u32;
    let mut approved_with_conditions = 0u32;
    let mut rejected = 0u32;
    let mut inconclusive = 0u32;
    let mut pending = 0u32;
    let mut outstanding_conditions: Vec<String> = Vec::new();
    let mut finding_lines: Vec<String> = Vec::new();

    for r in reviews {
        match r.outcome {
            Some(ReviewOutcome::Approved) => approved += 1,
            Some(ReviewOutcome::ApprovedWithConditions) => {
                approved_with_conditions += 1;
                for c in &r.conditions {
                    outstanding_conditions.push(format!("[{}] {}", r.review_id, c));
                }
            }
            Some(ReviewOutcome::Rejected) => rejected += 1,
            Some(ReviewOutcome::Inconclusive) => inconclusive += 1,
            None => pending += 1,
        }
        if !r.findings_summary.is_empty() {
            finding_lines.push(format!(
                "- [{}] ({:?}) {}",
                r.review_id, r.review_kind, r.findings_summary
            ));
        }
    }

    let mut digest = String::with_capacity(1024);
    digest.push_str(&format!("Review Digest ({} reviews)\n", total));
    digest.push_str(&format!(
        "Outcomes: {} approved, {} approved-with-conditions, {} rejected, {} inconclusive, {} pending\n",
        approved, approved_with_conditions, rejected, inconclusive, pending
    ));

    if !outstanding_conditions.is_empty() {
        digest.push_str("\nOutstanding conditions:\n");
        for c in &outstanding_conditions {
            digest.push_str(&format!("  {}\n", c));
        }
    }

    if !finding_lines.is_empty() {
        digest.push_str("\nFindings:\n");
        for f in &finding_lines {
            digest.push_str(&format!("  {}\n", f));
        }
    }

    digest
}

/// Describes how a review outcome affects the plan gate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewPlanGateEffect {
    /// The review that produced this effect.
    pub review_id: String,
    /// The plan gate affected.
    pub plan_gate_id: String,
    /// Which gate condition is affected.
    pub gate_condition: String,
    /// The effect on the condition (pass, fail, no_change).
    pub condition_effect: String,
    /// Human-readable justification for the effect.
    pub justification: String,
    /// When this effect was applied.
    pub applied_at: DateTime<Utc>,
}
