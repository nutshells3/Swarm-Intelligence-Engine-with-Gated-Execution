//! Policy enforcement logic (Tier 3).
//!
//! Provides pure functions that evaluate user policy and execution policy
//! to produce dispatch decisions, timeouts, retry budgets, concurrency
//! checks, and model bindings. No I/O -- callers supply state.

use user_policy::{GlobalExecutionPolicy, ModelBinding, UserPolicySnapshot};

/// The outcome of a policy check.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PolicyDecision {
    /// Whether the action is permitted.
    pub allowed: bool,
    /// Human-readable reason.
    pub reason: String,
    /// The numeric limit that was applied, if any.
    pub applied_limit: Option<i32>,
}

/// Stateless policy enforcement functions.
pub struct PolicyEnforcer;

impl PolicyEnforcer {
    /// Check whether a new task may be dispatched given current policy.
    ///
    /// Rules evaluated:
    /// - `active_agent_count` must be below `max_active_agents`.
    /// - `task_class` must not be empty (basic sanity).
    pub fn can_dispatch(
        policy: &GlobalExecutionPolicy,
        active_agent_count: i32,
        task_class: &str,
    ) -> PolicyDecision {
        if task_class.is_empty() {
            return PolicyDecision {
                allowed: false,
                reason: "task_class must not be empty".to_string(),
                applied_limit: None,
            };
        }

        if active_agent_count >= policy.max_active_agents {
            return PolicyDecision {
                allowed: false,
                reason: format!(
                    "active agent count ({}) has reached the limit ({})",
                    active_agent_count, policy.max_active_agents
                ),
                applied_limit: Some(policy.max_active_agents),
            };
        }

        PolicyDecision {
            allowed: true,
            reason: format!(
                "dispatch allowed: {} of {} agent slots in use",
                active_agent_count, policy.max_active_agents
            ),
            applied_limit: Some(policy.max_active_agents),
        }
    }

    /// Return the timeout (in seconds) to apply for a given task class.
    ///
    /// Currently returns a uniform default from policy. Task-class-specific
    /// overrides can be layered on top via `ExecutionPolicy::timeouts` in
    /// the control-plane policy module.
    pub fn get_timeout(_policy: &GlobalExecutionPolicy, _task_class: &str) -> u64 {
        // The GlobalExecutionPolicy does not carry per-task-class timeouts;
        // those live in ExecutionPolicy::timeouts (POL-006). Return a
        // sensible default here (10 minutes).
        600
    }

    /// Return the retry budget for a given task class.
    pub fn get_retry_budget(policy: &GlobalExecutionPolicy, _task_class: &str) -> i32 {
        policy.default_retry_budget
    }

    /// Check whether concurrency limits allow one more worker.
    ///
    /// Returns `true` if `running_count` is below the configured
    /// `default_concurrency` ceiling.
    pub fn check_concurrency(
        policy: &GlobalExecutionPolicy,
        running_count: i32,
    ) -> bool {
        running_count < policy.default_concurrency
    }

    /// Resolve the model binding for a given worker role from the user
    /// policy snapshot.
    ///
    /// Mapping:
    /// - "planner" -> `snapshot.planner`
    /// - "implementer" -> `snapshot.implementer`
    /// - "reviewer" -> `snapshot.reviewer`
    /// - "debugger" -> `snapshot.debugger`
    /// - "research" -> `snapshot.research`
    /// - anything else -> fallback to provider_mode + model_family from
    ///   the global policy
    pub fn resolve_model(
        policy: &UserPolicySnapshot,
        worker_role: &str,
    ) -> ModelBinding {
        match worker_role {
            "planner" => policy.planner.clone(),
            "implementer" => policy.implementer.clone(),
            "reviewer" => policy.reviewer.clone(),
            "debugger" => policy.debugger.clone(),
            "research" => policy.research.clone(),
            _ => {
                // Fallback: build a binding from global defaults
                ModelBinding {
                    provider_mode: Some(policy.global.default_provider_mode),
                    provider_name: None,
                    model_name: Some(policy.global.default_model_family.clone()),
                    reasoning_effort: None,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use user_policy::ProviderMode;

    fn test_global_policy() -> GlobalExecutionPolicy {
        GlobalExecutionPolicy {
            default_provider_mode: ProviderMode::Api,
            default_model_family: "claude-4".to_string(),
            max_active_agents: 4,
            default_concurrency: 3,
            default_retry_budget: 2,
            certification_routing: "auto".to_string(),
        }
    }

    #[test]
    fn dispatch_allowed_when_below_limit() {
        let policy = test_global_policy();
        let decision = PolicyEnforcer::can_dispatch(&policy, 2, "implementation");
        assert!(decision.allowed);
    }

    #[test]
    fn dispatch_denied_when_at_limit() {
        let policy = test_global_policy();
        let decision = PolicyEnforcer::can_dispatch(&policy, 4, "implementation");
        assert!(!decision.allowed);
    }

    #[test]
    fn dispatch_denied_for_empty_task_class() {
        let policy = test_global_policy();
        let decision = PolicyEnforcer::can_dispatch(&policy, 0, "");
        assert!(!decision.allowed);
    }

    #[test]
    fn concurrency_check() {
        let policy = test_global_policy();
        assert!(PolicyEnforcer::check_concurrency(&policy, 2));
        assert!(!PolicyEnforcer::check_concurrency(&policy, 3));
    }

    #[test]
    fn retry_budget() {
        let policy = test_global_policy();
        assert_eq!(PolicyEnforcer::get_retry_budget(&policy, "review"), 2);
    }
}
