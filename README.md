# SIEGE

**Swarm Intelligence Engine with Gated Execution**

A multi-agent orchestration engine that plans before it acts.

Give SIEGE an engineering objective and it turns that objective into a gated execution cycle: it extracts intent from conversation, drafts a plan, validates that plan against explicit readiness conditions, decomposes the work into a dependency-aware task graph, dispatches agents across isolated git worktrees, integrates the results, and feeds what it learned back into the next cycle.

This is not a “planning-as-a-suggestion” system. In SIEGE, planning is a hard gate. If the plan is incomplete, execution does not start.

<video src="https://github.com/user-attachments/assets/a1b86f0e-d8a7-4d9f-886c-e18f7d02440a" controls width="100%"></video>

---

## Documentation

- [Docs index](docs/README.md)
- [Core concepts](docs/CONCEPTS.md)
- [Architecture overview](docs/ARCHITECTURE.md)
- [Gated execution and planning](docs/GATED_EXECUTION_AND_PLANNING.md)
- [Formal assurance](docs/FORMAL_ASSURANCE.md)
- [Adapters and execution](docs/ADAPTERS_AND_EXECUTION.md)
- [Governance: reviews, conflicts, promotion](docs/GOVERNANCE.md)
- [Dashboard, API, and CLI](docs/DASHBOARD_API_AND_CLI.md)
- [Deployment and scaling](docs/DEPLOYMENT_AND_SCALING.md)
- [Status and maturity](docs/STATUS.md)
- [Origins and research context](docs/ORIGINS.md)

---

## Why SIEGE

Most multi-agent stacks treat planning as advisory. They can fan out quickly, but weak plans create expensive downstream failures: overlapping file edits, invalid task orderings, merge churn, retries caused by missing acceptance criteria, and fragile execution that only looks scalable until the first dependency breaks.

SIEGE was built to attack that failure mode at the source.

Before implementation begins, the planning engine evaluates a **10-condition gate**. The gate checks whether the objective has been summarized, the architecture is drafted, milestones exist, acceptance criteria are defined, dependencies are valid, invariants are extracted and holding, risks are identified, and unresolved questions remain within budget. If those conditions do not hold, the engine blocks execution instead of pretending the plan is good enough.

That single design choice changes the character of the system. SIEGE is optimized for **controlled parallelism**, **traceable execution**, and **policy-driven orchestration** rather than blind swarm activity.

---

## What SIEGE does

Given a goal such as:

```text
Build a REST API for user authentication with JWT support, tests, and admin endpoints.
```

SIEGE can:

- extract constraints, decisions, and open questions from conversation,
- elaborate an architecture and milestone tree,
- evaluate the planning gate before code generation begins,
- decompose the objective into a dependency-aware task graph,
- dispatch different tasks to different agent roles and model providers,
- isolate concurrent work in separate git worktrees,
- detect overlap and conflict risk before integration,
- merge completed work back through controlled integration,
- run project-aware build and test steps,
- route selected claims through formal-assurance or certification lanes,
- persist execution history, failures, and lessons for later cycles.

In other words: SIEGE is not just an agent runner. It is an orchestration engine with planning, control-plane discipline, review surfaces, conflict handling, and optional assurance paths built into the runtime model.

---

## Quick start

```bash
# Prerequisites: Docker, Rust 1.86+, Node.js 20+
make dev
```

That starts PostgreSQL, the API server, the loop runner, worker dispatch, and the web dashboard.

Then open:

- Web dashboard: `http://localhost:5173`
- Swagger UI: `http://127.0.0.1:8845/swagger-ui/`

You can also run services individually:

```bash
make db
make api
make loop
make dispatch
make web
make cli
```

Demo mode without real LLM calls:

```bash
make demo
```

---

## Status

The backend pipeline has been verified end-to-end with real LLM calls (Codex CLI / gpt-5.4) and real formal-claim certification (OAE engine):

```text
objective → plan gate (10 conditions, completeness 1.0)
  → decomposition (dependency-aware task graph)
  → dispatch (parallel, isolated git worktrees)
  → execution (Codex CLI, multiple tasks succeeded)
  → certification (formal-claim CLI invoked, results projected)
  → retry on timeout, heartbeat emission, zero errors
```

The web dashboard is code-complete (12 panels, generated types, SSE streaming) but has not yet been integration-tested against a live backend. The Tauri desktop shell builds but is untested.

---

## How it works

```text
Objective
  │
  ▼
Conversation extraction
  │  Parse constraints, decisions, and open questions from chat
  ▼
Plan elaboration
  │  Draft architecture, milestones, risks, and invariants
  ▼
10-condition planning gate
  │  Block execution until plan readiness is explicit
  ▼
Decomposition
  │  Build a dependency-aware task graph and inject prior lessons
  ▼
Dispatch
  │  Select adapters, resolve skills, enforce policy and concurrency
  ▼
Parallel execution
  │  Run tasks in isolated git worktrees
  ▼
Integration
  │  Merge results, run build and test workflows, update state
  ▼
Review / conflicts / certification
  │  Surface disputes, gate sensitive claims, preserve auditability
  ▼
Next cycle
     Learn from failures, generate follow-on tasks, continue iterating
```

Every major transition is designed to be explicit. SIEGE keeps planning artifacts, execution state, event history, and downstream review or certification consequences tied to the same orchestration loop rather than scattering them across ad hoc scripts.

---

## Adapters and policy

SIEGE is designed to work with multiple execution backends and model providers.

The engine can detect or route across:

- Claude Code CLI
- Codex CLI
- Anthropic API
- OpenAI-compatible APIs
- local OpenAI-compatible endpoints
- custom CLI tools
- mock/demo adapters

Role-specific policy lets you use different models or providers for different parts of the cycle: planner, implementer, reviewer, formalizer, and related roles. Concurrency limits, retry budgets, and certification routing are policy concerns, not hard-coded assumptions.

---

## Dashboard and CLI

SIEGE includes both an interactive dashboard and a CLI REPL.

The dashboard surfaces the orchestration loop through dedicated views for planning, tasks, branches, conflicts, certification, reviews, skills, settings, and loop history. The CLI gives you a direct operational surface for creating objectives, inspecting gate status, tailing events, and checking task progress.

The API and CLI expose event-streaming surfaces, and the web UI provides a live operational view of the system as cycles advance.

---

## Why this engine looks different

SIEGE was not designed as a minimal chat wrapper around a task queue.

It comes from a broader line of work concerned with large-scale decomposition, structured reasoning, formalization pathways, and robustness-aware execution. That research origin is why the engine treats planning, policy, review, conflict handling, and formal assurance as first-class runtime concerns rather than optional extras layered on after the fact.

You do not need the broader research context to use SIEGE as an orchestration engine. But that context explains why the engine is unusually opinionated about gates, traceability, and execution discipline.

---

## Core capabilities

### Gated planning

SIEGE blocks implementation until plan readiness is explicit and machine-checkable. The gate is part of the runtime, not a prompt convention.

### Dependency-aware decomposition

Tasks are organized as an ordered graph rather than a flat queue. That matters when different agents operate concurrently on related milestones.

### Isolated worktree execution

Each task can run in its own git worktree, reducing branch contention and making integration safer.

### Policy-driven orchestration

Provider choice, retry behavior, concurrency ceilings, and certification routing are controlled through explicit policy rather than buried in code paths.

### Conflict and review surfaces

Parallel systems fail in predictable ways. SIEGE promotes conflicts, reviews, and branch state into first-class entities instead of hiding them behind logs.

### Formal assurance and certification

SIEGE integrates with the Formal Claim engine (OAE) to route correctness-critical outputs through actual formal verification. This is not a placeholder: the certification pipeline has been tested end-to-end with real CLI invocations.

The engine supports a **hybrid certification model**:

- **Per-task certification** runs during execution. Nodes flagged `certification_required` are verified immediately when their task succeeds. Integration is blocked until per-task certification passes.
- **Post-integration certification** runs after merge. System-level claims that only become verifiable after integration (cross-module invariants, integration properties) are swept in a second pass.

Certification behavior is fully policy-driven:

```json
{
  "enabled": true,
  "frequency": "critical_only",
  "routing": "local",
  "grace_period_seconds": 10,
  "certification_timeout_seconds": 120,
  "formalizer_a": { "enabled": true, "mode": "required" },
  "formalizer_b": { "enabled": true, "mode": "optional" }
}
```

- `frequency: always` certifies every succeeded task. `critical_only` limits to contract/invariant/proof keywords and `certification_required` nodes. `on_request` requires explicit submission. `off` disables entirely.
- Dual formalization runs two independent verification passes and flags divergence.
- Connect the Formal Claim CLI via `FORMAL_CLAIM_CLI_PATH` environment variable or the HTTP gateway via `FORMAL_CLAIM_ENDPOINT`.

The formal assurance stack is built across several repositories:

| Repository | Role |
|------------|------|
| [orchestration-assurance-engine](https://github.com/nutshells3/orchestration-assurance-engine) | Certification engine, CLI, claim trace, audit pipeline |
| [fwp](https://github.com/nutshells3/fwp) | Formal verification adapters (Lean, Isabelle, Rocq) |
| [proof-assistant](https://github.com/nutshells3/proof-assistant) | Proof execution engine |
| [safeslice](https://github.com/nutshells3/safeslice) | Safe decomposition and slicing for verification targets |

### Iterative learning

The engine records failures and outcomes so later cycles can decompose or retry with more context instead of starting from zero every time.

---

## Architecture

```text
apps/
  web/          Dashboard
  desktop/      Tauri shell
  cli/          Interactive REPL

services/
  orchestration-api/    HTTP API, event surfaces, Swagger UI
  loop-runner/          Cycle state machine
  worker-dispatch/      Task execution and worktree dispatch

packages/
  planning-engine       Plan gate and planning validation
  control-plane         Command shapes and orchestration boundaries
  agent-adapters        Provider adapters and runtime normalization
  conversation-engine   Chat extraction and plan update pipeline
  skill-registry        Skill resolution and versioned packs
  review-governance     Review templates, scheduling, digests
  conflict-system       Conflict entities and routing
  integration           Integration and certification seams
  formal-readiness      Readiness predicates and export surfaces
  recursive-improvement Failure memory, scoring, safety checks
  git-control           Worktree lifecycle
  user-policy           Runtime execution policy
  state-model           Core orchestration types
  ui-models             Canonical read-model types
  scaling               Event-bus and deployment scaling seams
  observability         Metrics, heartbeats, retention policies
  roadmap-model         Planning and roadmap structures
  robustness-policy     Context budgets, overlap checks, parse recovery
  deployment            Modes, routing, preflight
  worker-protocol       Worker envelopes and messaging
```

---

## Where SIEGE fits

If you want a lightweight single-agent coding tool, SIEGE is overkill.

If you want an engine that can coordinate planning, controlled parallel execution, isolation, review, conflict handling, and optional assurance flows under one roof, that is the category SIEGE is targeting.

It is best thought of as a **general orchestration engine with research-grade execution discipline**.

---

## License

AGPL-3.0. See [LICENSE](LICENSE).
