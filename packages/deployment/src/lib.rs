//! Deployment and update schemas (M6).
//!
//! This crate defines deployment modes, remote endpoints, update channels,
//! migration compatibility, routing resolution, and related adapter types.
//!
//! Items: DEP-001 through DEP-012.
//!
//! Key design rules:
//! - Deployment mode is a first-class typed enum, never hidden in env vars.
//! - Remote transport errors are typed distinctly from local certification
//!   failures so they never masquerade as local outcomes.
//! - All persistence and routing types are explicit, machine-readable, and
//!   versioned.

pub mod adapters;
pub mod channels;
pub mod compatibility;
pub mod endpoints;
pub mod mode;
pub mod persistence;
pub mod preflight;
pub mod routing;
pub mod ui;
pub mod updater;

// Re-export primary types for ergonomic imports.
pub use adapters::{
    CompileAdapterConfig, CompileAdapterResult, CompileAdapterStatus,
    RemoteCertificationAdapterConfig, RemoteCertificationAdapterResult,
    RemoteCertificationAdapterStatus, RemoteTransportError,
};
pub use channels::{UpdateChannel, UpdateChannelPolicy};
pub use compatibility::{
    CompatibilityStatus, MigrationCompatibilityPolicy, MigrationCompatibilityRecord,
    SchemaVersionRange,
};
pub use endpoints::{
    LeanCompileEndpoint, RemoteCertificationEndpoint, EndpointHealth,
};
pub use mode::DeploymentMode;
pub use persistence::{
    DeploymentPolicyError, DeploymentPolicyRecord, DeploymentPolicyScope,
    load_deployment_policy, save_deployment_policy,
};
pub use preflight::{
    MigrationPreflightCheck, MigrationPreflightResult, PreflightCheckKind,
    PreflightOutcome, run_migration_preflight,
};
pub use routing::{
    EndpointType, RoutingDecision, RoutingPolicy, RoutingReason, RoutingTarget,
    resolve_routing,
};
pub use ui::{
    DeploymentModeDisplay, DeploymentStatusProjection, EndpointStatusDisplay,
};
pub use adapters::{bridge_certify_via_gateway, remote_compile};
pub use updater::{
    UpdateApplyResult, UpdateHook, UpdateHookKind, UpdateInfo, Updater,
    UpdaterIntegration,
};
