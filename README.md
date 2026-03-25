# SIEGE

**Swarm Intelligence Engine with Gated Execution**

Give it an engineering objective. It plans, decomposes, dispatches agents in parallel, verifies the result, and learns from failures — all without you babysitting every step.

---

## Demo

<video src="https://github.com/user-attachments/assets/a1b86f0e-d8a7-4d9f-886c-e18f7d02440a" controls width="100%"></video>

---

## What you can do with it

**Hand it a goal, get a working codebase back.**

```
"Build a REST API for user authentication with JWT tokens"
```

SIEGE will:
1. Call an LLM to draft an architecture and break it into milestones
2. Evaluate a 9-condition planning gate before writing any code
3. Decompose the plan into a dependency-aware task graph
4. Dispatch each task to an isolated git worktree with its own agent
5. Merge all worktrees, run your build and tests automatically
6. Learn from failures and retry with context from previous attempts
7. Optionally route critical claims through formal verification

**Use any LLM you want.** Claude Code, Codex CLI, Anthropic API, OpenAI API, local models via Ollama/vLLM, or any custom CLI tool.

**Watch it work in real time.** 12-panel web dashboard with live SSE streaming, or use the interactive CLI REPL.

---

## Quick start

```bash
# Prerequisites: Docker, Rust 1.86+, Node.js 20+
docker compose up -d          # PostgreSQL
make api                      # API server on :8845
make loop                     # Cycle runner (background)
make dispatch                 # Worker dispatch (background)
make web                      # Dashboard on :5173
```

Or all at once:
```bash
make dev
```

Demo mode (no LLM calls, mock adapter):
```bash
make demo
```

Then open `http://localhost:5173`, type an objective, and watch it go.

---

## How it works

```
You: "Build X"
  |
  v
Plan Elaboration ---- LLM drafts architecture, milestones, risks, invariants
  |
  v
9-Condition Gate ---- Blocks execution until the plan is solid
  |                   (architecture exists, milestones defined, deps acyclic,
  |                    acceptance criteria set, risks identified, invariants hold,
  |                    unresolved questions within budget)
  |
  v
Decomposition ------- Scans your repo, calls LLM to split into task DAG
  |                   Injects failure lessons from previous cycles
  |
  v
Dispatch ------------ Selects adapters, resolves skills, checks concurrency
  |                   Each task gets its own git worktree
  |
  v
Execution ----------- Agents work in parallel, isolated branches
  |                   File overlap detection prevents conflicts
  |
  v
Integration --------- Sequential merge, project-type-aware build/test
  |                   (cargo check, npm test, pytest, make test, go build)
  |
  v
Certification ------- Optional formal-claim gateway with dual formalization
  |                   Stale detection when upstream changes
  |
  v
Next Cycle ---------- Stores failure patterns, generates new tasks, loops
```

The cycle runner ticks every 3 seconds. Each tick advances all active cycles through their current phase.

---

## Adapters

SIEGE auto-detects what's available on your system:

| Adapter | Detection | What it does |
|---------|-----------|-------------|
| Claude Code CLI | `claude` on PATH | Full agent with extended thinking |
| Codex CLI | `codex` on PATH | OpenAI's coding agent |
| Anthropic API | `ANTHROPIC_API_KEY` env | Direct HTTP to Claude models |
| OpenAI API | `OPENAI_API_KEY` env | GPT-4o and compatible |
| Local (Ollama/vLLM) | `OPENAI_API_BASE` env | Any OpenAI-compatible endpoint |
| Custom CLI | `SWARM_CUSTOM_CLI` env | Any stdin/stdout tool |
| Mock | `SIEGE_DEMO_MODE=1` | Canned responses for demos |

Multiple adapters can coexist. Policy controls which adapter handles which task class.

---

## Dashboard

12 live panels:

| Panel | What it shows |
|-------|-------------|
| Dashboard | Cycle progress, agent status, task summary, gate status, event feed |
| Chat | Objective creation, conversation extraction, constraint/decision/question capture |
| Plan | 9-condition gate visualization, milestone tree with status |
| Tasks | Task board grouped by status, dependency-aware ordering |
| Graph | Node dependency graph |
| Branches | Nodes by lane: branch, mainline candidate, mainline, blocked |
| Conflicts | Open/resolved conflicts with competing artifacts |
| Certification | Certification queue, submission status, gate effects |
| Reviews | Plan/architecture/direction reviews with auto-approval, digest generation |
| Settings | Provider mode, model, concurrency, retry budget, certification config |
| Skills | Skill packs and worker templates with version/deprecation status |
| Loop History | Past cycles with phases and tracks |

All panels consume generated TypeScript types from the OpenAPI spec — no handwritten DTOs.

---

## CLI REPL

```bash
make cli
```

```
siege> Build a REST API for user management
  [objective created: obj-01a...]
  [loop created, cycle starting]

siege> /status
  Cycle: plan_elaboration (gate: 7/9 conditions met)

siege> /gate
  [x] objective summarized
  [x] architecture drafted
  [x] milestones created
  [ ] acceptance criteria
  [ ] risks identified
  ...

siege> /tasks
  ID          Node        Role          Status
  task-a1..   Design      planner       succeeded
  task-b2..   Implement   implementer   running
  task-c3..   Test        reviewer      queued

siege> /tail
  [SSE streaming: events appear in real time]
```

---

## Policy system

Control everything from the API or dashboard:

```json
{
  "global": {
    "default_provider_mode": "api",
    "max_active_agents": 4,
    "default_retry_budget": 3,
    "certification_routing": "critical_only"
  },
  "planner":     { "model_name": "claude-sonnet-4-20250514" },
  "implementer": { "model_name": "claude-sonnet-4-20250514" },
  "reviewer":    { "model_name": "gpt-4o" },
  "formalizer_a": { "enabled": true, "mode": "required" },
  "formalizer_b": { "enabled": true, "mode": "optional" }
}
```

Different models for different roles. Dual formalization for safety-critical claims. Per-task overrides.

---

## Stack

| Layer | Technology |
|-------|-----------|
| Control plane | Rust 1.86+, axum 0.8, tokio |
| State store | PostgreSQL 16 (97 tables, 18 migrations) |
| Frontend | React 18, TypeScript 5.8, Vite 6, TanStack Query, Zustand |
| Desktop | Tauri 2 |
| API docs | OpenAPI via utoipa (68/70 routes documented) |
| Type safety | Generated TS types from OpenAPI spec, enum parity checks |

---

## Architecture

```
apps/
  web/          React dashboard (12 panels)
  desktop/      Tauri shell
  cli/          Interactive REPL

services/
  orchestration-api/    84 HTTP routes + SSE streaming + Swagger UI
  loop-runner/          Cycle state machine (19 tick functions)
  worker-dispatch/      Adapter invocation + worktree isolation

packages/
  state-model           Core types (objectives, plans, nodes, tasks, cycles)
  control-plane         Command types + executor (11 commands)
  planning-engine       Plan gate, validation, pipeline traits
  agent-adapters        7 adapters + spawn runtime + normalization
  integration           Formal-claim gateway (CLI + HTTP) + certification pipeline
  conflict-system       5 conflict classes + auto-resolution
  review-governance     4 review templates + scheduling + digest
  skill-registry        Skill packs + 8-level resolution + version pinning
  conversation-engine   Chat extraction + plan update pipeline
  worker-protocol       Protocol envelopes + peer messaging
  user-policy           Execution policy + formalizer config
  roadmap-model         Roadmap nodes + ordering + absorption
  observability         Metrics + sidecars + heartbeats
  recursive-improvement Self-improvement + scoring + safety gates
  deployment            Modes + routing + preflight
  formal-readiness      Predicates + export + Lean/Isabelle prep
  scaling               Event bus + worktree pools + tier config
  git-control           Worktree lifecycle
  robustness-policy     Parse recovery + context budgets + overlap detection
  ui-models             Canonical projection types (CD-04)
```

---

## Numbers

- **84** API routes, **68** with OpenAPI annotations
- **97** SQL tables, **48** enum CHECK constraints
- **109** tests
- **20** Rust packages + **3** services + **3** apps
- **18** database migrations
- **12** dashboard panels
- **7** LLM adapters
- **9**-condition planning gate
- **5** conflict detection classes
- **4** review templates with auto-approval
- **3** scaling tiers (standalone / clustered / distributed)

---

## License

Proprietary.


## Current status
Prototype. Full release 3/29
