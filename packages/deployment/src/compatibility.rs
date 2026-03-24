//! DEP-005: Migration compatibility policy.
//!
//! CSV guardrail: "Define migration compatibility policy."
//! Acceptance: schema validation; mode-compatibility check.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Whether a migration between two schema versions is compatible.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityStatus {
    /// Fully compatible; migration is safe and automatic.
    Compatible,
    /// Compatible with caveats (e.g. new nullable columns).
    CompatibleWithCaveats,
    /// Incompatible; requires manual migration steps.
    Incompatible,
    /// Compatibility has not been assessed yet.
    Unassessed,
}

/// A range of schema versions, used to express which versions a
/// migration path supports.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaVersionRange {
    /// Inclusive lower bound of the version range.
    pub from_version: String,
    /// Inclusive upper bound of the version range.
    pub to_version: String,
}

/// A single migration compatibility record, expressing whether
/// upgrading from one version range to another is safe.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationCompatibilityRecord {
    pub record_id: String,
    /// The source version range.
    pub source_range: SchemaVersionRange,
    /// The target version range.
    pub target_range: SchemaVersionRange,
    /// Assessed compatibility status.
    pub status: CompatibilityStatus,
    /// Human-readable notes on caveats or incompatibilities.
    pub notes: Option<String>,
    pub assessed_at: DateTime<Utc>,
}

/// Policy governing how migration compatibility is enforced.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationCompatibilityPolicy {
    /// Whether to block deployment when compatibility is Unassessed.
    pub block_on_unassessed: bool,
    /// Whether to block deployment when compatibility is
    /// CompatibleWithCaveats.
    pub block_on_caveats: bool,
    /// Whether to require a preflight check (DEP-011) before migration.
    pub require_preflight: bool,
}
