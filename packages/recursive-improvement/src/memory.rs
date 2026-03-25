//! REC-010: Recursive roadmap memory.
//!
//! CSV guardrail: "Implement recursive roadmap memory (memory schema:
//!   prior objectives, outcomes, supersessions, reuse recommendations)."
//! Caution: "Do not let recursive memory override current objectives."
//! auto_approval_policy: never_silent
//!
//! Acceptance: memory is append-only and must not override current
//! objectives.  Prior outcomes, supersessions, and reuse recommendations
//! are recorded for learning without corrupting active state.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single entry in the recursive memory store.
/// Memory is append-only (CSV caution: must not override current objectives).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryEntry {
    /// Unique entry identifier.
    pub entry_id: String,
    /// The objective this entry records.
    pub objective_id: String,
    /// Outcome of the objective: "completed", "abandoned", "superseded",
    /// "rolled_back", "in_progress".
    pub outcome: String,
    /// Summary of what was learned.
    pub learned_summary: String,
    /// Key metrics at completion (structured JSON).
    pub outcome_metrics: serde_json::Value,
    /// Whether this entry supersedes a previous entry.
    pub supersedes_entry_id: Option<String>,
    /// When the entry was recorded.
    pub recorded_at: DateTime<Utc>,
}

/// A chain of supersessions linking objectives over time.
/// Allows tracing the evolution of a self-improvement direction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupersessionChain {
    /// Root objective that started the chain.
    pub root_objective_id: String,
    /// Ordered list of objective IDs in the chain (oldest first).
    pub chain: Vec<String>,
    /// Current head of the chain (the latest active objective, if any).
    pub current_head: Option<String>,
    /// Total number of supersessions in the chain.
    pub supersession_count: i32,
}

/// A reuse recommendation derived from memory.
/// Suggests whether a prior approach should be reused, adapted, or
/// avoided for a new objective.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReuseSignal {
    /// The memory entry this signal derives from.
    pub source_entry_id: String,
    /// The objective this signal applies to (the new objective).
    pub target_objective_id: String,
    /// Recommendation: "reuse", "adapt", "avoid".
    pub recommendation: String,
    /// Human-readable rationale.
    pub rationale: String,
    /// Confidence level: "high", "medium", "low".
    pub confidence: String,
}

/// A learning reinjection record: distilled lessons from a completed cycle
/// that should be fed into the next cycle's context window.  This enables
/// cross-cycle boundary learning without overriding current objectives.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LearningReinjection {
    /// The memory entry this reinjection derives from.
    pub source_entry_id: String,
    /// The target cycle or objective to reinject into.
    pub target_objective_id: String,
    /// Distilled lesson text (must be concise -- not full history replay).
    pub lesson: String,
    /// Whether this reinjection has been consumed by the target cycle.
    pub consumed: bool,
    /// When this reinjection was created.
    pub created_at: DateTime<Utc>,
}

/// Recursive memory.
///
/// Append-only memory store for recursive improvement loops.  Records
/// prior objectives, outcomes, supersession chains, reuse signals, and
/// cross-cycle learning reinjections.
/// Must not override or corrupt current objectives (CSV caution).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecursiveMemory {
    /// Unique memory store identifier (typically one per workspace).
    pub memory_id: String,
    /// All memory entries (append-only).
    pub entries: Vec<MemoryEntry>,
    /// Supersession chains derived from entries.
    pub supersession_chains: Vec<SupersessionChain>,
    /// Active reuse signals for current objectives.
    pub reuse_signals: Vec<ReuseSignal>,
    /// Cross-cycle learning reinjections for feeding lessons into new cycles.
    pub reinjections: Vec<LearningReinjection>,
    /// Whether this memory store is append-only (always true -- CSV constraint).
    pub append_only: bool,
    /// When the memory was last updated.
    pub last_updated_at: DateTime<Utc>,
}
