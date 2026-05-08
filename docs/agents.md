# Agent dev loop — design notes

Module of [`AGENTS.md`](../AGENTS.md). Roster, runbook, and prohibitions live there. This file holds the _why_. For HMR/CDP mechanics see [`dev-loop.md`](dev-loop.md).

## Why four agents

The four verbs are mutually exclusive: **answer** (read-only), **mutate** (writes files), **gate+commit** (writes git history), **install/test on device** (touches devices). A task maps to exactly one verb; selection is not a judgement call.

Earlier rosters split each verb further (`scout`/`triage`/`researcher` all answered; `coder`/`docs-updater` both mutated). Per Anthropic's writing-tools-for-agents guidance, overlapping responsibilities create selection ambiguity and degrade outcomes. Consolidation traded fine-grained personas for an unambiguous routing table.

## Why the answer agent never edits

Diagnosis context is wider than fix context — investigating an OOM means tailing logcat, running CDP probes, walking `git log`. Letting the same agent edit produces drive-by patches that "fix" the wrong thing. `wt-investigate` observes; `wt-edit` applies the spec.

## Why ship is its own agent

The pre-commit hook is mandatory and `--no-verify` is forbidden. `wt-ship` exists to make that contract enforceable: an edit that breaks something else still gets pushed when the same context rationalised both, so we always re-gate via a fresh `wt-ship` invocation that has no edit memory.

## Why filesystem, not events

Async subagents cannot wake the parent on completion (anthropics/claude-code#20921, openai/codex#15723). Workers signal through append-only files under `tmp/`: `investigate-<slug>.md`, `edit-report.json`, `last-commit.json`, `install-report.json`, `test-report.json`. Main thread polls between turns. Never pipe one agent's stdout into another — the file artefact is the contract and the only replay surface.

## One file, one owner per parallel run

If two `wt-edit` invocations could touch the same file, serialise them or use git worktrees (`worktree: true` in parallel subagent calls). Every invocation must leave an artefact — replay is otherwise impossible.
