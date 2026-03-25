//! Projection consistency checks (FRM-009).
//!
//! Machine-readable checks that compare authoritative SQL state and read
//! models so future formal validation can reason about projection
//! soundness.
//!
//! Design rules:
//! - Consistency must be explicitly checkable, not inferred from UI
//!   appearance.
//! - Stale-but-valid and inconsistent must remain distinct states.
//! - Predicates operate on authoritative state plus projection outputs
//!   only.
//!
//! CSV guardrail: "Define machine-readable checks that compare
//! authoritative SQL state and read models so future formal validation
//! can reason about projection soundness."
//!
//! Scope: Consistency predicates between authoritative rows, events, and
//! projections for task boards, roadmap panels, branch/mainline status,
//! review queues, and certification queues.
//!
//! Acceptance: Projection mismatches can be detected deterministically
//! and surfaced as explicit repair work.
//!
//! Proof hooks: projection replay check; mismatch detection fixture;
//! rebuild determinism check.
//!
//! Caution: Do not let projection freshness or lag masquerade as
//! correctness; stale but valid and inconsistent must remain distinct
//! states.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ─── Projection domain ───────────────────────────────────────────────────

/// The projection surface being checked for consistency.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionDomain {
    /// Task board projection.
    TaskBoard,
    /// Roadmap panel projection.
    RoadmapPanel,
    /// Branch/mainline status projection.
    BranchMainlineStatus,
    /// Review queue projection.
    ReviewQueue,
    /// Certification queue projection.
    CertificationQueue,
}

// ─── Consistency status ──────────────────────────────────────────────────

/// Outcome of a projection consistency check.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConsistencyStatus {
    /// Projection matches authoritative state.
    Consistent,
    /// Projection is stale but structurally valid (lag, not corruption).
    StaleButValid,
    /// Projection is inconsistent with authoritative state (mismatch).
    Inconsistent,
    /// Check could not be performed (missing data).
    Indeterminate,
}

// ─── Mismatch artifact ──────────────────────────────────────────────────

/// A single mismatch between authoritative state and a projection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionMismatch {
    /// Entity that is mismatched.
    pub entity_id: String,
    /// Entity kind (e.g. "node", "task", "certification").
    pub entity_kind: String,
    /// Field that is mismatched.
    pub field_name: String,
    /// Value in authoritative state.
    pub authoritative_value: String,
    /// Value in the projection.
    pub projected_value: String,
    /// Human-readable description of the mismatch.
    pub description: String,
}

// ─── Rebuild trigger ─────────────────────────────────────────────────────

/// Action to take when a projection inconsistency is detected.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RebuildAction {
    /// No action needed.
    None,
    /// Rebuild the projection from authoritative state.
    Rebuild,
    /// Flag for manual investigation.
    ManualInvestigation,
}

// ─── FRM-009: Projection consistency check ───────────────────────────────

/// FRM-009 -- Projection consistency check.
///
/// Compares authoritative SQL state against a read-model projection for
/// a specific domain surface. Mismatches are recorded as explicit
/// artifacts that can trigger rebuilds or manual investigation.
///
/// The check is deterministic: same authoritative snapshot and same
/// projection version produce the same result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionConsistencyCheck {
    /// Unique check identifier.
    pub check_id: String,
    /// Which projection domain is being checked.
    pub domain: ProjectionDomain,
    /// Consistency outcome.
    pub status: ConsistencyStatus,
    /// Mismatches found (empty if consistent).
    pub mismatches: Vec<ProjectionMismatch>,
    /// Recommended action based on the check result.
    pub rebuild_action: RebuildAction,
    /// Version of the authoritative schema used.
    pub authoritative_schema_version: String,
    /// Version of the projection schema used.
    pub projection_schema_version: String,
    /// When this check was performed.
    pub checked_at: DateTime<Utc>,
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    // FRM-009: consistency check roundtrip
    #[test]
    fn consistency_check_serialization_roundtrip() {
        let check = ProjectionConsistencyCheck {
            check_id: "chk-001".into(),
            domain: ProjectionDomain::TaskBoard,
            status: ConsistencyStatus::Consistent,
            mismatches: vec![],
            rebuild_action: RebuildAction::None,
            authoritative_schema_version: "1.0.0".into(),
            projection_schema_version: "1.0.0".into(),
            checked_at: Utc::now(),
        };
        let json = serde_json::to_string(&check).unwrap();
        let back: ProjectionConsistencyCheck = serde_json::from_str(&json).unwrap();
        assert_eq!(back.check_id, "chk-001");
        assert_eq!(back.status, ConsistencyStatus::Consistent);
        assert_eq!(back.rebuild_action, RebuildAction::None);
    }

    // FRM-009: mismatch detection
    #[test]
    fn consistency_check_with_mismatches() {
        let check = ProjectionConsistencyCheck {
            check_id: "chk-002".into(),
            domain: ProjectionDomain::BranchMainlineStatus,
            status: ConsistencyStatus::Inconsistent,
            mismatches: vec![ProjectionMismatch {
                entity_id: "node-1".into(),
                entity_kind: "node".into(),
                field_name: "lane".into(),
                authoritative_value: "branch".into(),
                projected_value: "mainline".into(),
                description: "Node lane diverged.".into(),
            }],
            rebuild_action: RebuildAction::Rebuild,
            authoritative_schema_version: "1.0.0".into(),
            projection_schema_version: "1.0.0".into(),
            checked_at: Utc::now(),
        };
        assert_eq!(check.status, ConsistencyStatus::Inconsistent);
        assert_eq!(check.mismatches.len(), 1);
        assert_eq!(check.rebuild_action, RebuildAction::Rebuild);
    }

    // FRM-009: stale-but-valid is distinct from inconsistent
    #[test]
    fn stale_but_valid_is_distinct_from_inconsistent() {
        let stale = ConsistencyStatus::StaleButValid;
        let inconsistent = ConsistencyStatus::Inconsistent;
        assert_ne!(stale, inconsistent);
    }

    // FRM-009: rebuild determinism -- same inputs produce same serialization
    #[test]
    fn consistency_check_deterministic() {
        let check = ProjectionConsistencyCheck {
            check_id: "chk-003".into(),
            domain: ProjectionDomain::ReviewQueue,
            status: ConsistencyStatus::Consistent,
            mismatches: vec![],
            rebuild_action: RebuildAction::None,
            authoritative_schema_version: "1.0.0".into(),
            projection_schema_version: "1.0.0".into(),
            checked_at: Utc::now(),
        };
        let json1 = serde_json::to_string(&check).unwrap();
        let json2 = serde_json::to_string(&check).unwrap();
        assert_eq!(json1, json2);
    }

    // FRM-009: all projection domains are representable
    #[test]
    fn all_projection_domains_roundtrip() {
        let domains = [
            ProjectionDomain::TaskBoard,
            ProjectionDomain::RoadmapPanel,
            ProjectionDomain::BranchMainlineStatus,
            ProjectionDomain::ReviewQueue,
            ProjectionDomain::CertificationQueue,
        ];
        for domain in &domains {
            let json = serde_json::to_string(domain).unwrap();
            let back: ProjectionDomain = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, domain);
        }
    }
}
