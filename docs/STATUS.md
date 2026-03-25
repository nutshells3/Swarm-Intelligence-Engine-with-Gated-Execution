# Status and maturity

This file is the honest reading guide for the repository.

SIEGE is not a toy repo, and it is not just a concept sketch. It has real orchestration machinery. At the same time, it is still an actively evolving engine, so different subsystems are at different levels of finish and polish.

## Maturity by subsystem

### Strong today

These are central to the identity of the engine and should be treated as core implemented capabilities:

- gated planning and typed plan validation,
- dependency-aware decomposition,
- adapter registry and multi-backend routing,
- isolated git worktree execution,
- explicit policy snapshots and per-role model binding,
- review governance and conflict modeling,
- certification / formal-assurance seams,
- plan-to-formal export and planning-oriented validation paths,
- control-plane mutation discipline with durable event recording,
- scaling and deployment abstractions.

### Active and expanding

These areas are real and valuable, but the operator experience or modular boundaries may continue to evolve:

- some dashboard panels and UI workflows,
- execution/runtime modularization and plugin-style composition,
- deeper external integration surfaces,
- documentation counts, screenshots, and operational examples,
- benchmarked scale claims under real provider budgets.

### How to evaluate the repo fairly

The right way to evaluate SIEGE is not:

- “does every screen look polished,” or
- “is this a tiny framework I can learn in five minutes.”

The right questions are:

- does the engine have a real planning contract,
- does it preserve dependency structure,
- does it isolate parallel execution,
- does it model governance explicitly,
- does it offer formal-assurance paths,
- does it have a credible route to larger-scale orchestration.

## What SIEGE should currently be called

A good description today is:

**a research-born orchestration engine with real runtime machinery and active subsystem expansion**.

For GitHub positioning, it is also reasonable to call it:

**a multi-agent orchestration engine with gated execution**.

Both descriptions are true. The first is more candid about origin and maturity; the second is better for top-level positioning.

## Expectations for users

If you are evaluating SIEGE for real use, expect:

- a stronger planning model than most agent repos,
- more governance and control-plane concepts than most agent repos,
- richer architecture and more packages than a minimal framework,
- better support for reasoning about orchestration state,
- a system that rewards deeper setup and reading.

## Expectations for contributors

If you contribute to SIEGE, expect to preserve these design principles:

- planning is a hard gate,
- policy lives in typed control surfaces,
- conflict history is durable,
- reviews are not disposable,
- formal assurance is routeable, not hacked in,
- scaling should not require rewriting the conceptual model,
- docs should explain both what exists and why it exists.
