---
name: spec-locked-implementation
description: Implement the next piece of the development-swarm IDE only after checking the product spec, milestone order, and backlog item guards. Use when working inside `_iteration/first` so implementation follows the locked design instead of improvising boilerplate or skipping architecture constraints.
---

# Spec Locked Implementation

Use this skill when implementing code for the new development-swarm IDE in `_iteration/first`.

The purpose is to prevent three failure modes:

- writing boilerplate that ignores the product architecture
- implementing out of milestone order
- coding before the current backlog item's cautions and checks are understood

## Read Order

Read only what is needed, in this order:

1. `C:/Users/madab/Downloads/Project/_iteration/first/docs/ARCHITECTURE.md`
2. `C:/Users/madab/Downloads/Project/_iteration/first/docs/MILESTONE_EXECUTION_ORDER.md`
3. `C:/Users/madab/Downloads/Project/_iteration/first/docs/MILESTONE_PLAN.csv`
4. The active milestone CSV for the current task
5. `C:/Users/madab/Downloads/Project/_iteration/first/docs/BACKLOG_ITEM_SCHEMA.md`
6. `C:/Users/madab/Downloads/Project/_iteration/first/docs/WRITE_DISCIPLINE.md`
7. `C:/Users/madab/Downloads/Project/_iteration/first/docs/STATE_AND_STORAGE.md`
8. `C:/Users/madab/Downloads/Project/_iteration/first/docs/USER_CONTROL_SURFACE.md`
9. `C:/Users/madab/Downloads/Project/_iteration/first/docs/TECH_STACK.md`
10. Only then read the code files you need to change

If the task touches a special area, then also read:

- roadmap changes: `ROADMAP_STEERING.md`
- review/approval changes: `REVIEW_AND_SUPERVISION.md`
- external agent adapters: `ADAPTER_RUNTIME_INVARIANTS.md`
- remote certification/update: `REMOTE_CERTIFICATION_AND_UPDATES.md`
- conversation ingestion: `CONVERSATION_AUTOMATION.md`

## Workflow

1. Identify the current milestone and exact backlog item ids.
2. Confirm the task belongs to the active milestone from `MILESTONE_EXECUTION_ORDER.md`.
3. Read the corresponding milestone CSV row(s).
4. If the row is missing critical fields like `goal`, `dependencies`, `proof_or_check_hooks`, `cautions`, or review policy, stop and fill those fields first instead of coding blindly.
5. Restate the implementation target in terms of:
   - state change
   - write path
   - projections
   - user policy impact
   - certification impact
6. Implement the smallest vertical slice that satisfies the row.
7. Validate with the checks implied by the row.
8. Update backlog status and note what remains blocked.

## Non-Negotiable Rules

- Do not jump ahead to a later milestone because it feels easier.
- Do not write UI-first code that invents state shape ad hoc.
- Do not bypass the authoritative write path.
- Do not invent a second canonical certification authority.
- Do not add backend-specific proof/runtime behavior into the orchestration core.
- Do not introduce large generic frameworks when a smaller slice will do.
- Do not add filler boilerplate just to make the repo look complete.

## What To Prefer

- vertical slices over horizontal framework churn
- schema-first over handler-first
- command/event/projection discipline over direct mutation
- explicit enums and lifecycle states over stringly-typed blobs
- generated types or shared schemas over duplicated interfaces

## Deliverable Shape

For each implementation task, produce:

1. the exact backlog item ids addressed
2. the files changed
3. the checks run
4. any remaining blockers or cautions
5. whether the task is ready for auto-approval, manual review, or further work
