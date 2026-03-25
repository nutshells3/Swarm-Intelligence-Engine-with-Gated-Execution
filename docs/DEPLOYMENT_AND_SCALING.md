# Deployment and scaling

SIEGE was designed with swarm-style execution in mind.

That does not mean every deployment should start large. It means the architecture is meant to scale from local development to multi-node execution without changing the conceptual model.

## Local development

For local development, the typical stack is:

- PostgreSQL,
- orchestration API,
- loop runner,
- worker dispatch,
- web dashboard,
- optional CLI.

This is enough to exercise the full orchestration loop on one machine.

## Deployment modes

The deployment layer makes assurance and compile routing explicit.

The main deployment modes are:

- **local_only** — certification and compile work stay local,
- **local_plus_remote** — local is primary but remote endpoints may be used,
- **remote_certification_preferred** — remote assurance is preferred, local is fallback,
- **certification_disabled** — formal certification is disabled for dev/test style runs.

The important design choice is that deployment mode is typed and explicit. It is not supposed to be hidden in random environment assumptions.

## Scaling tiers

The scaling package models three tiers:

- **standalone** — single deployment footprint, simplest setup,
- **clustered** — shared database with pooled worktrees and multiple cooperating workers,
- **distributed** — stronger event-bus topology and sharded execution patterns.

This lets the same engine reason about growth without rewriting the whole app around a different scheduler every time.

## Event buses

SIEGE’s scaling layer can abstract over different event-bus styles.

Typical choices include:

- PostgreSQL-backed event coordination for smaller setups,
- NATS-backed messaging for larger or more distributed setups.

That separation matters because the orchestration logic should not care whether a message moved through SQL polling or a dedicated bus.

## Worktree pools and worker isolation

At scale, worktree management becomes a real systems concern.

SIEGE includes scaling-aware isolation concepts such as:

- pooled worktree isolation,
- worker isolation abstractions,
- tier-specific configuration for pool size and sharding.

This is part of why the engine can target swarm execution more seriously than “open one repo and hope the agents cooperate.”

## Remote assurance and compile endpoints

Some deployments will want heavy formal or compile work off the local box.

SIEGE’s deployment layer is designed to support remote certification and compile endpoints while still preserving typed routing decisions and clear error semantics.

## Configuration knobs

Important knobs typically include:

- scaling tier,
- database pool size,
- worktree pool size,
- shard id / shard count,
- NATS URL,
- formal-claim remote endpoint configuration,
- local vs remote certification routing.

## A realistic way to talk about scale

SIEGE is designed for high-parallel orchestration. That is an architectural statement.

Actual throughput still depends on:

- provider rate limits,
- model spend,
- available hardware,
- worktree / disk pressure,
- network topology,
- review and certification policy,
- the cost of the tasks themselves.

So the right framing is:

- SIEGE is **architected for large-scale swarm execution**,
- real performance must still be validated under your budget, hardware, and provider mix.

## Operational recommendation

Start with standalone mode.
Then graduate to clustered or distributed operation only after you understand:

- your adapter mix,
- your review / certification pressure,
- your worktree churn,
- your event volume,
- your bottlenecks in integration and assurance.

Scaling a swarm runner is easy.
Scaling a governed orchestration engine is harder.
The point of SIEGE is to make that harder problem tractable without throwing away structure.
