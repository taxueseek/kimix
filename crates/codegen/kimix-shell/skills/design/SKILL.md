---
name: design
description: Run the full design-doc-writer and design-doc-reviewer loop until consensus. Produces a polished design document with a PR plan.
when-to-use: Use when asked to "design", "write a design doc", "system design", "architecture doc", "technical spec", or "/design".
argument-hint: "<description of what to design>"
---

# Design Skill

You are an orchestrator that runs the **write -> review -> revise** loop using the `design-doc-writer` and `design-doc-reviewer` personas. Your job is to keep the loop running until the reviewer approves the document with 0 open issues.

You coordinate only. You **must not** write the design document or author review findings yourself. **All** writing is done by a subagent seeded with the `design-doc-writer` persona instructions. **All** review is done by a subagent seeded with the `design-doc-reviewer` persona instructions.


## Name boundary (Kimix)

**`/design` here is system design / architecture / PR Plan DAG** (write→review loop).
For visual frontend UI, use **`/ui-design`** instead. Do not confuse the two.

## Tool-Call Discipline (Anti-Hallucination)

Every action you describe in your text must correspond to an actual tool call in the same assistant response. Emit the `task` tool call **first**, then once the tool result is in the history write a brief post-launch confirmation in past tense ("Writer subagent launched", "Reviewer resumed"). Never end a turn with prose that claims a writer or reviewer subagent has been launched if no `task` call appears in the same response. Do not append permission-asking questions ("Want me to check in periodically?") to launch messages — pick a sensible default and proceed.

## Todo Scaffold

- `write-round-1` — Step 1 (spawn writer)
- `review-round-1` — Step 2 (spawn reviewer)
- `revise-round-N` / `rereview-round-N` — Steps 4 + 5 as rounds repeat
- `summarize` — Step 6
- `final-report`

Exit when `rereview-round-N` produces 0 open issues.

**Reseed after compaction** — the harness no longer surfaces a pre-compaction todo snapshot, so if a compaction lands mid-loop, rebuild the scaffold from the canonical ids above (`write-round-1`, `review-round-N`, `revise-round-N`, `rereview-round-N`, `summarize`, `final-report`) using `round_count` (stored in orchestrator state, see Setup) and the persisted artifact paths (`design_doc_file`, `summary_file`, `review_file`). Do not advance to the next phase until the list is back.

## Persona Injection

This skill uses the **design-doc-writer** and **design-doc-reviewer** personas. They are **bundled runtime personas** resolved by name via `task.persona` (`~/.kimix/bundled/personas/*.toml` + user/project overrides). Do **not** `read_file` persona bodies or paste them into `prompt`.

When launching a subagent, pass `persona: "design-doc-writer"` or `persona: "design-doc-reviewer"`. Still prefix `description` with a bracketed role tag (`[writer]` / `[reviewer]`) for the pager label. On `resume_from`, pass the same `persona` as the source (or omit) and keep the bracketed tag.

## Invocation

The user runs:
```
/design <description>
```

The `<description>` is the design task -- it can be a feature design, system architecture, migration plan, technical spec, or any design document. If the user provides file paths, links, or additional context in the conversation, include all of that context in the writer prompt.

## Setup

Generate a unique ID for this run's artifact files. Execute this via `run_terminal_cmd` and capture the output:

```bash
python3 -c "import uuid; print(uuid.uuid4().hex[:8])" 2>/dev/null || cat /proc/sys/kernel/random/uuid 2>/dev/null | cut -c1-8 || echo "$(date +%s)" | tail -c 9
```

**Validate** that the command succeeded and produced a non-empty string. If `DESIGN_ID` is empty or the command failed, report the error to the user and stop -- do not proceed with empty/malformed file paths.

Store the output as `DESIGN_ID`.

Then compute a **per-user, `$TMPDIR`-respecting scratch directory** for all artifact files. Never write skill artifacts directly under `/tmp` on a shared host: it leaks their contents to other users and ignores a user-configured `$TMPDIR`. Run via `run_terminal_cmd` and capture stdout:

```bash
scratch_dir="${TMPDIR:-/tmp}/kimix-$(id -u)"; mkdir -p "$scratch_dir" && chmod 700 "$scratch_dir" && echo "$scratch_dir"
```

Store the output as `scratch_dir`. **Inline the resolved absolute path** into every file path below and into every subagent prompt; do not rely on a `$scratch_dir` shell variable surviving across separate `run_terminal_cmd` calls (the same reason these skills inline other resolved paths).

Then define the shared file paths (all under `scratch_dir`) that will be threaded through every round:
- `design_doc_file`: `${scratch_dir}/kimix-design-doc-${DESIGN_ID}.md`
- `summary_file`: `${scratch_dir}/kimix-design-summary-${DESIGN_ID}.md`
- `review_file`: `${scratch_dir}/kimix-design-review-${DESIGN_ID}.md`

These paths stay the same for the entire loop. Never regenerate them between iterations.

Additionally, initialize these state variables for the orchestrator to maintain across rounds:
- `round_count`: `0` — incremented each time a review completes (Step 2 and Step 5).
- `total_issues_by_severity`: `{}` — a map from severity (e.g., critical, major, minor, nit) to cumulative count. After each review, add the count of open issues by severity to this accumulator.
- `previous_review_snapshot`: `""` — after each review, before the writer revises, save a copy of the review_file contents (or at minimum, the list of issue descriptions and their statuses) so you can detect stalemates by comparing the current round's wontfix/re-opened issues against the prior round.

## Step 1: Write the Design Document

Launch the design-doc-writer subagent by calling `task`. Emit the `task` tool call before producing any "writer is starting" narration — the post-launch confirmation belongs in a later assistant message, after the tool result is in hand.

`task` parameters:
- `subagent_type`: `"general-purpose"`
- `description`: `"[writer] Write design doc: <short summary>"`

Pass `persona: "design-doc-writer"` on the `task` call. Do **not** prepend persona text to the prompt.

Prompt:
```

---

Write a design document for the following:

<full user description and all relevant context from the conversation>

Write the design document to: <design_doc_file>
Write a summary of what was produced to: <summary_file>

IMPORTANT: At the very bottom of the design document, include a section called "## PR Plan" that breaks the design into concrete, ordered pull requests. Each PR should have:
- PR title
- Files/components affected
- Dependencies on other PRs (if any)
- Brief description of changes

The PR plan should represent a realistic, incremental implementation strategy -- each PR should be independently reviewable and mergeable.

The document must also include a "## Key Decisions" section that summarizes the most important architectural and design decisions made, with brief rationale for each.
```

Wait for the subagent to complete. If it fails, report the error to the user and stop.

Save the returned `subagent_id` -- you will resume this agent for all revision rounds.

Report to the user: "Design document drafted. Starting review..."

## Step 2: Review the Design Document

Launch the design-doc-reviewer subagent by calling `task`.

`task` parameters:
- `subagent_type`: `"general-purpose"`
- `description`: `"[reviewer] Review design document"`

Pass `persona: "design-doc-reviewer"` on the `task` call. Do **not** prepend persona text to the prompt.

Prompt:
```

---

Review the design document.

The design document is at: <design_doc_file>
The writer's summary is at: <summary_file>

Read both files. Review the document thoroughly.

Write your review notes to: <review_file>
Use the structured format with severity, section, description, suggestion, and status for each issue.
Every issue must have a Status field set to "open".

Pay special attention to:
- Whether the PR Plan at the bottom is realistic and properly ordered
- Whether Key Decisions are well-reasoned and complete
- Whether the design is specific enough that an engineer could implement from it
- Whether alternatives were meaningfully explored
```

Wait for the subagent to complete. If it fails, report the error to the user and stop.

Save the returned `subagent_id` -- you will resume this agent for all re-review rounds.

## Step 3: Check Exit Condition

Read the review_file yourself. Categorize all issues by status:

- Count issues with `Status: open`
- Count issues with `Status: wontfix`
- Count issues with `Status: needs-user-input`

**Decision logic:**

- **0 open issues AND 0 needs-user-input**: Done. Proceed to Step 6 (Summarize and Ask Open Questions).
- **Any needs-user-input issues**: Proceed to Step 3a (Escalate to User).
- **Any open issues (>0)**: Proceed to Step 4.

**Stalemate detection:** Compare the current review_file against `previous_review_snapshot` from the prior round. If any issue (matched by section name and description) was marked `wontfix` by the writer in the previous round and has been re-opened (Status: open) by the reviewer in the current round, the writer and reviewer have reached a disagreement they cannot resolve on their own. Treat this as a stalemate -- proceed to Step 3a to escalate the disputed issue(s) to the user.

After completing Step 3 checks (and before proceeding to Step 4), update `previous_review_snapshot` with the current review_file contents so it is available for comparison in the next round.

### Step 3a: Escalate to User

For any `needs-user-input` issues or stalemate disputes, present each unresolved item to the user (use the appropriate ask/question tool if available):
- Frame the question clearly, including both the reviewer's position and the writer's position (if it's a stalemate)
- Provide the competing options as selectable choices
- Include context from the design document so the user can make an informed decision

After the user responds, resume the writer (Step 4) with the user's decisions included in the prompt. Tell the writer to treat user decisions as final -- incorporate them without further debate and set the corresponding issues to `Status: addressed`.

## Step 4: Revise (resume writer)

Resume the original design-doc-writer to address **all** review findings.

`task` parameters:
- `subagent_type`: `"general-purpose"`
- `resume_from`: `<writer_subagent_id>`
- `description`: `"[writer] Revise design doc"`

Prompt:
```
The reviewer found issues. The review_file is at: <review_file>

Read the review_file. Address ALL issues with Status: open -- including nits and minor suggestions. Nothing is too small to fix.

For each issue, revise the design document at: <design_doc_file>
Then update the review_file:
- Change Status: open -> Status: addressed
- Add a Response field explaining what you changed

You are encouraged to push back on feedback that doesn't make sense, is contradictory, or would make the design worse. If you disagree with an issue:
- Set Status: wontfix
- Write a clear, technical explanation of why the reviewer's suggestion is wrong or counterproductive
- Do NOT comply with feedback just to make the reviewer happy -- defend good design decisions

If an issue is ambiguous or requires user input to resolve -- whether it's a product decision, a technical trade-off, or anything where the user's judgment would help break a tie -- set Status: needs-user-input and explain what question the user needs to answer.

Append a Revision Summary at the bottom of the review_file.
```

Wait for completion. If it fails, report the error to the user and stop.

Update the saved writer `subagent_id` with the new one returned.

Report to the user: "Revisions applied. Running re-review..."

## Step 5: Re-review (resume reviewer)

Resume the original reviewer to re-review the revisions.

`task` parameters:
- `subagent_type`: `"general-purpose"`
- `resume_from`: `<reviewer_subagent_id>`
- `description`: `"[reviewer] Re-review design doc"`

Prompt:
```
The writer addressed the review issues. Re-review the design document.

The updated review_file with writer responses is at: <review_file>
The design document is at: <design_doc_file>
The writer's summary is at: <summary_file>

Read all files. Review the document again thoroughly.

Rewrite the review_file with your new findings:
- If a previous issue was properly addressed, do not re-list it.
- If a revision introduced a new problem, list it as a new issue with Status: open.
- If any issue was not properly addressed, re-list it with Status: open.
- Use the same structured format (severity, section, description, suggestion, status).
```

Wait for completion. If it fails, report the error to the user and stop.

Update the saved reviewer `subagent_id` with the new one returned.

**Go back to Step 3.** Repeat until 0 open issues.

## Step 6: Summarize and Ask Open Questions

Once the reviewer reports 0 open issues, read the final design document from `<design_doc_file>`.

### 6a: Extract Key Decisions

Read the design document and extract the "Key Decisions" section. Present a concise summary to the user listing each decision and its rationale.

### 6b: Ask Open Questions

Read the design document and extract the "Open Questions" section (if any remain). If there are unresolved open questions, present them to the user for resolution (use the appropriate ask/question tool if available). For each open question:
- Frame it as a clear question
- Provide the options or trade-offs mentioned in the document as selectable choices
- Include an "Other" option for custom input

If the user provides answers, resume the writer one final time to incorporate the answers into the design document, then skip re-review (the answers are user decisions, not design issues).

If there are no open questions, skip this step.

### 6c: Present PR Plan

Read and present the "PR Plan" section from the design document to the user.

## Exit Condition

The **only** exit condition for the write-review loop is the reviewer reporting **0 issues** of any severity. There is no iteration cap. Every issue -- including nits and minor suggestions -- must be addressed before the loop terminates.

## Cleanup

After presenting the final report, clean up the artifact files:

```bash
rm -f <summary_file> <review_file>
```

Keep the `<design_doc_file>` -- it is the deliverable.

## Final Report

When the loop terminates, present a final report to the user:

1. **Design document location** -- the path to the final document.
2. **Key decisions summary** -- from Step 6a.
3. **Review rounds** -- the value of `round_count` (how many review-revise iterations it took to reach consensus).
4. **Total issues addressed** -- the values from `total_issues_by_severity` (cumulative count across all rounds, broken down by severity).
5. **PR Plan** -- from Step 6c.
6. **Open questions** -- resolved answers or note that none remain.

## In-Progress Reporting

Give the user a brief status update after each phase:

- After write: "Design document drafted. Starting review..."
- After review (0 issues): "Review passed with 0 issues. Finalizing..." (then proceed to Step 6)
- After review (N issues): "Reviewer found N issues (X critical, Y major, Z minor/nit). Resuming writer to revise..."
- After revise: "Revisions applied. Running re-review..."
- Include the iteration number: "Re-review (round 2)...", "Re-review (round 3)..." etc.

## Rules

- **Pass `persona` on `task`** -- `design-doc-writer` or `design-doc-reviewer` by name. Never paste persona bodies into `prompt`. On `resume_from`, pass the same `persona` as the source (or omit).
- **Prefix `description` with a bracketed role tag** -- `[writer]` for design-doc-writer launches and revisions, `[reviewer]` for design-doc-reviewer launches and re-reviews. The pager parses the first `[tag]` of the description and uses it as the row label (the bracketed prefix is stripped from the displayed description). Without this, every row falls through to the generic "General" label. Keep the tag on `resume_from` follow-ups too so the label stays stable across rounds.
- **Include full context in prompts** -- both the writer and reviewer should receive all relevant context from the conversation in their task prompts.
- **resume_from on follow-ups** -- never launch a fresh subagent for revise or re-review rounds. Always resume the previous one so it retains its full working memory.
- **Save every subagent_id** returned by `task` -- these are needed for `resume_from` on subsequent rounds.
- **Read the review_file yourself** after each review to count open issues and decide whether to continue the loop.
- **Don't modify the design document yourself** -- all document changes go through the design-doc-writer persona subagent.
- **Explicitly tell subagents to write their output files** -- include the file path and what to write in every prompt.
- **Thread the same file paths** across all rounds -- never generate new paths between iterations.
- **No iteration cap** -- the loop runs until 0 issues. Do not add a max rounds limit.
- **Always include PR Plan and Key Decisions** -- these are mandatory sections in every design document produced by this skill.
- **Ask the user about open questions** -- never silently resolve open questions; always present them to the user for decision (use the appropriate ask/question tool if available).
- **Writer can push back** -- the writer is explicitly allowed (and encouraged) to set `Status: wontfix` with a technical justification. The reviewer may re-open it, but if they disagree twice, it's a stalemate that gets escalated to the user.
- **Escalate, don't spin** -- if the writer and reviewer cannot reach consensus on an issue after two rounds, escalate to the user by asking them directly (use the appropriate ask/question tool if available). Never let the loop spin on a disagreement.
- **User decisions are final** -- once the user resolves a disputed or `needs-user-input` issue, the writer must incorporate it without further debate.
- **Error handling** -- if a subagent fails or cannot be resumed, report the error to the user and stop. Do not silently continue with missing results.
