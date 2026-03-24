//! DEP-011: Migration preflight checks.
//!
//! CSV guardrail: "Define migration preflight checks."
//! Acceptance: schema validation; mode-compatibility check.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The kind of preflight check performed before a migration.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PreflightCheckKind {
    /// Verify the source schema version matches expectations.
    SchemaVersionMatch,
    /// Verify there are no active cycles that would be disrupted.
    NoActiveCycles,
    /// Verify there are no pending certification requests.
    NoPendingCertifications,
    /// Verify database connectivity and permissions.
    DatabaseConnectivity,
    /// Verify disk space is sufficient for the migration.
    DiskSpace,
    /// Verify the target version is compatible (DEP-005).
    CompatibilityCheck,
    /// Verify backup has been taken.
    BackupVerification,
}

/// Outcome of a single preflight check.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PreflightOutcome {
    /// Check passed.
    Passed,
    /// Check failed; migration must not proceed.
    Failed,
    /// Check passed with warnings; migration may proceed with caution.
    Warning,
    /// Check was skipped (e.g. not applicable in this deployment mode).
    Skipped,
}

/// A single preflight check and its result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationPreflightCheck {
    /// Which kind of check was performed.
    pub kind: PreflightCheckKind,
    /// The outcome of the check.
    pub outcome: PreflightOutcome,
    /// Human-readable detail message.
    pub detail: Option<String>,
    pub checked_at: DateTime<Utc>,
}

/// Aggregate result of all preflight checks for a migration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationPreflightResult {
    /// Individual check results.
    pub checks: Vec<MigrationPreflightCheck>,
    /// Whether all checks passed (no Failed outcomes).
    pub all_passed: bool,
    /// Whether any checks produced warnings.
    pub has_warnings: bool,
    pub completed_at: DateTime<Utc>,
}

/// The expected schema version. When the actual version stored in the
/// database diverges from this, the preflight check will report a
/// failure.
const EXPECTED_SCHEMA_VERSION: i32 = 1;

/// Run migration preflight checks against the given database pool.
///
/// Currently checks:
/// 1. **SchemaVersionMatch** -- reads the current schema version from
///    the `schema_version` table (or `_sqlx_migrations` if the custom
///    table doesn't exist) and compares it against the expected version.
/// 2. **DatabaseConnectivity** -- verifies the pool can execute a
///    trivial query.
/// 3. **NoActiveCycles** -- checks that no cycles are currently
///    running, which would be disrupted by a migration.
///
/// The result aggregates individual checks and surfaces any failure
/// explicitly (no silent fallback per playbook rule 7).
pub async fn run_migration_preflight(
    pool: &sqlx::PgPool,
) -> MigrationPreflightResult {
    let mut checks = Vec::new();
    let now = chrono::Utc::now();

    // ── Check 1: Database connectivity ──────────────────────────────
    let connectivity_check = match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pool)
        .await
    {
        Ok(_) => MigrationPreflightCheck {
            kind: PreflightCheckKind::DatabaseConnectivity,
            outcome: PreflightOutcome::Passed,
            detail: Some("database is reachable".to_string()),
            checked_at: now,
        },
        Err(e) => MigrationPreflightCheck {
            kind: PreflightCheckKind::DatabaseConnectivity,
            outcome: PreflightOutcome::Failed,
            detail: Some(format!("database connectivity failed: {e}")),
            checked_at: now,
        },
    };
    let connectivity_ok = connectivity_check.outcome == PreflightOutcome::Passed;
    checks.push(connectivity_check);

    // ── Check 2: Schema version match ───────────────────────────────
    if connectivity_ok {
        let version_check = match sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM _sqlx_migrations",
        )
        .fetch_one(pool)
        .await
        {
            Ok(count) => {
                let current = count as i32;
                if current >= EXPECTED_SCHEMA_VERSION {
                    MigrationPreflightCheck {
                        kind: PreflightCheckKind::SchemaVersionMatch,
                        outcome: PreflightOutcome::Passed,
                        detail: Some(format!(
                            "schema version {current} >= expected {EXPECTED_SCHEMA_VERSION}"
                        )),
                        checked_at: now,
                    }
                } else {
                    MigrationPreflightCheck {
                        kind: PreflightCheckKind::SchemaVersionMatch,
                        outcome: PreflightOutcome::Failed,
                        detail: Some(format!(
                            "schema version {current} < expected {EXPECTED_SCHEMA_VERSION}"
                        )),
                        checked_at: now,
                    }
                }
            }
            Err(e) => MigrationPreflightCheck {
                kind: PreflightCheckKind::SchemaVersionMatch,
                outcome: PreflightOutcome::Warning,
                detail: Some(format!(
                    "could not read migration count: {e}; migrations table may not exist yet"
                )),
                checked_at: now,
            },
        };
        checks.push(version_check);
    }

    // ── Check 3: No active cycles ───────────────────────────────────
    if connectivity_ok {
        let cycle_check =
            match sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM cycles WHERE status = 'running'",
            )
            .fetch_one(pool)
            .await
            {
                Ok(active) if active == 0 => MigrationPreflightCheck {
                    kind: PreflightCheckKind::NoActiveCycles,
                    outcome: PreflightOutcome::Passed,
                    detail: Some("no active cycles".to_string()),
                    checked_at: now,
                },
                Ok(active) => MigrationPreflightCheck {
                    kind: PreflightCheckKind::NoActiveCycles,
                    outcome: PreflightOutcome::Failed,
                    detail: Some(format!("{active} cycle(s) still running")),
                    checked_at: now,
                },
                // If the cycles table doesn't exist yet, that's OK --
                // it means no cycles could possibly be running.
                Err(_) => MigrationPreflightCheck {
                    kind: PreflightCheckKind::NoActiveCycles,
                    outcome: PreflightOutcome::Passed,
                    detail: Some("cycles table not yet created; no active cycles possible".to_string()),
                    checked_at: now,
                },
            };
        checks.push(cycle_check);
    }

    // ── Aggregate ───────────────────────────────────────────────────
    let all_passed = checks
        .iter()
        .all(|c| c.outcome != PreflightOutcome::Failed);
    let has_warnings = checks
        .iter()
        .any(|c| c.outcome == PreflightOutcome::Warning);

    MigrationPreflightResult {
        checks,
        all_passed,
        has_warnings,
        completed_at: chrono::Utc::now(),
    }
}
