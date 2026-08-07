---
name: plan
description: Create a grounded, decision-complete plan, then stop for approval. Use ONLY when the user explicitly asks to plan — when they request a plan, design, approach, rollout/migration strategy, or PR breakdown, or invoke the plan skill (/plan or /skill plan). Do NOT use for ordinary implement, build, fix, debug, or refactor requests — when asked to make a code change, implement it directly without planning first. One rule applies whether or not you read the body: when an explicit plan divides work into separate diffs, commits, or PRs and you are asked to publish, publish that split — never silently collapse the planned units into fewer; tell the user before deviating.
---

# Plan

Create a grounded, decision-complete plan before complex work.

## Scope

- Use this skill ONLY when the user explicitly asks to plan — they request a plan,
  design, approach, rollout or migration strategy, or PR breakdown, or invoke the
  plan skill directly (/skill plan).
- Do NOT use this skill for ordinary implement, build, fix, debug, or refactor
  requests. When asked to make a code change, do the work directly without planning
  first — even when the task is complex. Task complexity alone is not a trigger; the
  explicit planning request is.
- Once planning, adapt to the plan TYPE the request implies — implementation, design,
  debugging, rollout or migration, eval or research, or PR split (see Plan Shape).
  The plan type is a separate axis from the trigger: a debugging, implementation, or
  evaluation plan type is never itself a reason to invoke the skill on an ordinary
  coding task.
- Skip this skill for simple one-step edits, obvious bug fixes, formatting,
  copy changes, and direct questions that can be answered without planning.
- Treat this as planning guidance, not a host-enforced mode. Do not claim that
  the host is blocking write tools, commands, edits, or approvals for you.
- You may write at most one plan markdown file when durable handoff is useful.
  Do not edit code, apply
  patches, format, generate code, commit, push, open PRs, or run commands whose
  purpose is to carry out the implementation while planning.
- You may run non-mutating discovery: read and search files, inspect docs and
  specs, check git status, run dry-run commands, and run tests or builds only
  when they do not change tracked files.
- Resolve facts that can be discovered locally before asking the user.
- Ask only for product preferences or tradeoffs that cannot be discovered from
  the workspace.

## Research Before Drafting

For an explicit plan whose recommended approach or validation depends on the
current workspace, research the relevant repository evidence before drafting
the recommended approach or work plan.

- Establish the current behavior, owning boundary, constraints, reuse options,
  and validation path. Cite the relevant paths or commands and mark unresolved
  facts as assumptions or open questions.
- Start with targeted search across applicable instructions, decisions, owning
  code or docs, callers, tests, configuration, and existing utilities. Stop once
  the evidence makes the plan decision-complete; do not scan the whole repository
  or spawn subagents by default.
- If you delegate research, do not draft or deliver the recommended approach or
  work plan while any research child is pending. Wait for every research child
  to reach a terminal state, receive its result, and incorporate relevant
  findings. Mark failed, cancelled, timed-out, or unavailable results as such.
- If the user explicitly asks to stop or shorten research, follow the latest
  instruction and label the remaining gaps.

## Quality Bar

A plan is ready only when it is:

- **Grounded**: cite the user goal, current facts, docs, nearby code, tests,
  commands, logs, specs, issues, or PRs that shaped the plan when those sources
  exist. Mark guesses as assumptions.
- **Decision-complete**: name the key choices, the recommended choice, and why
  reasonable alternatives were rejected when they matter.
- **Purpose-fit**: choose the plan type that matches the work: implementation,
  design, debug, migration/rollout, eval/research, or review/PR split.
- **Executable**: turn the approach into ordered work units with clear
  dependencies, owners or surfaces, and no unrelated cleanup.
- **Verifiable**: each work unit has a focused validation command or real manual
  check. Include E2E, black-box, or release-binary checks when the surface needs
  them.
- **Scope-safe**: state non-goals, risks, rollback, and compatibility impact
  when they affect the implementation.
- **Question-minimal**: include open questions only when local discovery cannot
  answer them.
- **Not just a task list**: explain the context, recommended approach, and
  evidence path clearly enough that another person can critique the plan before
  execution.

## Optional File Output

Save the plan as a user-visible markdown file only when durable handoff is
useful.

Good reasons to save a file:

- The user asks for a file or gives a path.
- The plan is complex enough to hand to another session, document, issue, PR, or
  agent.
- The plan belongs to an existing spec, docs page, or project plan convention.
- The plan needs cross-session approval, revision, or later execution.

For short or exploratory plans, present the plan inline and do not create a file.

- If the user gives a path, use that path.
- If the workspace has an established durable plan location, follow it. Prefer,
  in order: an active `specs/<feature>/plan.md` for Spec Kit work, an existing
  docs or project plan convention such as `docs/plans/`, or a documented plan
  directory.
- If saving and no stronger convention exists, save to
  `.agents/plans/YYYY-MM-DD-<slug>.md`.
- Create parent directories as needed.
- You may update an active `specs/<feature>/plan.md`, an existing
  user-specified plan path, or a plan file you saved this session. Never
  overwrite any other existing file unless the user explicitly asks. When
  creating a new dated file and the chosen file exists, add a short numeric
  suffix.
- Do not use `/tmp` for the final plan. Temporary scratch is not a durable user
  artifact.

## Approval Gate

After generating the plan, always use the `request_user_input` tool when it is
available to ask the user to `Approve`, `Request changes`, or `Cancel`. This is
a required confirmation step, not an optional follow-up or a prose-only
question.

Do not start implementation before the user explicitly approves the delivered
plan. If `request_user_input` is unavailable, present the plan and stop for an
explicit user reply instead.

## Plan Shape

Write the plan so the next person can act without guessing. Prefer this core
shape unless the task clearly needs something smaller:

```markdown
## Goal
## Success Criteria
## Context And Current Facts
## Constraints And Non-goals
## Key Decisions
## Recommended Approach
## Work Plan
## Validation Plan
## Risks / Rollback
## Open Questions
```

Write `None` for open questions only after checking the repo for answers.
Keep `Success Criteria` outcome-oriented: what must be true for the work to be
done. Keep `Validation Plan` evidence-oriented: exact focused commands, E2E or
black-box checks when relevant, expected evidence, and manual checks that
cannot be automated.

Adapt the sections to the plan type:

- For implementation work, `Work Plan` should name the code surfaces, data flow,
  compatibility impact, existing utilities to reuse, and test strategy. Add PR
  slices only when the workspace workflow or review risk calls for them.
- For design work, emphasize interfaces, invariants, alternatives rejected, and
  migration or compatibility rules.
- For debugging work, list hypotheses, observations needed, instrumentation or
  logs to inspect, reproduction steps, and the evidence that will confirm or
  falsify each hypothesis.
- For rollout or migration work, include phases, gates, rollback, data safety,
  monitoring, and user-visible impact.
- For eval or research work, include the benchmark/question, corpus or sources,
  comparison arms, success metrics, confounders, and reproducibility evidence.

Do not invent an issue, spec, PR, or file path just to fill a section. If the
workspace rules require them, cite the real item or state the missing prerequisite
as an open question or blocker. For simple work, keep sections short. For complex
work, make the plan decision-complete enough that an implementer does not need to
invent scope, interfaces, or verification.

## Workflow

1. Classify whether planning is actually needed. If the task is simple, say so
   and answer or implement under normal rules instead.
2. Complete Research Before Drafting when it applies. Ground the plan in
   available truth: user request, repo/docs/code, logs, configs, prior
   decisions, and issue/spec/PR context when it exists.
3. Identify the plan type and the smallest viable path that proves the approach.
4. Choose the approach that matches existing patterns with the least new
   abstraction or process.
5. Split work only where sequencing, risk, ownership, review, or validation
   needs a real boundary.
6. Map every work unit to validation evidence or a real manual check.
7. If saving a plan file, briefly report the saved path. If presenting inline
   only, say that no file was created.
8. Highlight the highest-risk validation step.
9. Present the plan, call `request_user_input` when available, and wait for an
   explicit user decision before implementation.

## Exit

- After generating the plan, call `request_user_input` when it is available to
  ask for `Approve`, `Request changes`, or `Cancel`; do not substitute a
  prose-only question.
- If `request_user_input` is unavailable, present the plan and stop. Do not
  treat planning text as permission to edit.
- If the user approves, says go, continue, implement, or otherwise starts
  execution, leave planning and do the work under the normal workspace rules,
  carrying the plan's structural commitments (see Executing The Plan).
- Re-check the latest user message before editing so stale plan assumptions do
  not override a new instruction.

## Executing The Plan

These rules bind during execution whether the plan came from this skill or was
supplied by the user, and whether or not this skill was ever invoked. If you
loaded this skill at publish time, only this section applies — the planning
restrictions above do not.

- A plan's structural commitments survive into execution: the diff/commit/PR
  split and the unit ordering stay binding while the work is carried out and
  published.
- This governs the shape of publication, never its authorization: committing,
  pushing, or publishing still needs the user's ask, per source-control
  safety. A plan step that says "commit" does not by itself authorize a
  commit.
- When the user has you publish work the plan divides into N separate diffs,
  commits, or PRs, publish exactly that split: commit each planned unit
  separately, in the planned order, even when one combined commit would
  satisfy the literal request ("commit this", "publish it").
- Work already finished in one mixed working tree still gets split at publish
  time: group the changed files by plan unit and commit unit by unit. If one
  file's changes span units, split at the hunk level (`git add -p`) or assign
  the file to the earliest unit and say so.
- A later explicit user instruction about the structure supersedes the plan —
  follow it; the deviation notice is for deviations you initiate.
- Deviate from the planned structure — collapse, merge, reorder, or skip
  units — only after telling the user what you are changing and why, before
  publishing a different structure than the plan promised.
