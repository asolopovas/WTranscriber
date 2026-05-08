# Agent dev loop - design notes

Module of [`AGENTS.md`](../AGENTS.md). Roles, runbook, coordination rules, and prohibitions live there. This file holds only the _why_ - the design constraints that shaped them. For HMR/CDP mechanics see [`dev-loop.md`](dev-loop.md).

## Why three executors, not one

`wt-coder` (edits) · `wt-committer` (gate + push) · `wt-triage` (forensics). Sharp boundaries - overlap is a decomposition bug.

- **Coder must not commit.** A fix that breaks something else still gets pushed when the same context rationalised both. Always re-gate via committer.
- **Committer must run the gate.** `.githooks/pre-commit` is mandatory; the committer's job is to hit it and never bypass with `--no-verify`.
- **Triage must not edit.** Diagnosis context is wider than fix context; mixing them produces drive-by patches. Triage observes; coder fixes.

## Why filesystem, not events

Claude Code / Codex async subagents **cannot wake the parent on completion** (anthropics/claude-code#20921, openai/codex#15723). Workers therefore signal through append-only files under `tmp/` (contract listed in AGENTS.md → Coordination rules). Main thread polls between turns; new lines → `wt-triage` with the _excerpt only_.

Never pipe one agent's stdout into another. The file artifact is the contract - it's also the only replay surface.

## One file, one owner per parallel run

If two coders could touch the same file, serialise them or use git worktrees (`worktree: true` in parallel subagent calls). Every invocation must leave an artifact (`tmp/*.json`, commit hash, verdict) - replay is otherwise impossible.
