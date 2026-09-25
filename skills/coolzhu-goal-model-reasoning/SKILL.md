---
name: coolzhu-goal-model-reasoning
description: Constrain COOLZHU Goal model reasoning to the requested scope, available context, retry budget, and evidence standard. Use for every Goal phase and especially long-running GLM sessions, retries, context compression, and completion review.
---

# COOLZHU Goal Model Reasoning

Reason from the current phase contract, not from broad project opportunities.

## Establish scope

Before changing anything:

1. Restate the requested outcome and allowed files or modules.
2. List explicit forbidden areas when the task is narrow.
3. Inspect current code and `git diff` before proposing a change.
4. Prefer the smallest implementation that satisfies the phase.

Do not modify unrelated memory, browser, realtime audio, TTS, diagnostics, tests, or UI modules merely because they are nearby. Do not change contract tests to hide an out-of-scope implementation.

## Manage context and retries

- Use the current room history, supplied progress block, and loaded Skill guidance as authoritative context.
- When context approaches its configured threshold, preserve decisions, exact paths, errors, and pending verification in the compacted summary.
- Keep reasoning and output inside the configured token budgets. Do not spend the response budget narrating routine steps.
- Diagnose a failure before retrying. Do not repeat the same failing action unchanged.
- Stop after three equivalent failed attempts and report the concrete blocker, unless a new diagnosis provides a materially different action.

## Evidence standard

- Treat model self-report as unverified.
- A successful build is not proof that `#[cfg(test)]` code compiles; run the relevant tests.
- A green test run is not proof of correct scope if tests were changed. Inspect `git diff` and the changed-file list.
- Do not claim completion from planning, directory creation, placeholder files, or partial output.
- Report exact commands, exit codes, artifact paths, and remaining uncertainty.

## Final reasoning check

Before completion, verify that:

- Only allowed files changed.
- No test was weakened to accommodate the change.
- Required artifacts are meaningful and present.
- The phase verification signal passed independently.
- Any context compression retained the next action and failure evidence.
