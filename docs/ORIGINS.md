# Origins: why SIEGE exists

SIEGE did not begin as “let’s make one more agent framework.”

It began as infrastructure for a larger line of work concerned with structured reasoning, structured failure, and orchestration at a scale where ad hoc prompting stops being enough.

## The motivating problem

A strong language model can often produce a good answer.

But there is a difference between:

- producing a plausible answer,
- expressing confidence,
- and explicitly representing what that answer depends on.

The broader research motivation behind SIEGE was the idea that a system should be able to preserve more structure around claims, dependencies, assumptions, ambiguity, and failure than ordinary text-only pipelines do.

## Why an orchestration engine was needed

If you want to decompose large bodies of work into structured artifacts — engineering objectives, claim graphs, formalizable fragments, robustness slices, reviewable units — you need orchestration infrastructure.

Not one agent. Not one loop. Infrastructure.

That infrastructure needs to handle things like:

- explicit planning,
- large fan-out,
- dependency-aware decomposition,
- isolated execution,
- policy-controlled routing,
- preserved provenance,
- review and conflict handling,
- optional formal-assurance paths.

SIEGE was built because those requirements are hard to satisfy with a minimal “one chat session plus a task queue” architecture.

## The broader stack context

SIEGE is best understood as one layer in a broader research-derived stack.

In that broader picture:

- **SIEGE** is the orchestration layer.
- **OAE** is the normalization and assurance layer that can turn natural-language material into more canonical, auditable artifacts and route sensitive material through stricter flows.
- **SafeSlice** is the robustness-oriented layer concerned with how outputs degrade as structure changes.
- **FWP and related formal backends** provide formal protocol and proof-assistant seams.

This document matters because it explains why SIEGE includes concepts that seem “too heavy” for a normal agent repo: the repo inherited real design pressure from a bigger research program.

## Why the engine is gate-heavy

If the long-term goal involves structured artifacts, claim decomposition, robustness analysis, or formalization, then planning quality is not optional.

You cannot safely fan out thousands of attempts from a bad plan and hope governance later will save you.

That is why SIEGE hardens planning into a gate.

## Why the engine is artifact-heavy

The broader research program cared not just about whether something worked, but about:

- what it depended on,
- how it failed,
- what evidence contradicted it,
- where ambiguity remained,
- whether a stronger assurance path was required.

That pressure is why SIEGE treats reviews, conflicts, certifications, and event history as native runtime objects.

## Planning assurance changed the picture further

Originally, a lot of assurance-heavy systems validated outputs after the work was already done.

SIEGE now also supports routing planning artifacts into formal-readiness and formal-claim style validation paths. That means assurance can begin earlier, before execution is fully unleashed.

For research-derived infrastructure, that is a meaningful shift. It makes the planning phase itself subject to stronger scrutiny.

## What this repo is and is not

This repository is the orchestration engine.

It is not the entire research stack in one folder, and it is not trying to be a standalone theorem prover, benchmark lab, or data-generation platform by itself.

What it does provide is the runtime control layer that makes the larger ambition operationally plausible.

## Why keep this context in the docs

The main `README.md` should sell the engine clearly and directly.
That is correct.

But the origins matter because they explain the design vocabulary of the repo:

- gates,
- formal-readiness,
- certification routing,
- conflict artifacts,
- review governance,
- recursive improvement with safety constraints,
- scaling-aware isolation.

Without the research context, those features can look arbitrary.
With the context, they look like what they are: deliberate responses to a bigger systems problem.
