//! DEP-012: Deployment mode UI data model.
//!
//! CSV guardrail: "Define deployment mode UI data model."
//! Acceptance: schema validation.
//!
//! These types are display-oriented projections for the operator
//! dashboard, derived from authoritative deployment policy records.
//!
//! ## API endpoint status
//!
//! The orchestration-api does **not** currently expose a
//! `/api/deployment/config` endpoint. The existing route set covers
//! objectives, tasks, certification, metrics, and peer messaging but
//! has no deployment surface.
//!
//! **Recommendation**: a `/api/deployment/config` GET/PATCH route
//! should be added to `services/orchestration-api/src/routes/` in a
//! follow-up bundle, consuming `DeploymentStatusProjection` (for GET)
//! and accepting a `DeploymentPolicyRecord`-shaped update (for PATCH).
//! The route must follow the 4-part contract rule: OpenAPI + generated
//! client types + runtime decoder + boundary test.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::channels::UpdateChannel;
use crate::endpoints::EndpointHealth;
use crate::mode::DeploymentMode;

/// Display-oriented summary of the current deployment mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeploymentModeDisplay {
    /// The active deployment mode.
    pub mode: DeploymentMode,
    /// Human-readable label for the mode.
    pub label: String,
    /// Whether certification is currently active (derived from mode).
    pub certification_active: bool,
    /// Whether remote endpoints are configured and reachable.
    pub remote_available: bool,
}

/// Display-oriented status of a single endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EndpointStatusDisplay {
    pub endpoint_id: String,
    pub label: String,
    pub health: EndpointHealth,
    pub last_health_check: Option<DateTime<Utc>>,
}

/// Full deployment status projection for the operator dashboard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeploymentStatusProjection {
    /// Current deployment mode display.
    pub mode_display: DeploymentModeDisplay,
    /// Status of all configured endpoints.
    pub endpoints: Vec<EndpointStatusDisplay>,
    /// Active update channel.
    pub update_channel: UpdateChannel,
    /// Current system version.
    pub current_version: String,
    /// Whether an update is available.
    pub update_available: bool,
    /// Available update version, if any.
    pub available_version: Option<String>,
    /// Timestamp of this projection.
    pub projected_at: DateTime<Utc>,
}
