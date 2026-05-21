# Execution plans

Plans are first-class repository artifacts for work that spans multiple turns, risky refactors, or changes that require decisions to survive context resets.

## Directories

```text
docs/plans/active/     In-progress execution plans
docs/plans/completed/  Finished plans kept for design history
docs/plans/abandoned/  Superseded plans with a short reason
```

Keep small one-turn tasks out of this directory unless the plan records important design history.

## When to create a plan

Create `docs/plans/active/<slug>.md` when work has any of these properties:

- Multiple phases or expected handoffs.
- User-visible behaviour with non-trivial acceptance criteria.
- Architecture, release, Android, or ASR pipeline risk.
- A bug that needs reproduction evidence and a verification log.
- A refactor where decisions should remain discoverable after the branch merges.

## Template

```md
# <Plan title>

Status: active
Owner: agent
Started: YYYY-MM-DD
Related docs: links

## Goal

## Acceptance criteria

## Current context

## Steps

- [ ] ...

## Decisions

- YYYY-MM-DD: ...

## Verification log

- YYYY-MM-DD: command/result

## Handoff notes
```

## Lifecycle

- Update the plan as decisions are made; do not leave stale checkboxes.
- Move completed plans to `docs/plans/completed/` with final verification notes.
- Move superseded plans to `docs/plans/abandoned/` and record why.
- If a plan reveals a durable rule, update `AGENTS.md`, a relevant doc, or tooling before closing it.
