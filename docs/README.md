# SIEGE documentation

The top-level `README.md` explains what SIEGE is and why the project exists.
This `docs/` folder is the deeper map: how the engine is structured, how the runtime behaves, and why the repository looks heavier than a typical agent repo.

## Start here

If you are new to the codebase, read these in order:

1. [`CONCEPTS.md`](./CONCEPTS.md) — the core nouns: objective, plan, node, task, cycle, gate, conflict, certification.
2. [`ARCHITECTURE.md`](./ARCHITECTURE.md) — the system map: apps, services, packages, data flow, and runtime responsibilities.
3. [`GATED_EXECUTION_AND_PLANNING.md`](./GATED_EXECUTION_AND_PLANNING.md) — the heart of SIEGE: hard-gated planning and decomposition.
4. [`ADAPTERS_AND_EXECUTION.md`](./ADAPTERS_AND_EXECUTION.md) — providers, roles, worktrees, dispatch, retries, and skill resolution.
5. [`FORMAL_ASSURANCE.md`](./FORMAL_ASSURANCE.md) — how SIEGE can route both planning artifacts and execution outputs through formal-claim workflows.
6. [`GOVERNANCE.md`](./GOVERNANCE.md) — reviews, conflicts, promotion discipline, and failure-aware control surfaces.
7. [`DASHBOARD_API_AND_CLI.md`](./DASHBOARD_API_AND_CLI.md) — operator surfaces.
8. [`DEPLOYMENT_AND_SCALING.md`](./DEPLOYMENT_AND_SCALING.md) — deployment modes, scaling tiers, event buses, and remote assurance routing.
9. [`STATUS.md`](./STATUS.md) — current subsystem maturity and how to evaluate the repo honestly.
10. [`ORIGINS.md`](./ORIGINS.md) — the research background and broader stack context.

## Recommended reading paths

### I want to use SIEGE as an engine

Read:

- [`ARCHITECTURE.md`](./ARCHITECTURE.md)
- [`GATED_EXECUTION_AND_PLANNING.md`](./GATED_EXECUTION_AND_PLANNING.md)
- [`ADAPTERS_AND_EXECUTION.md`](./ADAPTERS_AND_EXECUTION.md)
- [`DASHBOARD_API_AND_CLI.md`](./DASHBOARD_API_AND_CLI.md)
- [`DEPLOYMENT_AND_SCALING.md`](./DEPLOYMENT_AND_SCALING.md)

### I want to understand why SIEGE is governance-heavy

Read:

- [`GOVERNANCE.md`](./GOVERNANCE.md)
- [`FORMAL_ASSURANCE.md`](./FORMAL_ASSURANCE.md)
- [`ORIGINS.md`](./ORIGINS.md)

### I want to contribute code

Read:

- [`CONCEPTS.md`](./CONCEPTS.md)
- [`ARCHITECTURE.md`](./ARCHITECTURE.md)
- [`STATUS.md`](./STATUS.md)

## What SIEGE is not

SIEGE is not a thin chat wrapper around a task queue.

It is an orchestration engine with explicit planning contracts, runtime policy, dependency-aware decomposition, isolated execution, review and conflict surfaces, optional formal-assurance paths, and persistence around the full execution loop.

That shape makes more sense once you read the planning, governance, and origins docs together.
