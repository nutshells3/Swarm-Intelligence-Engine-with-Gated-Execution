//! ADT-009: Capability registry for external agent adapters.
//!
//! Tracks which agents are available, what they can do, and their current
//! health status. The control plane uses this to route tasks to the
//! appropriate adapter.

use serde::{Deserialize, Serialize};
use crate::adapter::AgentKind;

/// Health status of a registered agent adapter.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdapterHealth {
    /// Adapter is healthy and accepting invocations.
    Healthy,
    /// Adapter is experiencing intermittent issues.
    Degraded,
    /// Adapter is unreachable or non-functional.
    Unhealthy,
    /// Adapter health has not been checked yet.
    Unknown,
}

/// ADT-009 -- A registered adapter's capability record.
///
/// The capability registry maintains one entry per available agent adapter.
/// The control plane queries this to decide which adapter to use for a
/// given task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterCapabilityRecord {
    /// Unique adapter identifier.
    pub adapter_id: String,
    /// Kind of agent this adapter wraps.
    pub agent_kind: AgentKind,
    /// Human-readable name for the adapter.
    pub display_name: String,
    /// Task kinds this adapter can handle.
    pub accepted_task_kinds: Vec<String>,
    /// Maximum concurrent invocations.
    pub max_concurrency: u32,
    /// Maximum context tokens the agent can accept.
    pub max_context_tokens: u32,
    /// Whether the adapter supports streaming output.
    pub supports_streaming: bool,
    /// Whether the adapter supports cancellation.
    pub supports_cancel: bool,
    /// Current health status.
    pub health: AdapterHealth,
    /// Model name configured for this adapter.
    pub model_name: String,
    /// Provider mode (api, session, local).
    pub provider_mode: String,
}

/// The capability registry itself -- a collection of adapter records.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterCapabilityRegistry {
    pub adapters: Vec<AdapterCapabilityRecord>,
}

impl AdapterCapabilityRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            adapters: Vec::new(),
        }
    }

    /// Register a new adapter.
    pub fn register(&mut self, record: AdapterCapabilityRecord) {
        self.adapters.push(record);
    }

    /// Find adapters that can handle a given task kind.
    pub fn find_by_task_kind(&self, task_kind: &str) -> Vec<&AdapterCapabilityRecord> {
        self.adapters
            .iter()
            .filter(|a| a.accepted_task_kinds.iter().any(|k| k == task_kind))
            .filter(|a| a.health != AdapterHealth::Unhealthy)
            .collect()
    }

    /// Find adapters by agent kind.
    pub fn find_by_agent_kind(&self, kind: AgentKind) -> Vec<&AdapterCapabilityRecord> {
        self.adapters
            .iter()
            .filter(|a| a.agent_kind == kind)
            .collect()
    }

    /// Update health status for an adapter.
    pub fn update_health(&mut self, adapter_id: &str, health: AdapterHealth) {
        if let Some(adapter) = self.adapters.iter_mut().find(|a| a.adapter_id == adapter_id) {
            adapter.health = health;
        }
    }
}

impl Default for AdapterCapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}
