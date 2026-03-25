//! Stale certification invalidation types.
//!
//! This module provides the invalidation record and tracking types that
//! ensure stale certification results are never silently kept as valid.
//!
//! Key design rule: when any upstream change occurs that could affect a
//! previously certified output, the certification must be explicitly
//! invalidated and optionally resubmitted. Silent staleness is forbidden.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::certification::StaleReason;

/// A record of a certification result being invalidated due to staleness.
/// These records are never deleted so the system retains a full audit trail
/// of what was invalidated and why.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StaleInvalidationRecord {
    /// Unique invalidation record identifier.
    pub invalidation_id: String,
    /// The submission whose result was invalidated.
    pub submission_id: String,
    /// The candidate that was invalidated.
    pub candidate_id: String,
    /// Why the certification became stale.
    pub stale_reason: StaleReason,
    /// The specific change that triggered invalidation.
    pub triggering_change_ref: String,
    /// The node lifecycle state at the time of invalidation.
    pub lifecycle_at_invalidation: String,
    /// The lane at the time of invalidation.
    pub lane_at_invalidation: String,
    /// Whether the node was demoted to a less-trusted lane.
    pub lane_demoted: bool,
    /// Whether a revalidation was automatically triggered.
    pub revalidation_triggered: bool,
    /// When the invalidation occurred.
    pub invalidated_at: DateTime<Utc>,
}

/// Tracks the staleness state of a set of certified outputs for a given
/// node, providing a summary view for projections.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeStalenessStatus {
    /// The node being tracked.
    pub node_id: String,
    /// Total active (non-stale) certifications.
    pub active_certification_count: i32,
    /// Total invalidated (stale) certifications.
    pub stale_certification_count: i32,
    /// Total pending revalidations.
    pub pending_revalidation_count: i32,
    /// Whether the node is currently considered fully certified.
    pub is_fully_certified: bool,
    /// When this status was last computed.
    pub computed_at: DateTime<Utc>,
}
