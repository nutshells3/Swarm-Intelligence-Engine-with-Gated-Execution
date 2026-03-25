# Adapters and execution

SIEGE is designed to route work across different providers, roles, and execution surfaces.

This matters because planning, implementation, review, debugging, and formalization do not all benefit from the same backend.

## Adapter model

The adapter layer exists to normalize heterogeneous backends behind a common orchestration model.

Typical targets include:

- Claude Code CLI,
- Codex CLI,
- Anthropic API,
- OpenAI-compatible APIs,
- local OpenAI-compatible endpoints,
- custom stdin/stdout CLIs,
- mock/demo adapters.

The engine can auto-detect available adapters from the environment and register capabilities accordingly.

## Provider modes

At the policy level, SIEGE can distinguish between different provider modes such as:

- **API** — direct HTTP or SDK-style model access,
- **session** — session-oriented or CLI-oriented execution,
- **local** — self-hosted or localhost model endpoints.

This matters because execution characteristics differ. A CLI agent can read and mutate a repo directly. A hosted API model usually needs a different prompt and execution boundary.

## Role-specific routing

SIEGE’s policy model treats role binding as a first-class concern.

Typical roles include:

- planner,
- implementer,
- reviewer,
- debugger,
- research,
- formalizer A,
- formalizer B.

That lets the engine do things like:

- use one backend for planning and another for coding,
- keep the reviewer separate from the implementer,
- reserve expensive models for only the most valuable phases,
- route formalizer work through a distinct assurance configuration.

## Example policy shape

```json
{
  "global": {
    "default_provider_mode": "api",
    "default_model_family": "general",
    "max_active_agents": 8,
    "default_concurrency": 4,
    "default_retry_budget": 3,
    "certification_routing": "critical_only"
  },
  "planner": {
    "provider_name": "anthropic",
    "model_name": "claude-sonnet"
  },
  "implementer": {
    "provider_name": "codex",
    "provider_mode": "session"
  },
  "reviewer": {
    "provider_name": "openai",
    "model_name": "gpt-4o"
  },
  "formalizer_a": {
    "enabled": true,
    "mode": "required",
    "certification_frequency": "critical_only",
    "binding": {
      "provider_name": "formal-claim"
    }
  }
}
```

The exact values will vary by deployment, but the point is that routing decisions belong in policy, not in random conditional logic buried in worker code.

## Worktree isolation

One of SIEGE’s strongest execution primitives is git worktree isolation.

Instead of letting multiple agents push into one mutable workspace, the engine can assign isolated worktrees per task or lane.

Benefits:

- reduced file-level contention,
- clearer integration boundaries,
- easier branch-level inspection,
- safer concurrency under real repo mutation.

## Tasks, attempts, and retries

A task is the dispatch unit.
An attempt is a specific run of that task.

Retries are part of the runtime model, not an afterthought. This makes it possible to:

- track which failures are transient,
- keep retry budgets explicit,
- avoid infinite blind repetition,
- learn from failed attempts in later cycles.

## Health and fallback

The adapter registry is not just a list. It can support capability and health awareness.

That matters because “preferred provider unavailable” should not necessarily mean “entire system down.” It may mean “fall back to the next healthy adapter allowed by policy.”

## Skills and worker shaping

SIEGE also supports worker shaping through the skill registry and review / task templates.

This is important because execution quality depends on more than choosing a model. It also depends on:

- the role’s expected inputs,
- the artifact schema it should produce,
- the constraints and context it receives,
- the kind of output downstream systems can consume safely.

## How to think about execution in SIEGE

A good mental model is:

- adapters decide **who can execute**,
- policy decides **who should execute what**,
- tasks decide **what needs to be attempted**,
- worktrees decide **where that attempt can safely mutate code**,
- reviews / conflicts / certification decide **whether the result can advance**.

That separation is what lets SIEGE scale beyond “one prompt, one repo, one branch, one loop.”
