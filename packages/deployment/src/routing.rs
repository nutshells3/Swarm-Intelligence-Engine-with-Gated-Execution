//! DEP-007: Local/remote routing resolution.
//!
//! CSV guardrail: "Define local/remote routing resolution."
//! Caution: "Do not let remote transport errors masquerade as local
//!   certification."
//! Acceptance: routing simulation.

use serde::{Deserialize, Serialize};

use crate::endpoints::EndpointHealth;
use crate::mode::DeploymentMode;

/// The target a routing decision resolves to.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RoutingTarget {
    /// Route to the local toolchain.
    Local,
    /// Route to a remote endpoint.
    Remote,
    /// Skip the operation (only valid when certification is disabled).
    Skip,
}

/// The reason a particular routing target was chosen. Logged for
/// audit and debugging.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RoutingReason {
    /// Deployment mode mandates this target.
    ModeMandate,
    /// Remote endpoint is unhealthy; falling back to local.
    RemoteUnhealthy,
    /// Local toolchain is unavailable; using remote.
    LocalUnavailable,
    /// Operator preference override.
    OperatorOverride,
    /// Certification is disabled by policy.
    CertificationDisabled,
}

/// A resolved routing decision. Immutable once produced; the control
/// plane logs it and forwards work accordingly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutingDecision {
    /// What kind of work is being routed (e.g. "certification",
    /// "lean_compile").
    pub work_kind: String,
    /// Where the work is routed.
    pub target: RoutingTarget,
    /// Why this target was chosen.
    pub reason: RoutingReason,
    /// The endpoint ID when target is Remote.
    pub endpoint_id: Option<String>,
}

/// Policy inputs used by the routing resolver. The resolver produces
/// a `RoutingDecision` from these inputs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutingPolicy {
    /// Current deployment mode.
    pub deployment_mode: DeploymentMode,
    /// Whether the local toolchain is available.
    pub local_available: bool,
    /// Health of the remote certification endpoint.
    pub remote_certification_health: EndpointHealth,
    /// Health of the remote Lean compile endpoint.
    pub remote_compile_health: EndpointHealth,
    /// Optional operator override target.
    pub operator_override: Option<RoutingTarget>,
}

/// The kind of endpoint being routed (certification vs. lean compile).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointType {
    Certification,
    LeanCompile,
}

/// Resolve a routing decision from the policy and endpoint type.
///
/// Rules:
/// - Operator override always wins (with `OperatorOverride` reason).
/// - `CertificationDisabled` -> always Skip.
/// - `LocalOnly` -> always Local (regardless of endpoint type).
/// - `LocalPlusRemote` -> prefer local; fallback to remote if local is
///   unavailable AND the relevant remote endpoint is Healthy or
///   Degraded.
/// - `RemoteCertificationPreferred` -> prefer remote for certification,
///   fallback to local if remote is Unhealthy/Unknown. For Lean
///   compile, same as LocalPlusRemote.
///
/// Remote transport errors are typed distinctly from local certification
/// failures, so this resolver never lets a transport failure masquerade
/// as a local outcome (per playbook rule 7).
pub fn resolve_routing(
    policy: &RoutingPolicy,
    endpoint_type: EndpointType,
) -> RoutingDecision {
    let work_kind = match endpoint_type {
        EndpointType::Certification => "certification".to_string(),
        EndpointType::LeanCompile => "lean_compile".to_string(),
    };

    // 1. Operator override always wins.
    if let Some(target) = policy.operator_override {
        return RoutingDecision {
            work_kind,
            target,
            reason: RoutingReason::OperatorOverride,
            endpoint_id: None,
        };
    }

    // 2. CertificationDisabled -> Skip.
    if policy.deployment_mode == DeploymentMode::CertificationDisabled {
        return RoutingDecision {
            work_kind,
            target: RoutingTarget::Skip,
            reason: RoutingReason::CertificationDisabled,
            endpoint_id: None,
        };
    }

    let remote_health = match endpoint_type {
        EndpointType::Certification => &policy.remote_certification_health,
        EndpointType::LeanCompile => &policy.remote_compile_health,
    };
    let remote_usable = matches!(remote_health, EndpointHealth::Healthy | EndpointHealth::Degraded);

    match policy.deployment_mode {
        DeploymentMode::LocalOnly => RoutingDecision {
            work_kind,
            target: RoutingTarget::Local,
            reason: RoutingReason::ModeMandate,
            endpoint_id: None,
        },

        DeploymentMode::LocalPlusRemote => {
            if policy.local_available {
                RoutingDecision {
                    work_kind,
                    target: RoutingTarget::Local,
                    reason: RoutingReason::ModeMandate,
                    endpoint_id: None,
                }
            } else if remote_usable {
                RoutingDecision {
                    work_kind,
                    target: RoutingTarget::Remote,
                    reason: RoutingReason::LocalUnavailable,
                    endpoint_id: None,
                }
            } else {
                // Both unavailable -- still route local so the caller
                // gets an explicit local failure rather than a silent
                // fallback.
                RoutingDecision {
                    work_kind,
                    target: RoutingTarget::Local,
                    reason: RoutingReason::ModeMandate,
                    endpoint_id: None,
                }
            }
        }

        DeploymentMode::RemoteCertificationPreferred => {
            if endpoint_type == EndpointType::Certification {
                if remote_usable {
                    RoutingDecision {
                        work_kind,
                        target: RoutingTarget::Remote,
                        reason: RoutingReason::ModeMandate,
                        endpoint_id: None,
                    }
                } else if policy.local_available {
                    RoutingDecision {
                        work_kind,
                        target: RoutingTarget::Local,
                        reason: RoutingReason::RemoteUnhealthy,
                        endpoint_id: None,
                    }
                } else {
                    // Both unavailable -- prefer local so we surface
                    // an explicit failure, never a silent fallback.
                    RoutingDecision {
                        work_kind,
                        target: RoutingTarget::Local,
                        reason: RoutingReason::RemoteUnhealthy,
                        endpoint_id: None,
                    }
                }
            } else {
                // Lean compile: same as LocalPlusRemote
                if policy.local_available {
                    RoutingDecision {
                        work_kind,
                        target: RoutingTarget::Local,
                        reason: RoutingReason::ModeMandate,
                        endpoint_id: None,
                    }
                } else if remote_usable {
                    RoutingDecision {
                        work_kind,
                        target: RoutingTarget::Remote,
                        reason: RoutingReason::LocalUnavailable,
                        endpoint_id: None,
                    }
                } else {
                    RoutingDecision {
                        work_kind,
                        target: RoutingTarget::Local,
                        reason: RoutingReason::ModeMandate,
                        endpoint_id: None,
                    }
                }
            }
        }

        // CertificationDisabled already handled above.
        DeploymentMode::CertificationDisabled => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_policy() -> RoutingPolicy {
        RoutingPolicy {
            deployment_mode: DeploymentMode::LocalOnly,
            local_available: true,
            remote_certification_health: EndpointHealth::Healthy,
            remote_compile_health: EndpointHealth::Healthy,
            operator_override: None,
        }
    }

    #[test]
    fn local_only_always_routes_local() {
        let policy = base_policy();
        let decision = resolve_routing(&policy, EndpointType::Certification);
        assert_eq!(decision.target, RoutingTarget::Local);
        assert_eq!(decision.reason, RoutingReason::ModeMandate);
    }

    #[test]
    fn certification_disabled_always_skips() {
        let mut policy = base_policy();
        policy.deployment_mode = DeploymentMode::CertificationDisabled;
        let decision = resolve_routing(&policy, EndpointType::Certification);
        assert_eq!(decision.target, RoutingTarget::Skip);
        assert_eq!(decision.reason, RoutingReason::CertificationDisabled);
    }

    #[test]
    fn operator_override_wins() {
        let mut policy = base_policy();
        policy.operator_override = Some(RoutingTarget::Remote);
        let decision = resolve_routing(&policy, EndpointType::Certification);
        assert_eq!(decision.target, RoutingTarget::Remote);
        assert_eq!(decision.reason, RoutingReason::OperatorOverride);
    }

    #[test]
    fn local_plus_remote_prefers_local() {
        let mut policy = base_policy();
        policy.deployment_mode = DeploymentMode::LocalPlusRemote;
        let decision = resolve_routing(&policy, EndpointType::Certification);
        assert_eq!(decision.target, RoutingTarget::Local);
    }

    #[test]
    fn local_plus_remote_falls_back_to_remote() {
        let mut policy = base_policy();
        policy.deployment_mode = DeploymentMode::LocalPlusRemote;
        policy.local_available = false;
        let decision = resolve_routing(&policy, EndpointType::Certification);
        assert_eq!(decision.target, RoutingTarget::Remote);
        assert_eq!(decision.reason, RoutingReason::LocalUnavailable);
    }

    #[test]
    fn remote_preferred_uses_remote_when_healthy() {
        let mut policy = base_policy();
        policy.deployment_mode = DeploymentMode::RemoteCertificationPreferred;
        let decision = resolve_routing(&policy, EndpointType::Certification);
        assert_eq!(decision.target, RoutingTarget::Remote);
        assert_eq!(decision.reason, RoutingReason::ModeMandate);
    }

    #[test]
    fn remote_preferred_falls_back_to_local_when_unhealthy() {
        let mut policy = base_policy();
        policy.deployment_mode = DeploymentMode::RemoteCertificationPreferred;
        policy.remote_certification_health = EndpointHealth::Unhealthy;
        let decision = resolve_routing(&policy, EndpointType::Certification);
        assert_eq!(decision.target, RoutingTarget::Local);
        assert_eq!(decision.reason, RoutingReason::RemoteUnhealthy);
    }

    #[test]
    fn work_kind_is_correct() {
        let policy = base_policy();
        let cert = resolve_routing(&policy, EndpointType::Certification);
        assert_eq!(cert.work_kind, "certification");
        let compile = resolve_routing(&policy, EndpointType::LeanCompile);
        assert_eq!(compile.work_kind, "lean_compile");
    }
}
