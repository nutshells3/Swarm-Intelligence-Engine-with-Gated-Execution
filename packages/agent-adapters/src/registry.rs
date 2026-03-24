//! Runtime adapter registry.
//!
//! Manages a collection of available agent adapters, providing auto-detection
//! from the environment (CLI tools on PATH, API keys in env vars) and
//! selection based on preference or availability.

use crate::adapter::{AdapterRequest, AdapterResponse, AgentAdapter, AgentKind};
use crate::anthropic_api::AnthropicApiAdapter;
use crate::capability::{AdapterCapabilityRecord, AdapterCapabilityRegistry, AdapterHealth};
use crate::claude::ClaudeCliAdapter;
use crate::codex::CodexCliAdapter;
use crate::custom_cli::CustomCliAdapter;
use crate::local_model::LocalModelAdapter;
use crate::mock::MockAdapter;
use crate::openai_api::OpenAiApiAdapter;

/// A boxed adapter that can be stored in the registry.
///
/// Because `AgentAdapter::invoke` returns `impl Future`, we cannot use
/// `dyn AgentAdapter` directly. Instead, we define a trait-object-safe
/// wrapper that boxes the future.
pub trait BoxedAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn agent_kind(&self) -> AgentKind;
    fn invoke_boxed(
        &self,
        request: AdapterRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = AdapterResponse> + Send + '_>>;
}

/// Blanket implementation: any AgentAdapter can be used as a BoxedAdapter.
impl<T: AgentAdapter> BoxedAdapter for T {
    fn name(&self) -> &str {
        AgentAdapter::name(self)
    }

    fn agent_kind(&self) -> AgentKind {
        AgentAdapter::agent_kind(self)
    }

    fn invoke_boxed(
        &self,
        request: AdapterRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = AdapterResponse> + Send + '_>> {
        Box::pin(AgentAdapter::invoke(self, request))
    }
}

/// Registry of available agent adapters.
///
/// Use `auto_detect()` to populate from the environment, or build manually
/// with `register()`.
pub struct AdapterRegistry {
    adapters: Vec<Box<dyn BoxedAdapter>>,
    /// ADT-009: Capability registry populated during auto_detect.
    pub capabilities: AdapterCapabilityRegistry,
}

impl AdapterRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            adapters: Vec::new(),
            capabilities: AdapterCapabilityRegistry::new(),
        }
    }

    /// Register a new adapter and record its capabilities.
    pub fn register<A: AgentAdapter + 'static>(&mut self, adapter: A) {
        // ADT-009: Build a capability record from the adapter metadata.
        let cap = build_capability_record(&adapter);
        self.capabilities.register(cap);
        self.adapters.push(Box::new(adapter));
    }

    /// Auto-detect available adapters from the environment.
    ///
    /// Checks for:
    /// - `claude` CLI on PATH
    /// - `codex` CLI on PATH
    /// - `ANTHROPIC_API_KEY` env var (enables Anthropic API adapter)
    /// - `OPENAI_API_KEY` env var (enables OpenAI API adapter)
    pub fn auto_detect() -> Self {
        let mut registry = Self::new();

        // Demo mode: use mock adapter only (no external dependencies).
        if std::env::var("SIEGE_DEMO_MODE").ok().map_or(false, |v| v == "1") {
            tracing::info!("SIEGE_DEMO_MODE=1 — registering mock adapter only");
            registry.register(MockAdapter::new());
            return registry;
        }

        // Check if claude CLI is available.
        if which("claude").is_ok() {
            tracing::info!("Auto-detected claude CLI on PATH");
            registry.register(ClaudeCliAdapter::new());
        }

        // Check if codex CLI is available.
        if which("codex").is_ok() {
            tracing::info!("Auto-detected codex CLI on PATH");
            registry.register(CodexCliAdapter::new());
        }

        // Check for Anthropic API key.
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            if !key.is_empty() {
                tracing::info!("Auto-detected ANTHROPIC_API_KEY");
                registry.register(AnthropicApiAdapter::new(key));
            }
        }

        // Check for OpenAI API key.
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            if !key.is_empty() {
                tracing::info!("Auto-detected OPENAI_API_KEY");
                registry.register(OpenAiApiAdapter::new(key));
            }
        }

        // ADT-016: Check for custom CLI adapter via env var.
        if let Ok(cli_path) = std::env::var("SWARM_CUSTOM_CLI") {
            if !cli_path.is_empty() {
                tracing::info!(path = %cli_path, "Auto-detected SWARM_CUSTOM_CLI");
                registry.register(CustomCliAdapter::new(cli_path));
            }
        }

        // Check for local model servers.
        // Custom URL takes priority.
        if let Ok(url) = std::env::var("SWARM_LOCAL_MODEL_URL") {
            let model = std::env::var("SWARM_LOCAL_MODEL_NAME")
                .unwrap_or_else(|_| "default".to_string());
            tracing::info!(url = %url, model = %model, "Auto-detected SWARM_LOCAL_MODEL_URL");
            registry.register(LocalModelAdapter::custom(url, model, "custom".to_string()));
        } else {
            // Probe ollama (localhost:11434)
            if probe_local_server("http://localhost:11434") {
                tracing::info!("Auto-detected ollama on localhost:11434");
                registry.register(LocalModelAdapter::ollama());
            }
            // Probe vLLM (localhost:8000)
            if probe_local_server("http://localhost:8000") {
                let model = std::env::var("SWARM_VLLM_MODEL")
                    .unwrap_or_else(|_| "default".to_string());
                tracing::info!("Auto-detected vLLM on localhost:8000");
                registry.register(LocalModelAdapter::vllm(model));
            }
        }

        registry
    }

    /// Look up an adapter by name.
    pub fn get(&self, name: &str) -> Option<&dyn BoxedAdapter> {
        self.adapters
            .iter()
            .find(|a| a.name() == name)
            .map(|a| a.as_ref())
    }

    /// List the names of all registered adapters.
    pub fn list(&self) -> Vec<&str> {
        self.adapters.iter().map(|a| a.name()).collect()
    }

    /// Return how many adapters are registered.
    pub fn len(&self) -> usize {
        self.adapters.len()
    }

    /// Return whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }

    /// Select the best adapter for a request.
    ///
    /// If `preferred` is given and registered, returns that adapter.
    /// Otherwise returns the first registered adapter (if any).
    pub fn select(&self, preferred: Option<&str>) -> Option<&dyn BoxedAdapter> {
        if let Some(name) = preferred {
            if let Some(adapter) = self.get(name) {
                return Some(adapter);
            }
        }
        self.adapters.first().map(|a| a.as_ref())
    }

    /// Find all adapters of a given agent kind.
    pub fn find_by_kind(&self, kind: AgentKind) -> Vec<&dyn BoxedAdapter> {
        self.adapters
            .iter()
            .filter(|a| a.agent_kind() == kind)
            .map(|a| a.as_ref())
            .collect()
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Check whether a command is available on PATH.
fn which(cmd: &str) -> Result<(), ()> {
    let check_cmd = if cfg!(windows) { "where" } else { "which" };
    std::process::Command::new(check_cmd)
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|_| ())
        .and_then(|s| if s.success() { Ok(()) } else { Err(()) })
}

/// ADT-009: Build a capability record from an adapter's trait metadata.
fn build_capability_record<A: AgentAdapter>(adapter: &A) -> AdapterCapabilityRecord {
    let kind = adapter.agent_kind();
    let name = adapter.name().to_string();

    // Derive sensible defaults based on agent kind.
    let (accepted_tasks, max_ctx, supports_stream, supports_cancel, provider_mode, model_name) =
        match kind {
            AgentKind::Claude => (
                vec!["code".into(), "review".into(), "plan".into(), "chat".into()],
                200_000_u32,
                false,
                false,
                "cli".to_string(),
                "claude-default".to_string(),
            ),
            AgentKind::Codex => (
                vec!["code".into(), "review".into()],
                128_000_u32,
                false,
                false,
                "cli".to_string(),
                "codex-default".to_string(),
            ),
            AgentKind::HttpApi => (
                vec!["code".into(), "review".into(), "plan".into(), "chat".into()],
                200_000_u32,
                true,
                true,
                "api".to_string(),
                "api-default".to_string(),
            ),
            AgentKind::Local => (
                vec!["code".into(), "chat".into()],
                32_000_u32,
                false,
                false,
                "local".to_string(),
                "local-default".to_string(),
            ),
            AgentKind::GenericCli => (
                vec!["code".into(), "chat".into()],
                100_000_u32,
                false,
                false,
                "cli".to_string(),
                "custom-cli".to_string(),
            ),
        };

    AdapterCapabilityRecord {
        adapter_id: name.clone(),
        agent_kind: kind,
        display_name: name,
        accepted_task_kinds: accepted_tasks,
        max_concurrency: 4,
        max_context_tokens: max_ctx,
        supports_streaming: supports_stream,
        supports_cancel,
        health: AdapterHealth::Unknown,
        model_name,
        provider_mode,
    }
}

/// Probe whether a local model server is listening (non-blocking, 500ms timeout).
fn probe_local_server(base_url: &str) -> bool {
    // Use a synchronous TCP connect probe — faster than HTTP for detection.
    let addr = base_url
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    std::net::TcpStream::connect_timeout(
        &addr.parse().unwrap_or_else(|_| {
            std::net::SocketAddr::from(([127, 0, 0, 1], 0))
        }),
        std::time::Duration::from_millis(500),
    )
    .is_ok()
}
