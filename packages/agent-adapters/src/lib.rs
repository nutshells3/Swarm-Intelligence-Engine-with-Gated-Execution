//! External agent adapters -- governed wrappers for coding/chat agents
//! (ADT-001 through ADT-010).
//!
//! This crate provides:
//!
//! - **Adapter trait** (ADT-001): The core `AgentAdapter` trait that all
//!   external agent wrappers implement, enforcing UTF-8 I/O, durable
//!   provenance capture, and empty-output retry contracts.
//!
//! - **Codex adapter** (ADT-002, ADT-003, ADT-006): Codex CLI adapter
//!   that runs `codex exec --skip-git-repo-check`.
//!
//! - **Claude adapter** (ADT-004, ADT-005, ADT-007): Claude CLI adapter
//!   that runs `claude --print --output-format json`.
//!
//! - **Anthropic API adapter**: Direct HTTP adapter for the Anthropic
//!   Messages API (POST /v1/messages).
//!
//! - **OpenAI API adapter**: Direct HTTP adapter for the OpenAI Chat
//!   Completions API (POST /v1/chat/completions).
//!
//! - **Registry**: Runtime registry that auto-detects available adapters
//!   from CLI tools on PATH and API keys in env vars.
//!
//! - **Provenance** (ADT-008): Durable invocation provenance records.
//!
//! - **Capability registry** (ADT-009): Registry of available adapters
//!   and their capabilities.
//!
//! - **Output normalization** (ADT-010): UTF-8 validation, whitespace
//!   normalization, and output truncation.
//!
//! Key design rules:
//! - No silent shell hacks; all subprocess calls are explicit.
//! - No implicit env-only behavior; all config is typed.
//! - UTF-8 is enforced at the adapter boundary.
//! - Every invocation creates provenance metadata.

pub mod adapter;
pub mod anthropic_api;
pub mod capability;
pub mod claude;
pub mod codex;
pub mod custom_cli;
pub mod local_model;
pub mod mock;
pub mod normalize;
pub mod openai_api;
pub mod provenance;
pub mod registry;
pub mod spawn;

// Re-export primary types for ergonomic imports.
pub use adapter::{
    AdapterError, AdapterErrorKind, AdapterInput, AdapterOutput, AdapterProvenance,
    AdapterRequest, AdapterResponse, AdapterStatus, AgentAdapter, AgentKind, TokenUsage,
};
pub use capability::{AdapterCapabilityRecord, AdapterCapabilityRegistry, AdapterHealth};
pub use claude::{ClaudeAdapter, ClaudeCliAdapter, ClaudeRequestConfig, ClaudeResponseMeta, ClaudeStopReason};
pub use codex::{CodexAdapter, CodexCliAdapter, CodexEnvVar, CodexRequestConfig, CodexResponseMeta};
pub use normalize::{NormalizationPolicy, NormalizationResult, NormalizedOutput, normalize_output, extract_codex_exec_content};
pub use provenance::{InvocationOutcome, ProvenanceRecord};
#[cfg(feature = "persistence")]
pub use provenance::record_invocation;
pub use registry::{AdapterRegistry, BoxedAdapter};
pub use anthropic_api::AnthropicApiAdapter;
pub use custom_cli::CustomCliAdapter;
pub use local_model::LocalModelAdapter;
pub use openai_api::OpenAiApiAdapter;
pub use spawn::{
    SpawnBackend, SpawnConfig, SpawnError, SpawnHandle, SpawnResult,
    SubprocessSpawnBackend, IsolationHint,
    // G2 SPN-004 through SPN-009 types
    CommandPrepManifest, StdinMode, PromptDeliveryMode, PermissionPolicy,
    SpawnBootstrapResult, ReadinessStatus, RuntimeBindingManifest,
    normalize_command, bootstrap_spawned_worker, check_readiness, prepare_runtime_env,
};
