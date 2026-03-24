---
name: conversation-to-plan-ingestion
description: Convert long planning conversations into durable objective, constraint, roadmap, review, and backlog state for the development-swarm IDE. Use when a user is discussing product direction, architecture, execution policy, milestone order, review rules, or certification policy and those decisions must be absorbed into structured state instead of remaining in chat only.
---

# Conversation To Plan Ingestion

Use this skill when a design or planning conversation should become durable orchestration state.

This skill exists because the product must eventually automate the exact kind of long planning conversation that is currently happening manually.

## Purpose

Turn a conversation into:

- objective updates
- constraint extracts
- design decisions
- open questions
- roadmap absorption updates
- backlog item drafts
- review requirements
- projection update intents

## Read Order

Read only what is needed, in this order:

1. `C:/Users/madab/Downloads/Project/_iteration/first/docs/ARCHITECTURE.md`
2. `C:/Users/madab/Downloads/Project/_iteration/first/docs/CONVERSATION_AUTOMATION.md`
3. `C:/Users/madab/Downloads/Project/_iteration/first/docs/PLANNING_AND_SPEC_LOOP.md`
4. `C:/Users/madab/Downloads/Project/_iteration/first/docs/ROADMAP_STEERING.md`
5. `C:/Users/madab/Downloads/Project/_iteration/first/docs/BACKLOG_ITEM_SCHEMA.md`
6. `C:/Users/madab/Downloads/Project/_iteration/first/docs/MILESTONE_EXECUTION_ORDER.md`
7. The active milestone CSV
8. `C:/Users/madab/Downloads/Project/_iteration/first/docs/REVIEW_AND_SUPERVISION.md` if review/approval is involved
9. `C:/Users/madab/Downloads/Project/_iteration/first/docs/USER_CONTROL_SURFACE.md` if execution policy changed

## Workflow

1. Summarize the current conversation in one objective-centered paragraph.
2. Extract durable constraints.
3. Extract explicit design decisions.
4. Extract unresolved questions.
5. Decide whether the conversation:
   - creates a roadmap node
   - is absorbed into an existing roadmap node
   - reprioritizes the roadmap
   - changes branch/mainline policy
   - changes execution policy
   - changes review or certification policy
6. Draft backlog item changes or additions.
7. Mark which live projections should update immediately.
8. Mark what still requires review before implementation or promotion.

## Required Output Categories

Always try to produce these categories, even if some are empty:

- `objective_updates`
- `constraint_extracts`
- `design_decisions`
- `open_questions`
- `roadmap_actions`
- `backlog_drafts`
- `review_requirements`
- `projection_updates`

## Roadmap Actions

Roadmap action types should be constrained to:

- `create_node`
- `absorb_into_node`
- `reprioritize_node`
- `defer_node`
- `reject_node`
- `no_change`

## Non-Negotiable Rules

- Do not leave important planning decisions only in raw chat.
- Do not treat conversation summary as sufficient; extract constraints and decisions separately.
- Do not silently change roadmap meaning without emitting a roadmap action.
- Do not let a conversation update canonical certification state directly.
- Do not create backlog drafts without cautions, checks, and review implications.
- Do not replay the entire conversation blindly into every downstream worker.

## What To Prefer

- durable structured state over prose memory
- roadmap absorption over duplicate roadmap nodes
- explicit unresolved questions over fake certainty
- small backlog drafts over giant unstructured task blobs
- immediate projection updates for planning state, not delayed-only updates

## Deliverable Shape

For each use, return:

1. conversation summary
2. extracted constraints
3. extracted design decisions
4. extracted open questions
5. roadmap actions
6. backlog draft items
7. review requirements
8. projection updates
