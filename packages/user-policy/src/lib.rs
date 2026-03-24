use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderMode {
    Api,
    Session,
    Local,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FormalizerMode {
    Off,
    Optional,
    Required,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ModelBinding {
    pub provider_mode: Option<ProviderMode>,
    pub provider_name: Option<String>,
    pub model_name: Option<String>,
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FormalizerPolicy {
    pub enabled: bool,
    pub mode: FormalizerMode,
    pub binding: ModelBinding,
    pub certification_frequency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalExecutionPolicy {
    pub default_provider_mode: ProviderMode,
    pub default_model_family: String,
    pub max_active_agents: i32,
    pub default_concurrency: i32,
    pub default_retry_budget: i32,
    pub certification_routing: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserPolicySnapshot {
    pub policy_id: String,
    pub global: GlobalExecutionPolicy,
    pub planner: ModelBinding,
    pub implementer: ModelBinding,
    pub reviewer: ModelBinding,
    pub debugger: ModelBinding,
    pub research: ModelBinding,
    pub formalizer_a: FormalizerPolicy,
    pub formalizer_b: FormalizerPolicy,
}
