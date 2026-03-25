//! ADT-008: Provenance capture for adapter invocations.
//!
//! CSV guardrail: "durable stdio capture check"
//! Every adapter invocation creates a durable provenance record capturing
//! the full I/O exchange, timing, and outcome. This ensures auditability
//! and supports empty-output retry simulation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::adapter::AgentKind;
#[cfg(feature = "persistence")]
use crate::adapter::{AdapterResponse, AdapterStatus};

/// Outcome classification for a provenance record.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InvocationOutcome {
    /// Invocation completed successfully with output.
    Success,
    /// Invocation completed but returned empty output.
    EmptyOutput,
    /// Invocation timed out.
    Timeout,
    /// Invocation failed with an error.
    Failed,
    /// Invocation was retried after empty output.
    RetriedAfterEmpty,
    /// Invocation was cancelled.
    Cancelled,
}

/// Durable provenance record for a single adapter invocation.
///
/// Every call through an AgentAdapter produces exactly one of these records.
/// The record is persisted to the `adapter_invocations` table and is never
/// silently dropped.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProvenanceRecord {
    /// Unique invocation identifier.
    pub invocation_id: String,
    /// Task ID from the control plane.
    pub task_id: String,
    /// Worker ID that initiated the invocation.
    pub worker_id: String,
    /// Kind of agent that was invoked.
    pub agent_kind: AgentKind,
    /// The prompt/instruction sent (captured for audit).
    pub input_summary: String,
    /// Hash of the full input (for large inputs, the summary is a truncation).
    pub input_hash: String,
    /// The output content received (captured for audit).
    pub output_summary: Option<String>,
    /// Hash of the full output.
    pub output_hash: Option<String>,
    /// Exit code for CLI-based agents.
    pub exit_code: Option<i32>,
    /// Stderr capture (UTF-8 validated).
    pub stderr_capture: Option<String>,
    /// Outcome classification.
    pub outcome: InvocationOutcome,
    /// Duration of the invocation in milliseconds.
    pub duration_ms: u64,
    /// Number of retry attempts made for this invocation chain.
    pub retry_attempt: u32,
    /// Whether UTF-8 validation passed on the output.
    pub utf8_valid: bool,
    /// Timestamp of invocation start.
    pub started_at: DateTime<Utc>,
    /// Timestamp of invocation completion.
    pub completed_at: DateTime<Utc>,
}

/// Record an adapter invocation in the `adapter_invocations` table.
///
/// This uses the lightweight `AdapterProvenance` struct that every adapter
/// already populates in its response, rather than the heavier `ProvenanceRecord`.
/// The function is behind the `persistence` feature gate (requires sqlx).
///
/// Schema assumed:
/// ```sql
/// CREATE TABLE IF NOT EXISTS adapter_invocations (
///     invocation_id   TEXT PRIMARY KEY,
///     task_id         TEXT NOT NULL,
///     adapter_name    TEXT NOT NULL,
///     model_used      TEXT NOT NULL,
///     provider        TEXT NOT NULL,
///     status          TEXT NOT NULL,
///     duration_ms     BIGINT NOT NULL,
///     output_length   BIGINT NOT NULL,
///     started_at      TEXT NOT NULL,
///     finished_at     TEXT NOT NULL,
///     recorded_at     TIMESTAMPTZ NOT NULL DEFAULT now()
/// );
/// ```
#[cfg(feature = "persistence")]
pub async fn record_invocation(
    pool: &sqlx::PgPool,
    response: &AdapterResponse,
) -> Result<(), sqlx::Error> {
    let status_str = match response.status {
        AdapterStatus::Succeeded => "succeeded",
        AdapterStatus::Failed => "failed",
        AdapterStatus::TimedOut => "timed_out",
        AdapterStatus::EmptyOutput => "empty_output",
        AdapterStatus::MalformedOutput => "malformed_output",
        AdapterStatus::RetryableError => "retryable_error",
    };

    sqlx::query(
        "INSERT INTO adapter_invocations \
             (invocation_id, task_id, adapter_name, model_used, provider, \
              status, duration_ms, output_length, started_at, finished_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
         ON CONFLICT (invocation_id) DO NOTHING",
    )
    .bind(&response.provenance.invocation_id)
    .bind(&response.task_id)
    .bind(&response.provenance.adapter_name)
    .bind(&response.provenance.model_used)
    .bind(&response.provenance.provider)
    .bind(status_str)
    .bind(response.duration_ms as i64)
    .bind(response.output.len() as i64)
    .bind(&response.provenance.started_at)
    .bind(&response.provenance.finished_at)
    .execute(pool)
    .await?;

    Ok(())
}
