//! DEP-006: Deployment policy persistence types and runtime.
//!
//! CSV guardrail: "Define deployment policy persistence types."
//! Acceptance: schema validation.
//!
//! Deployment policy is stored as a typed, versioned record -- never
//! inferred from environment variables or scattered across config files.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::channels::UpdateChannelPolicy;
use crate::compatibility::MigrationCompatibilityPolicy;
use crate::mode::DeploymentMode;

/// Scope to which a deployment policy applies.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentPolicyScope {
    /// Applies to the entire system.
    Global,
    /// Applies to a specific objective.
    Objective,
    /// Applies to a specific session.
    Session,
}

/// A versioned, persisted deployment policy record. The control plane
/// loads the active record at session start and snapshots it for each
/// cycle so policy changes never take effect mid-cycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeploymentPolicyRecord {
    pub policy_id: String,
    /// Monotonically increasing revision for versioning.
    pub revision: u32,
    /// Scope of this policy.
    pub scope: DeploymentPolicyScope,
    /// The deployment mode (DEP-001).
    pub deployment_mode: DeploymentMode,
    /// Update channel policy (DEP-004).
    pub update_channel: UpdateChannelPolicy,
    /// Migration compatibility enforcement (DEP-005).
    pub migration_compatibility: MigrationCompatibilityPolicy,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Errors from deployment policy persistence operations.
#[derive(Debug, thiserror::Error)]
pub enum DeploymentPolicyError {
    #[error("database error: {0}")]
    Database(String),
    #[error("deserialization error: {0}")]
    Deserialization(String),
    #[error("policy not found: {0}")]
    NotFound(String),
}

impl From<sqlx::Error> for DeploymentPolicyError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e.to_string())
    }
}

/// Load the active deployment policy for the given scope from the
/// `deployment_policies` table. Returns the most recent revision.
///
/// The query selects the single row with the highest revision for the
/// given scope so that policy changes are version-tracked and the
/// control plane always works with the latest committed version.
pub async fn load_deployment_policy(
    pool: &PgPool,
    scope: DeploymentPolicyScope,
) -> Result<DeploymentPolicyRecord, DeploymentPolicyError> {
    let scope_str = serde_json::to_value(&scope)
        .map_err(|e| DeploymentPolicyError::Deserialization(e.to_string()))?
        .as_str()
        .unwrap_or("global")
        .to_owned();

    let row = sqlx::query_as::<_, (String, i32, String, String, String, String, DateTime<Utc>, DateTime<Utc>)>(
        "SELECT policy_id, revision, scope, deployment_mode, \
               update_channel, migration_compatibility, \
               created_at, updated_at \
         FROM deployment_policies \
         WHERE scope = $1 \
         ORDER BY revision DESC \
         LIMIT 1",
    )
    .bind(&scope_str)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        DeploymentPolicyError::NotFound(format!("no policy for scope '{scope_str}'"))
    })?;

    let deployment_mode: DeploymentMode = serde_json::from_str(&format!("\"{}\"", row.3))
        .map_err(|e| DeploymentPolicyError::Deserialization(e.to_string()))?;
    let update_channel: UpdateChannelPolicy = serde_json::from_str(&row.4)
        .map_err(|e| DeploymentPolicyError::Deserialization(e.to_string()))?;
    let migration_compatibility: MigrationCompatibilityPolicy =
        serde_json::from_str(&row.5)
            .map_err(|e| DeploymentPolicyError::Deserialization(e.to_string()))?;

    Ok(DeploymentPolicyRecord {
        policy_id: row.0,
        revision: row.1 as u32,
        scope,
        deployment_mode,
        update_channel,
        migration_compatibility,
        created_at: row.6,
        updated_at: row.7,
    })
}

/// Save (INSERT) a deployment policy record into the
/// `deployment_policies` table. The caller is responsible for
/// incrementing the revision; this function does not auto-increment.
///
/// Uses INSERT so that every revision is preserved as an immutable
/// audit trail. The control plane reads the highest revision via
/// [`load_deployment_policy`].
pub async fn save_deployment_policy(
    pool: &PgPool,
    policy: &DeploymentPolicyRecord,
) -> Result<(), DeploymentPolicyError> {
    let scope_str = serde_json::to_value(&policy.scope)
        .map_err(|e| DeploymentPolicyError::Deserialization(e.to_string()))?
        .as_str()
        .unwrap_or("global")
        .to_owned();

    let mode_str = serde_json::to_value(&policy.deployment_mode)
        .map_err(|e| DeploymentPolicyError::Deserialization(e.to_string()))?
        .as_str()
        .unwrap_or("local_only")
        .to_owned();

    let update_channel_json = serde_json::to_string(&policy.update_channel)
        .map_err(|e| DeploymentPolicyError::Deserialization(e.to_string()))?;

    let migration_compat_json = serde_json::to_string(&policy.migration_compatibility)
        .map_err(|e| DeploymentPolicyError::Deserialization(e.to_string()))?;

    sqlx::query(
        "INSERT INTO deployment_policies \
             (policy_id, revision, scope, deployment_mode, \
              update_channel, migration_compatibility, \
              created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&policy.policy_id)
    .bind(policy.revision as i32)
    .bind(&scope_str)
    .bind(&mode_str)
    .bind(&update_channel_json)
    .bind(&migration_compat_json)
    .bind(policy.created_at)
    .bind(policy.updated_at)
    .execute(pool)
    .await?;

    Ok(())
}
