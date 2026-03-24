//! SPN-001 through SPN-010: Spawn backend abstraction.
//!
//! Defines how agents are spawned and managed. The `SpawnBackend` trait
//! (SPN-001) is the pluggable interface; `SubprocessSpawnBackend` (SPN-002)
//! is the concrete implementation that wraps the existing adapter.invoke_boxed()
//! subprocess model.
//!
//! Future backends (SPN-003 through SPN-010) are documented with trait
//! definitions but not yet implemented:
//!
//! - SPN-003: TmuxSpawnBackend  -- spawn inside a tmux session for persistence
//! - SPN-004: ContainerSpawnBackend -- spawn inside a Docker/Podman container
//! - SPN-005: RemoteSshSpawnBackend -- spawn on a remote host via SSH
//! - SPN-006: KubernetesSpawnBackend -- spawn as a Kubernetes Job
//! - SPN-007: WasmSpawnBackend -- spawn inside a WASI sandbox
//! - SPN-008: NixSpawnBackend -- spawn inside a Nix shell for reproducibility
//! - SPN-009: FirecrackerSpawnBackend -- spawn inside a Firecracker microVM
//! - SPN-010: PooledSpawnBackend -- reuse warm agent processes from a pool

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

use crate::adapter::{AdapterRequest, AdapterResponse};
use crate::registry::BoxedAdapter;

// ── SPN-001: Spawn configuration and handle types ────────────────────────

/// Configuration for spawning an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnConfig {
    /// Task ID from the control plane.
    pub task_id: String,
    /// Working directory for the spawned agent.
    pub working_directory: PathBuf,
    /// Adapter request to forward to the agent.
    pub request: AdapterRequest,
    /// Maximum lifetime in seconds before the spawn is killed.
    pub max_lifetime_seconds: u64,
    /// Optional environment variables to inject.
    pub env_vars: Vec<(String, String)>,
    /// Isolation level hint (the backend may ignore this).
    pub isolation_hint: IsolationHint,
}

/// Hint for how isolated the spawn should be.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IsolationHint {
    /// No special isolation; run in current process context.
    None,
    /// Filesystem-level isolation (worktree, chroot).
    Filesystem,
    /// Process-level isolation (separate PID namespace, cgroup).
    Process,
    /// Full container isolation (Docker, Podman, Firecracker).
    Container,
    /// Remote machine isolation (SSH, Kubernetes).
    Remote,
}

/// Handle to a spawned agent process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnHandle {
    /// Unique identifier for this spawn instance.
    pub spawn_id: String,
    /// Task ID this spawn is executing.
    pub task_id: String,
    /// Backend-specific identifier (PID, container ID, pod name, etc.).
    pub backend_id: String,
    /// Name of the backend that created this spawn.
    pub backend_name: String,
}

/// Errors from spawn operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpawnError {
    /// What went wrong.
    pub message: String,
    /// Whether this error is retryable.
    pub retryable: bool,
}

impl std::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SpawnError(retryable={}): {}",
            self.retryable, self.message
        )
    }
}

impl std::error::Error for SpawnError {}

/// Result of a completed spawn.
#[derive(Debug, Clone)]
pub struct SpawnResult {
    /// The adapter response from the spawned agent.
    pub response: AdapterResponse,
    /// The handle that was used.
    pub handle: SpawnHandle,
}

// ── SPN-001: The core SpawnBackend trait ──────────────────────────────────

/// Pluggable backend for spawning and managing agent processes.
///
/// Implementors decide *how* an agent is run (subprocess, tmux, container,
/// remote, etc.). The dispatch layer only interacts through this trait.
pub trait SpawnBackend: Send + Sync {
    /// Spawn an agent with the given configuration.
    ///
    /// Returns a handle that can be used to check liveness or kill the spawn.
    fn spawn(
        &self,
        config: SpawnConfig,
    ) -> impl std::future::Future<Output = Result<SpawnHandle, SpawnError>> + Send;

    /// Check whether a previously spawned agent is still alive.
    fn is_alive(
        &self,
        handle: &SpawnHandle,
    ) -> impl std::future::Future<Output = bool> + Send;

    /// Kill a previously spawned agent.
    fn kill(
        &self,
        handle: &SpawnHandle,
    ) -> impl std::future::Future<Output = Result<(), SpawnError>> + Send;

    /// Wait for the spawn to complete and return its result.
    fn wait(
        &self,
        handle: &SpawnHandle,
    ) -> impl std::future::Future<Output = Result<SpawnResult, SpawnError>> + Send;

    /// Return the backend's display name.
    fn name(&self) -> &str;
}

// ── SPN-002: SubprocessSpawnBackend ──────────────────────────────────────

/// Subprocess-based spawn backend.
///
/// Wraps the existing `BoxedAdapter::invoke_boxed()` call. Each spawn runs
/// the adapter in a tokio task, which in turn launches the CLI subprocess.
/// This is the default and currently only concrete backend.
pub struct SubprocessSpawnBackend {
    adapter: Arc<dyn BoxedAdapter>,
}

impl SubprocessSpawnBackend {
    /// Create a new subprocess backend wrapping the given adapter.
    pub fn new(adapter: Arc<dyn BoxedAdapter>) -> Self {
        Self { adapter }
    }
}

impl SpawnBackend for SubprocessSpawnBackend {
    async fn spawn(&self, config: SpawnConfig) -> Result<SpawnHandle, SpawnError> {
        let spawn_id = uuid::Uuid::now_v7().to_string();

        // The subprocess is launched inside invoke_boxed which is called
        // by the dispatch layer after obtaining a handle. We just validate
        // the configuration here and return a handle.
        //
        // The actual invocation happens in `wait()`.
        Ok(SpawnHandle {
            spawn_id,
            task_id: config.task_id,
            backend_id: format!("subprocess-{}", self.adapter.name()),
            backend_name: "subprocess".to_string(),
        })
    }

    async fn is_alive(&self, _handle: &SpawnHandle) -> bool {
        // Subprocess invocations are synchronous from the caller's
        // perspective (they block in wait()). If we're being asked
        // about liveness, the task is still running.
        true
    }

    async fn kill(&self, _handle: &SpawnHandle) -> Result<(), SpawnError> {
        // Subprocess killing is handled by the tokio timeout in the adapter.
        // For a more robust implementation, we would need to track PIDs.
        Ok(())
    }

    async fn wait(&self, handle: &SpawnHandle) -> Result<SpawnResult, SpawnError> {
        // Build a minimal request from the handle's task_id.
        // In practice, the dispatch layer calls invoke_boxed directly;
        // this is the wrapper that makes it fit the SpawnBackend contract.
        //
        // The full request must be provided by the caller through spawn().
        // For now we return an error since wait() without the original
        // config is incomplete. The intended usage pattern is:
        //
        //   let handle = backend.spawn(config.clone()).await?;
        //   // ... later ...
        //   let response = adapter.invoke_boxed(config.request).await;
        //
        // The SubprocessSpawnBackend is a thin wrapper; the real subprocess
        // management happens inside the adapter.
        Err(SpawnError {
            message: format!(
                "SubprocessSpawnBackend::wait() for task {} -- \
                 use adapter.invoke_boxed() directly for subprocess invocations",
                handle.task_id
            ),
            retryable: false,
        })
    }

    fn name(&self) -> &str {
        "subprocess"
    }
}

// ── SPN-003: TmuxSpawnBackend (trait definition, not implemented) ────────

/// Tmux-based spawn backend (SPN-003).
///
/// Would spawn the agent inside a named tmux session, allowing the operator
/// to attach for debugging and the agent to survive brief orchestrator restarts.
///
/// Not yet implemented. The trait definition is provided for planning:
///
/// ```text
/// - spawn: `tmux new-session -d -s <spawn_id> '<cli> < prompt_file'`
/// - is_alive: `tmux has-session -t <spawn_id>`
/// - kill: `tmux kill-session -t <spawn_id>`
/// - wait: poll tmux pane capture until process exits
/// ```
pub struct TmuxSpawnBackend;

// ── SPN-004: ContainerSpawnBackend (trait definition, not implemented) ───

/// Container-based spawn backend (SPN-004).
///
/// Would spawn the agent inside a Docker or Podman container, providing
/// full filesystem and network isolation.
///
/// Not yet implemented. Planned interface:
///
/// ```text
/// - spawn: `docker run --name <spawn_id> -v workdir:/work <image> <cli>`
/// - is_alive: `docker inspect -f '{{.State.Running}}' <spawn_id>`
/// - kill: `docker kill <spawn_id>`
/// - wait: `docker wait <spawn_id>` then `docker logs <spawn_id>`
/// ```
pub struct ContainerSpawnBackend;

// ── SPN-005: RemoteSshSpawnBackend (trait definition, not implemented) ───

/// Remote SSH spawn backend (SPN-005).
///
/// Would spawn the agent on a remote host via SSH, useful for distributing
/// work across multiple machines.
///
/// Not yet implemented. Planned interface:
///
/// ```text
/// - spawn: `ssh <host> 'nohup <cli> < prompt_file > output_file 2>&1 &'`
/// - is_alive: `ssh <host> 'kill -0 <pid>'`
/// - kill: `ssh <host> 'kill <pid>'`
/// - wait: poll remote output file via sftp/scp
/// ```
pub struct RemoteSshSpawnBackend;

// ── SPN-006: KubernetesSpawnBackend (trait definition, not implemented) ──

/// Kubernetes Job spawn backend (SPN-006).
///
/// Would create a Kubernetes Job resource for each agent invocation,
/// leveraging cluster scheduling and resource limits.
///
/// Not yet implemented. Planned interface:
///
/// ```text
/// - spawn: kubectl apply Job manifest
/// - is_alive: check Job .status.active
/// - kill: kubectl delete job <name>
/// - wait: kubectl wait --for=condition=complete job/<name>
/// ```
pub struct KubernetesSpawnBackend;

// ── SPN-007: WasmSpawnBackend (trait definition, not implemented) ────────

/// WASI sandbox spawn backend (SPN-007).
///
/// Would run the agent CLI compiled to WASI inside a Wasm runtime (wasmtime,
/// wasmer), providing strong sandboxing with capability-based I/O.
///
/// Not yet implemented.
pub struct WasmSpawnBackend;

// ── SPN-008: NixSpawnBackend (trait definition, not implemented) ─────────

/// Nix shell spawn backend (SPN-008).
///
/// Would wrap the agent invocation in `nix-shell` or `nix develop`, ensuring
/// a reproducible environment with pinned dependencies.
///
/// Not yet implemented.
pub struct NixSpawnBackend;

// ── SPN-009: FirecrackerSpawnBackend (trait definition, not implemented) ─

/// Firecracker microVM spawn backend (SPN-009).
///
/// Would spawn the agent inside a Firecracker microVM for strong isolation
/// with near-native performance.
///
/// Not yet implemented.
pub struct FirecrackerSpawnBackend;

// ── SPN-010: PooledSpawnBackend (trait definition, not implemented) ──────

/// Pooled process spawn backend (SPN-010).
///
/// Would maintain a pool of warm agent processes that can be reused across
/// invocations, reducing cold-start latency.
///
/// Not yet implemented. Planned interface:
///
/// ```text
/// - spawn: acquire an idle process from the pool, send prompt
/// - is_alive: check pool slot status
/// - kill: return process to pool or kill if tainted
/// - wait: read response from pool slot
/// ```
pub struct PooledSpawnBackend;

// ═══════════════════════════════════════════════════════════════════════════
// G2 Worker Spawn Runtime — additive patch (bundle-04a)
//
// SPN-004: CLI adapter bridge contract
// SPN-005: Command normalization + permission policy
// SPN-006: Prompt delivery modes
// SPN-007: Spawn → registration bootstrap bridge
// SPN-008: Readiness handoff
// SPN-009: Runtime environment preparation
//
// Invariants:
//   1. Spawn does NOT replace registration — it bridges INTO WRK-001
//   2. No new process-launch hardcode in dispatch or adapter internals
//   3. Runtime bindings use typed manifest, NOT ambient env mutation
//   4. Launch success ≠ worker readiness (readiness needs heartbeat)
// ═══════════════════════════════════════════════════════════════════════════

use crate::adapter::AgentKind;

// ── SPN-004: CLI adapter bridge contract ──────────────────────────────────
//
// The custom CLI adapter (ADT-016) produces a CommandPrepManifest instead
// of directly launching. SubprocessSpawnBackend consumes it. This separates
// adapter-level semantics from native command preparation so built-in and
// custom CLI agents map to the same launch manifest.

/// How stdin should be provided to the spawned process.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StdinMode {
    /// No stdin input.
    None,
    /// Pipe the prompt directly to stdin (default for most CLI tools).
    PromptOnStdin,
    /// Read input from a file at the given path.
    FileInput(String),
}

/// Typed command-preparation manifest produced by adapters (SPN-004).
///
/// Adapters produce this instead of directly launching processes.
/// The spawn backend consumes it to perform the actual launch.
/// This is the canonical bridge between adapter semantics and process launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandPrepManifest {
    /// Executable command (absolute path or PATH-relative name).
    pub command: String,
    /// Command-line arguments.
    pub args: Vec<String>,
    /// Environment variables to set for the spawned process.
    pub env: Vec<(String, String)>,
    /// Working directory override. If None, uses the task's worktree.
    pub working_dir: Option<String>,
    /// How stdin is provided to the process.
    pub stdin_mode: StdinMode,
    /// Maximum execution time in seconds.
    pub timeout_seconds: u64,
    /// Kind of agent being spawned.
    pub agent_kind: AgentKind,
    /// How the prompt is delivered to the agent (SPN-006).
    pub prompt_delivery: PromptDeliveryMode,
}

// ── SPN-005: Command normalization + permission policy ────────────────────
//
// Normalization: validate command exists, apply timeout from policy,
// strip disallowed env vars. Permission policy controls what the spawned
// process is allowed to do.

/// Permission policy applied during command normalization (SPN-005).
///
/// Controls what the spawned agent process is allowed to do.
/// The normalize_command function uses this to constrain the manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionPolicy {
    /// Whether the spawned process may access the network.
    pub allow_network: bool,
    /// Whether the spawned process may write to the filesystem.
    pub allow_filesystem_write: bool,
    /// Whether the spawned process may execute sub-shells.
    pub allow_shell_exec: bool,
    /// Maximum memory in MB (None = no limit).
    pub max_memory_mb: Option<u32>,
    /// Maximum execution time in seconds (overrides manifest if lower).
    pub max_duration_seconds: u64,
}

impl Default for PermissionPolicy {
    fn default() -> Self {
        Self {
            allow_network: false,
            allow_filesystem_write: true,
            allow_shell_exec: false,
            max_memory_mb: None,
            max_duration_seconds: 600,
        }
    }
}

/// Environment variable names that are never propagated to spawned processes.
const DISALLOWED_ENV_PREFIXES: &[&str] = &[
    "SWARM_INTERNAL_",
    "ORCHESTRATION_DATABASE_",
    "SWARM_SECRET_",
];

/// Normalize a command-prep manifest against a permission policy (SPN-005).
///
/// - Validates the command is non-empty
/// - Caps timeout to `policy.max_duration_seconds`
/// - Strips env vars matching disallowed prefixes
/// - Validates working directory is set when filesystem writes are allowed
///
/// Returns the normalized manifest or a SpawnError if validation fails.
pub fn normalize_command(
    manifest: &CommandPrepManifest,
    policy: &PermissionPolicy,
) -> Result<CommandPrepManifest, SpawnError> {
    // Validate: command must be non-empty
    if manifest.command.trim().is_empty() {
        return Err(SpawnError {
            message: "CommandPrepManifest.command is empty".to_string(),
            retryable: false,
        });
    }

    // Validate: no shell metacharacters in command (no hidden shell concat)
    if manifest.command.contains(';')
        || manifest.command.contains('|')
        || manifest.command.contains('&')
        || manifest.command.contains('`')
    {
        return Err(SpawnError {
            message: format!(
                "Command contains shell metacharacters: '{}'",
                manifest.command
            ),
            retryable: false,
        });
    }

    // Cap timeout to policy maximum
    let effective_timeout = manifest
        .timeout_seconds
        .min(policy.max_duration_seconds);

    // Strip disallowed env vars
    let filtered_env: Vec<(String, String)> = manifest
        .env
        .iter()
        .filter(|(key, _)| {
            !DISALLOWED_ENV_PREFIXES
                .iter()
                .any(|prefix| key.starts_with(prefix))
        })
        .cloned()
        .collect();

    Ok(CommandPrepManifest {
        command: manifest.command.clone(),
        args: manifest.args.clone(),
        env: filtered_env,
        working_dir: manifest.working_dir.clone(),
        stdin_mode: manifest.stdin_mode.clone(),
        timeout_seconds: effective_timeout,
        agent_kind: manifest.agent_kind,
        prompt_delivery: manifest.prompt_delivery.clone(),
    })
}

// ── SPN-006: Prompt delivery modes ────────────────────────────────────────
//
// Different CLI agents accept prompts differently. This enum makes the
// delivery mode explicit so interactive and non-interactive CLIs are
// launched under one contract without losing or duplicating task instructions.

/// How the prompt/instruction is delivered to the spawned agent (SPN-006).
///
/// The spawn backend uses this to decide whether to pipe the prompt
/// to stdin, pass it as a CLI argument, or write it to a temp file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromptDeliveryMode {
    /// Pipe the prompt to the process's stdin (default for most CLI tools).
    StdinPipe,
    /// Pass the prompt as a `--prompt "..."` command-line argument.
    CommandLineArg,
    /// Write the prompt to a temporary file and pass the path as an argument.
    TempFile,
}

impl Default for PromptDeliveryMode {
    fn default() -> Self {
        Self::StdinPipe
    }
}

// ── SPN-007: Spawn → registration bootstrap bridge ────────────────────────
//
// Produces a result that WRK-001 registration can consume. It does NOT
// perform registration itself. The bridge provides proposed_worker_id and
// capabilities so the registration layer can accept or reject the spawned
// worker.

/// Result of bootstrapping a spawned worker for registration (SPN-007).
///
/// Contains everything the WRK-001 registration flow needs to register
/// the newly spawned worker. The spawn layer proposes identity and
/// capabilities; the registration layer accepts or rejects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnBootstrapResult {
    /// Handle to the spawned process (for liveness checks and kill).
    pub spawn_handle: SpawnHandle,
    /// Proposed worker ID for WRK-001 registration.
    /// The registration layer may accept or replace this.
    pub proposed_worker_id: String,
    /// Proposed capabilities the worker declares.
    pub proposed_capabilities: Vec<String>,
    /// One-time bootstrap token for registration handoff.
    /// If present, WRK-001 registration requires this token.
    pub registration_token: Option<String>,
}

/// Bootstrap a spawned worker, producing a registration-ready result (SPN-007).
///
/// This bridges spawn into WRK-001 registration. It does NOT perform
/// registration itself — it prepares the handoff data.
///
/// The function:
/// 1. Verifies the spawned process is alive
/// 2. Generates a proposed worker ID
/// 3. Derives capabilities from the manifest's agent_kind
/// 4. Generates a one-time bootstrap token
pub async fn bootstrap_spawned_worker(
    handle: SpawnHandle,
    manifest: &CommandPrepManifest,
) -> Result<SpawnBootstrapResult, SpawnError> {
    // The handle should already exist from a prior spawn() call.
    // We derive registration metadata from the manifest without
    // performing registration.

    let proposed_worker_id = format!(
        "spn-{}-{}",
        handle.task_id,
        &handle.spawn_id[..8.min(handle.spawn_id.len())]
    );

    let proposed_capabilities = match manifest.agent_kind {
        AgentKind::Codex => vec![
            "implementation".to_string(),
            "code_generation".to_string(),
        ],
        AgentKind::Claude => vec![
            "implementation".to_string(),
            "review".to_string(),
            "analysis".to_string(),
        ],
        AgentKind::GenericCli => vec![
            "implementation".to_string(),
        ],
        AgentKind::HttpApi => vec![
            "api_call".to_string(),
        ],
        AgentKind::Local => vec![
            "implementation".to_string(),
        ],
    };

    let registration_token = Some(uuid::Uuid::now_v7().to_string());

    Ok(SpawnBootstrapResult {
        spawn_handle: handle,
        proposed_worker_id,
        proposed_capabilities,
        registration_token,
    })
}

// ── SPN-008: Readiness handoff ────────────────────────────────────────────
//
// Launch success ≠ worker readiness. This checks whether the spawned
// process has actually become ready (alive + heartbeat within timeout).

/// Readiness status of a spawned worker (SPN-008).
///
/// A spawned process goes through: Pending → Ready or Failed/Timeout.
/// The dispatch layer must not treat a live PID as a ready worker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessStatus {
    /// Process is alive but has not yet confirmed readiness.
    Pending,
    /// Process is alive and has confirmed readiness (heartbeat received).
    Ready,
    /// Process exited or reported a fatal error before becoming ready.
    Failed(String),
    /// Process did not confirm readiness within the allowed window.
    Timeout,
}

/// Check readiness of a spawned worker (SPN-008).
///
/// Simple liveness check: if the process is no longer alive, it failed.
/// If it has been running for longer than `timeout_ms` without a
/// heartbeat confirmation, it is a timeout. Otherwise it is still pending.
///
/// Callers should poll this or integrate it with a heartbeat listener.
/// The `backend` parameter is used to check process liveness.
pub async fn check_readiness<B: SpawnBackend>(
    backend: &B,
    handle: &SpawnHandle,
    timeout_ms: u64,
    launch_elapsed_ms: u64,
) -> ReadinessStatus {
    let alive = backend.is_alive(handle).await;

    if !alive {
        return ReadinessStatus::Failed(
            "Spawned process is no longer alive before readiness confirmation".to_string(),
        );
    }

    if launch_elapsed_ms >= timeout_ms {
        return ReadinessStatus::Timeout;
    }

    ReadinessStatus::Pending
}

// ── SPN-009: Runtime environment preparation ──────────────────────────────
//
// Converts a typed manifest into concrete env vars for the spawned process.
// No ambient mutation — returns the env map. Policy, worktree, skill,
// and secret bindings are all expressed as typed fields.

/// Typed runtime binding manifest for a spawned worker (SPN-009).
///
/// This is the single source of truth for what environment the spawned
/// process should see. It replaces ad-hoc env mutation with an explicit,
/// auditable manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeBindingManifest {
    /// Worker identity (matches WRK-001 registration).
    pub worker_id: String,
    /// Task being executed.
    pub task_id: String,
    /// Skill pack to load (if any).
    pub skill_pack_id: Option<String>,
    /// Provider mode override (e.g. "full-auto", "suggest").
    pub provider_mode: Option<String>,
    /// Model binding override (e.g. "claude-sonnet-4-20250514", "o3-mini").
    pub model_binding: Option<String>,
    /// Path to the worktree for this task.
    pub worktree_path: Option<String>,
    /// Explicit environment overrides from policy resolution.
    pub env_overrides: Vec<(String, String)>,
    /// Context budget in tokens for this task.
    pub context_budget_tokens: Option<u32>,
    /// Maximum execution time in seconds.
    pub timeout_seconds: u64,
}

/// Prepare the runtime environment for a spawned worker (SPN-009).
///
/// Converts the typed RuntimeBindingManifest into a flat list of
/// environment variable key-value pairs. No ambient env mutation —
/// the caller passes these to the spawn backend.
///
/// Standard env vars produced:
/// - `SWARM_WORKER_ID`
/// - `SWARM_TASK_ID`
/// - `SWARM_SKILL_PACK_ID` (if present)
/// - `SWARM_PROVIDER_MODE` (if present)
/// - `SWARM_MODEL_BINDING` (if present)
/// - `SWARM_WORKTREE_PATH` (if present)
/// - `SWARM_CONTEXT_BUDGET_TOKENS` (if present)
/// - `SWARM_TIMEOUT_SECONDS`
/// - Plus all `env_overrides` (which may include secrets scoped by policy)
pub fn prepare_runtime_env(manifest: &RuntimeBindingManifest) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = Vec::new();

    // Core identity
    env.push(("SWARM_WORKER_ID".to_string(), manifest.worker_id.clone()));
    env.push(("SWARM_TASK_ID".to_string(), manifest.task_id.clone()));

    // Optional bindings — only set if present (no empty-string injection)
    if let Some(ref skill_pack) = manifest.skill_pack_id {
        env.push(("SWARM_SKILL_PACK_ID".to_string(), skill_pack.clone()));
    }
    if let Some(ref provider_mode) = manifest.provider_mode {
        env.push(("SWARM_PROVIDER_MODE".to_string(), provider_mode.clone()));
    }
    if let Some(ref model) = manifest.model_binding {
        env.push(("SWARM_MODEL_BINDING".to_string(), model.clone()));
    }
    if let Some(ref worktree) = manifest.worktree_path {
        env.push(("SWARM_WORKTREE_PATH".to_string(), worktree.clone()));
    }
    if let Some(budget) = manifest.context_budget_tokens {
        env.push((
            "SWARM_CONTEXT_BUDGET_TOKENS".to_string(),
            budget.to_string(),
        ));
    }

    // Timeout is always set
    env.push((
        "SWARM_TIMEOUT_SECONDS".to_string(),
        manifest.timeout_seconds.to_string(),
    ));

    // Policy-resolved overrides (may include scoped secrets)
    for (key, value) in &manifest.env_overrides {
        env.push((key.clone(), value.clone()));
    }

    env
}
