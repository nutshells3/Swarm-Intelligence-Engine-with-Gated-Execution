# Architecture overview

SIEGE is organized as a layered orchestration engine rather than a single monolithic service.

At a high level, the repository has four visible layers:

1. **operator surfaces** — web, desktop, CLI,
2. **runtime services** — API, loop runner, worker dispatch,
3. **packages** — typed logic for planning, policy, governance, formal assurance, scaling, and projections,
4. **state and infra** — PostgreSQL, event journal, worktrees, optional remote assurance endpoints, optional event bus backends.

## Topology

```text
apps/
  web/              Dashboard and operational UI
  desktop/          Tauri shell
  cli/              Interactive REPL

services/
  orchestration-api/    HTTP API, Swagger, operator endpoints, event surfaces
  loop-runner/          cycle state machine and orchestration ticks
  worker-dispatch/      adapter execution, worktree coordination, dispatch flow

packages/
  state-model           core orchestration types
  control-plane         commands, executor, mutation discipline
  planning-engine       plan schemas, gate logic, validation
  conversation-engine   chat extraction and plan updates
  agent-adapters        Claude / Codex / API / local / custom / mock adapters
  git-control           worktree lifecycle and branch metadata
  skill-registry        skill resolution and pack metadata
  review-governance     review artifacts, scheduling, templates, digests
  conflict-system       conflict classes, triggers, adjudication records
  integration           certification and formal-claim gateway seams
  formal-readiness      plan/export predicates and readiness checks
  robustness-policy     certification grades, thresholds, safety policy
  recursive-improvement iteration scoring, failure memory, anti-gaming rules
  deployment            routing modes for local vs remote assurance / compile work
  scaling               event buses, pooled worktrees, tier-specific config
  observability         metrics, heartbeats, sidecars, retention helpers
  ui-models             canonical projection shapes for operator surfaces
  user-policy           runtime execution policy snapshots
  roadmap-model         planning and roadmap structures
  worker-protocol       worker envelopes and messaging
```

## How the runtime moves

SIEGE’s happy path looks like this:

```text
objective
  -> conversation extraction
  -> plan elaboration
  -> 10-condition planning gate
  -> decomposition into node graph
  -> task creation and dispatch
  -> isolated execution in worktrees
  -> integration / testing / promotion
  -> reviews / conflicts / certification
  -> next cycle with learned context
```

The important part is that the steps above are not informal habits. They are modeled as runtime concerns with durable state.

## State model and write discipline

The state layer is designed around authoritative mutations plus durable event history.

At a practical level, the control-plane executor follows a simple discipline:

1. check idempotency,
2. apply the mutation,
3. append an `event_journal` record,
4. commit inside the caller’s transaction boundary.

That is what lets the API, loop runner, and projections speak the same language instead of each inventing their own side effects.

## Package groups

### 1. Core orchestration model

These packages define the nouns and control surfaces of the engine:

- `state-model`
- `control-plane`
- `user-policy`
- `ui-models`

### 2. Planning and decomposition

These packages turn an objective into an executable structure:

- `conversation-engine`
- `planning-engine`
- `roadmap-model`

### 3. Execution and isolation

These packages turn structure into running work:

- `agent-adapters`
- `worker-protocol`
- `git-control`
- `skill-registry`
- `worker-dispatch` service

### 4. Governance and assurance

These packages stop swarm execution from becoming opaque chaos:

- `review-governance`
- `conflict-system`
- `integration`
- `formal-readiness`
- `robustness-policy`
- `recursive-improvement`

### 5. Deployment and scaling

These packages let the same orchestration model operate under different runtime topologies:

- `deployment`
- `scaling`
- `observability`

## Why the repo is package-heavy

SIEGE was not built as a thin task runner. It was designed around explicit boundaries:

- planning should be typed and checkable,
- execution should be isolatable,
- policy should be mutable without rewriting core logic,
- conflicts and reviews should be preserved as artifacts,
- formal assurance should be a routeable seam rather than a one-off script,
- scaling should not require rewriting application code.

That is why the repository looks more like an engine or research platform than a prompt app.

## What to read next

After this file, the two most important docs are:

- [`GATED_EXECUTION_AND_PLANNING.md`](./GATED_EXECUTION_AND_PLANNING.md)
- [`FORMAL_ASSURANCE.md`](./FORMAL_ASSURANCE.md)

Those two explain why SIEGE’s runtime model is stricter than most multi-agent stacks.
