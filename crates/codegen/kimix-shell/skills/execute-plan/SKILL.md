---
name: execute-plan
description: Execute a PR Plan DAG from a design document. Parses the plan, topologically sorts it, implements PRs in parallel using worktree-isolated subagents, runs mandatory orchestrator-level review, and assembles either a Graphite PR stack or a plain-git branch stack depending on tool availability.
when-to-use: Use when asked to "execute plan", "run the plan", "implement the design", or "/execute-plan".
argument-hint: "<design-doc-path> [--effort N] [--concurrency N] [--dry-run] [--resume <PLAN_ID>] [--instructions \"...\"] [--no-graphite] [--auto-pr]"
---

# Execute Plan Skill

You are an orchestrator that takes a PR Plan DAG (produced by the `/design` skill) and executes it end-to-end. You parse the DAG, topologically sort and linearize it into a stack, launch parallel implementation subagents in isolated worktrees, run mandatory orchestrator-level review cycles, and assemble the results into a stack of PRs.

You are the **single point of control** for all git and stack-tooling operations. Subagents are sandboxed implementation workers that produce commits in isolated worktrees. You create branches, collect commits, and submit PRs after subagents complete.


## Kimix runtime mapping

This skill was adapted from Grok Build orchestration. On Kimix:

| Grok | Kimix |
|------|-------|
| `spawn_subagent` | `task` |
| `get_command_or_subagent_output` | `task_output` |
| `kill_command_or_subagent` | `kill_task` |
| `todo_write` | `todo_write` (same) |
| worktree isolation | `task` with `isolation: "worktree"` |
| `grok worktree rm` | `git worktree remove --force <path>` |

Personas are runtime-named: pass `persona` on `task` (bundled under `~/.kimix/bundled/personas/`).

## Two Assembly Modes

The skill supports two stack-assembly modes. Mode selection happens once during Setup (see Step 0.5) and is recorded in the state file as `graphite_available`:

- **Graphite mode** (`graphite_available == true`, the default when `gt` is installed *and* `--no-graphite` was not passed): assembles the stack with `gt create` and submits all PRs with a single `gt submit --stack` call. Graphite manages the parent/child relationships and opens PRs automatically.
- **Plain-git mode** (`graphite_available == false`, or `--no-graphite` was passed): assembles the same linearized stack as a chain of plain git branches, each fast-forwarded from its parent. Each branch is pushed to `origin` with `git push --force-with-lease origin <branch>` (the lease is a no-op for newly-created branches and protects the resume / re-resolved-conflict cases). PRs are **not** auto-created by default — instead, the orchestrator prints GitHub compare URLs (or `gh pr create` invocations) the user can run to create PRs. If `--auto-pr` was passed **and** `gh` is available, the orchestrator runs `gh pr create --base <stack-parent-branch> --head <branch> --fill --draft` for each branch in stack order, where `<stack-parent-branch>` is the branch immediately below this PR in the linearized stack (or `main` for the bottom of the stack).

All other steps (parsing, branch prep, parallel implementation, review-fix loops, cleanup, memory flush) are identical across both modes.

You coordinate only. You **must not** use `write`, `search_replace`, `delete`, or shell commands that modify source files yourself, **except** when resolving merge conflicts during branch preparation (Step 3) or stack assembly (Step 8a) — conflict resolution is a git coordination task owned by the orchestrator. **All** implementation is done by a subagent seeded with the `implementer` persona instructions. **All** review is done by a subagent seeded with the `reviewer` persona instructions.

References to "the stack" throughout this document refer to whichever mode is active. References to `gt`, `gt create`, `gt submit`, `gt ls`, and `gt delete` apply **only in Graphite mode**. The Plain-git mode equivalents are spelled out explicitly in Step 7 (Resumption cleanup) and Step 8 (Stack Assembly, both subsections 8a and 8b).

## Subagent Worktree Protocol

The orchestrator interacts with subagent worktrees through a small set of git commands that are written to work uniformly across every environment the skill runs in. Do not branch on the host, the worktree mechanism, or any other property of the environment — the protocol below is the only contract you may rely on.

**Rule 1 — fetch without a destination refspec.** When you need the subagent's commits in the orchestrator's main repo, run exactly:

```bash
git fetch <worktree_path> HEAD --no-tags
```

Never add `:refs/heads/<pr.branch>` and never pass `--force`. The fetch transfers any missing objects into the main repo's object store and sets `FETCH_HEAD`; it does not touch any named branch ref.

**Rule 2 — `pr.commit_sha` is the authoritative reference.** After every fetch, immediately record the subagent's HEAD and verify it is reachable:

```bash
pr.commit_sha = $(git -C <worktree_path> rev-parse HEAD)
git cat-file -t <pr.commit_sha>   # must print "commit"
```

Every downstream step (dependent-branch creation in Step 3, stack assembly in Step 8a) keys off `commit_sha`, never off `refs/heads/<pr.branch>` in the main repo. Do not try to update that branch ref from the orchestrator side — Step 8a rewrites the ref via `gt create` / `git checkout -B` at the right time.

**Rule 3 — tear down the worktree before mutating its branch ref.** Step 8a is the only step that mutates `refs/heads/<pr.branch>` in the main repo. Immediately before that mutation, the orchestrator must remove the subagent's worktree:

```bash
if [ -n "<pr.worktree_path>" ] && [ -d "<pr.worktree_path>" ]; then
  git worktree remove --force "<pr.worktree_path>"
fi
```

The command is idempotent and safe to run when the worktree is already gone, missing, or was never created (e.g., a `failed`/`skipped` PR).

## Tool-Call Discipline (Anti-Hallucination)

Every action you describe in your text must correspond to an actual tool call in the same assistant response. The execution loop spawns subagents in tight cycles, so the safest pattern is: emit the `task` tool call **first**, then once the tool result is in the history, write the user-visible status update referencing the returned `subagent_id`. Never end a turn with prose that claims a PR's implementer or reviewer "is being launched" when no `task` call appears in the same response. Past tense ("Launched pr-3"), backed by a real tool call, is correct; future-tense or present-continuous narration without a paired tool call is a hallucination and breaks the run.

## Todo Scaffold

The PR plan is a DAG. Each DAG node becomes a top-level todo with id `pr-<node-id>`. Inside each node, use sub-id namespacing for phases:

- `pr-<n>:branch-prep` — Step 3
- `pr-<n>:execute` — Step 4 (worker loop)
- `pr-<n>:review` — review phase if invoked
- `pr-<n>:merge-ready` — push + CI green + ready-to-merge

Terminal state: all `pr-<n>:merge-ready` ids `completed`. Only then write the final stack assembly report.

**Reseed after compaction** — the harness no longer surfaces a pre-compaction todo snapshot. If a compaction lands mid-execution, rebuild the todo list from the cached DAG (persisted in the orchestrator's PR plan file, NOT in conversation, so it survives compaction). Reseed before advancing any further step.

## Persona Injection

This skill uses the **implementer** and **reviewer** personas. They are **bundled runtime personas** resolved by name via `task.persona`. Do **not** `read_file` persona bodies or paste them into `prompt`.

When launching a subagent, pass `persona: "implementer"` or `persona: "reviewer"`. Still prefix `description` with `[implementer]` / `[reviewer]` for the pager label. On `resume_from`, pass the same `persona` as the source (or omit) and keep the bracketed tag.

## Invocation

The user runs:
```
/execute-plan <design-doc-path> [--effort N] [--concurrency N] [--dry-run] [--resume <PLAN_ID>] [--no-graphite] [--auto-pr]
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `<design-doc-path>` | String | Required | Path to the design document (from `/design`) |
| `--effort` | Integer (1-2) | 1 | Review thoroughness. 1 = one reviewer, loop until 0 issues (default). 2 = currently identical to 1 (one reviewer, loop until 0 issues). Reserved for future multi-reviewer support per PR, matching `/implement`'s effort-scales-reviewer-count pattern. |
| `--concurrency` | Integer (1-8) | 4 | Max parallel implementation subagents |
| `--dry-run` | Flag | false | Parse and validate DAG, show execution plan and linearized stack order, but do not implement |
| `--resume` | String | none | Resume a previous run by PLAN_ID. Reads the state file and retries failed PRs. |
| `--instructions` | String | none | Extra instructions injected into every implementer and reviewer prompt. Use for cross-cutting concerns like "Enforce rust rules from /root/.grok/memory/MEMORY.md" or "Don't modify the public API". |
| `--no-graphite` | Flag | false | Force plain-git mode even if `gt` is installed. When set, `graphite_available` is forced to `false` and Step 8 uses the plain-git assembly path. |
| `--auto-pr` | Flag | false | Only meaningful in plain-git mode. When set **and** `gh` is detected, the orchestrator runs `gh pr create --base <stack-parent-branch> --head <branch> --fill --draft` for each branch in stack order, where `<stack-parent-branch>` is defined in the Two Assembly Modes section above. When unset, the orchestrator only prints compare URLs / suggested commands. Ignored in Graphite mode (where `gt submit --stack` always opens PRs). |

Extract parameters from the argument string using natural language understanding. If `--effort` is not present or out of range (1-2), default to 1. If `--concurrency` is not present or out of range (1-8), default to 4. If `--instructions` is present, extract the quoted string value after it. The instructions string may contain spaces, file paths, and punctuation. If `--instructions` is not present, default to "". `--no-graphite` and `--auto-pr` are boolean flags (presence = true).

**If `--resume <PLAN_ID>` is specified**, skip Setup and jump to Step 7 (Resumption).

## Setup

Generate a unique ID for this run. Execute via `run_terminal_cmd` and capture the output:

```bash
python3 -c "import uuid; print(uuid.uuid4().hex[:8])"
```

**Validate** that the command succeeded and produced a non-empty string. If `PLAN_ID` is empty or the command failed, report the error to the user and stop.

Store the output as `PLAN_ID`.

Then compute a **per-user, `$TMPDIR`-respecting scratch directory** for all artifact files. Never write skill artifacts directly under `/tmp` on a shared host: it leaks their contents to other users and ignores a user-configured `$TMPDIR`. Run via `run_terminal_cmd` and capture stdout:

```bash
scratch_dir="${TMPDIR:-/tmp}/kimix-$(id -u)"; mkdir -p "$scratch_dir" && chmod 700 "$scratch_dir" && echo "$scratch_dir"
```

Store the output as `scratch_dir`. **Inline the resolved absolute path** into every file path below and into every subagent prompt; do not rely on a `$scratch_dir` shell variable surviving across separate `run_terminal_cmd` calls (the same reason this skill inlines `${MEMORY_HELPER}`).

Then define the shared file paths (all under `scratch_dir`):
- `state_file`: `${scratch_dir}/kimix-exec-plan-${PLAN_ID}.json`
- Per-PR summary: `${scratch_dir}/kimix-exec-summary-${PLAN_ID}-<pr-id>.md`
- Per-PR review: `${scratch_dir}/kimix-exec-review-${PLAN_ID}-<pr-id>.md`

These paths stay the same for the entire run. Never regenerate them between iterations.

Initialize these orchestrator state variables:
- `design_doc_path`: the user-provided path to the design document
- `effort`: parsed effort level (1 or 2)
- `max_concurrent`: parsed concurrency limit (1-8, default 4)
- `dag`: the parsed PR Plan DAG (populated in Step 1)
- `linearized_order`: the linearized PR order for stack assembly (populated in Step 2)
- `ready_queue`: PRs ready to execute (all dependencies complete)
- `in_progress`: map of PR id to task_id for currently running subagents
- `completed`: map of PR id to completion data (commit_sha, worktree_path)
- `failed`: map of PR id to error information
- `skipped`: map of PR id to skip reason
- `past_issues_briefing`: `""` -- populated in Step 0 from the workspace memory file. Contains a formatted markdown block of common issue patterns from previous runs, injected into implementer and reviewer prompts.
- `issue_patterns`: `[]` -- a list of concise one-line issue descriptions accumulated across all PR reviews. After each review round (Step 5b and Step 5c re-reviews), extract a one-line description of each open issue and append (deduplicating exact matches). Used in Step 10 (Memory Flush).
- `total_issues_by_severity`: `{}` -- a map from severity (bug, suggestion, nit) to cumulative count. After each review round (Step 5b and Step 5c re-reviews), add the count of open issues by severity to this accumulator. Used in Step 10c for the memory flush.
- `existing_patterns_snapshot`: `[]` -- from Step 0, used in Step 10b for phrasing harmonization.
- `memory_existed_before`: `false` -- from Step 0. The Final Report uses the helper's Step 10d update output `existed_before` flag instead (which may differ if the file was created by a concurrent run between Step 0 and Step 10).
- `user_instructions`: `""` -- parsed from `--instructions` argument. When non-empty, injected into every implementer prompt (Step 4a), reviewer prompt (Step 5a), and fix cycle prompt (Step 5c).
- `no_graphite_flag`: `false` -- parsed from `--no-graphite` argument. When `true`, forces `graphite_available` to `false` regardless of what is installed.
- `auto_pr_flag`: `false` -- parsed from `--auto-pr` argument. Only consulted in plain-git mode (see Step 8).
- `graphite_available`: `null` -- populated in Step 0.5 from the `command -v gt` probe (combined with `no_graphite_flag`).
- `gh_available`: `null` -- populated in Step 0.5 from the `command -v gh` probe. Used in plain-git mode to decide whether `--auto-pr` can run.

For the four tool-detection fields, treat `null` as "not yet probed" and `true`/`false` as "probed and known". Step 0.5 is the only step that transitions them from `null`. Every consumer (Step 7, Step 8, Step 8b) should assume `null` is never seen at consumption time — if it ever is, re-run the Step 0.5 probe before continuing.

Initialize the state file:

```json
{
  "plan_id": "<PLAN_ID>",
  "design_doc_path": "<design-doc-path>",
  "status": "initializing",
  "created_at": "<ISO 8601 timestamp>",
  "effort": <effort>,
  "max_concurrent": <max_concurrent>,
  "linearized_order": [],
  "dag": { "nodes": [] },
  "stack_assembly_started": false,
  "stack_assembly_progress": [],
  "graphite_stack_submitted": false,
  "pr_urls": [],
  "pr_create_commands": [],
  "user_instructions": "<user_instructions>",
  "no_graphite_flag": <no_graphite_flag>,
  "auto_pr_flag": <auto_pr_flag>,
  "graphite_available": null,
  "gh_available": null
}
```

Field notes:
- `graphite_stack_submitted` has a dual meaning. In Graphite mode it literally means "`gt submit --stack` succeeded". In plain-git mode it is reused as a generic "Step 8b finished" sentinel — Step 7's resumption gate keys off this field in both modes, so the overload is intentional. Renaming would break older state files.
- `pr_urls` holds either Graphite-returned PR URLs (Graphite mode), `gh`-created PR URLs (plain-git + `--auto-pr`), or GitHub compare URLs (plain-git, no `--auto-pr`). In plain-git mode each entry is an object `{branch, url, kind, note?}` where `kind` is one of `"pr"`, `"compare"`, or `"pushed-only"`, and `note` is an optional explanatory string. See Step 8b (plain-git mode) for the full schema and field semantics. Graphite mode entries are plain strings.
- `pr_create_commands` is populated only in plain-git mode when `--auto-pr` was not used (or `gh` was not available). Each entry is a copy/paste-ready `gh pr create ...` invocation, in stack order. Empty otherwise.

Write this initial state file using the `write` tool.

Report to the user: `"Starting execute-plan with PLAN_ID: <PLAN_ID>, effort: <effort>, concurrency: <max_concurrent>"`
If `user_instructions` is non-empty, add: `"User instructions: <first 100 chars>..."`

## Step 0.5: Tool Detection (Graphite & gh)

Probe the environment **once** at the start of the run to choose the stack-assembly mode. Use the safe `if ... then ... else ... fi` form (the `&& X || Y` shortcut is a footgun if `X` ever fails):

```bash
if command -v gt >/dev/null 2>&1; then echo "yes"; else echo "no"; fi

if command -v gh >/dev/null 2>&1; then echo "yes"; else echo "no"; fi
```

For additional confidence that the binary named `gt` is actually Graphite (and not an unrelated tool that happens to share the name), additionally run `gt --help 2>/dev/null` when the first probe returns `yes` and verify the output contains `graphite` (case-insensitive substring match — e.g., `if gt --help 2>/dev/null | grep -qi graphite; then echo "yes"; else echo "no"; fi`). `gt --version` is unreliable here because Graphite's version output is a bare version string with no product name, so it fails the substring check even for a legitimate Graphite install; `gt --help` reliably prints the Graphite CLI banner. Treat unrecognized output as `no`.

Set the state variables based on the probe results:

- `gh_available = (gh probe returned "yes")`
- If `no_graphite_flag` is `true`: set `graphite_available = false` (user explicitly disabled Graphite).
- Otherwise: set `graphite_available = (gt probe returned "yes" AND gt --help output contains "graphite" case-insensitively)`.

Re-write the state file via the `write` tool with the updated `graphite_available` and `gh_available` values (the skill does not use a JSON-merge tool; every state-file update is a full rewrite). From this point on, every step that touches stack tooling branches on `graphite_available`.

Report exactly one of the following based on the resolved mode:

- Graphite mode: `"Graphite mode: gt detected. Stack will be assembled with gt create + gt submit --stack."`
- Plain-git mode (auto-fallback): `"Plain-git mode: gt not installed. Stack will be assembled with plain git branches; PR creation guidance will be printed after assembly."` (when `no_graphite_flag == false` AND `gt` missing)
- Plain-git mode (user override): `"Plain-git mode: --no-graphite was set. Stack will be assembled with plain git branches; PR creation guidance will be printed after assembly."` (when `no_graphite_flag == true`)

If plain-git mode is active and `auto_pr_flag` is true but `gh_available` is false, additionally warn: `"--auto-pr was requested but gh is not installed; the orchestrator will fall back to printing compare URLs."`

## Step 0: Memory Retrieval (Past Issues Briefing)

Before launching any implementers, attempt to load past issue patterns from the workspace memory file. This briefing is injected into both implementer and reviewer prompts to help avoid recurring issues. The execute-plan skill **shares the same memory file** as the `/implement` skill -- patterns from both skills help each other.

### Resolve the helper path

The memory helper lives in the implement skill's directory. Derive the path from the implement skill's known location in the system context (the skills list announces each skill's path). The helper is at:

```
memory_helper_path = dirname(<path-to-implement-SKILL.md>) + "/scripts/memory.py"
```

For example, if the implement skill's SKILL.md is at `/root/.kimix/worktrees/xai/repo/.kimix/skills/implement/SKILL.md`, then `memory_helper_path` is `/root/.kimix/worktrees/xai/repo/.kimix/skills/implement/scripts/memory.py`.

**Substitute this absolute path directly** into every helper invocation -- do not rely on a bash environment variable surviving across `run_terminal_cmd` calls. All examples below show `${MEMORY_HELPER}` for readability; in practice, inline the absolute path.

**Invoke the helper from the workspace root** (the default cwd for `run_terminal_cmd`). The helper derives the workspace id from the cwd's git context.

### Read Path

1. Run `python3 "${MEMORY_HELPER}" snapshot` via `run_terminal_cmd` and capture stdout. The helper prints structured JSON:

   ```json
   {
     "common_issues": [
       {"category": "Error Handling", "description": "Missing null check", "count": 5}
     ],
     "recent_runs": [...],
     "exists": true
   }
   ```

   Store the `common_issues` list as `existing_patterns_snapshot`. Store the boolean `exists` as `memory_existed_before`.
2. If the helper exits non-zero, log a brief note, set `past_issues_briefing` to `""`, `existing_patterns_snapshot` to `[]`, `memory_existed_before` to `false`, and proceed to Step 1.
3. If `existing_patterns_snapshot` is empty (or `exists` is `false`), set `past_issues_briefing` to `""`.

### Parsing & Formatting

If `existing_patterns_snapshot` is non-empty:

1. Filter to only entries with `count >= 2`.
2. Sort by `count` descending.
3. Take the top 10 entries.
4. Format into `past_issues_briefing`:

```
## Past Issues to Avoid
Based on previous implementation runs, the following patterns commonly cause issues:
1. Missing null/undefined checks on function inputs (seen 5 times)
2. Missing tests for error/edge case paths (seen 8 times)

Pay special attention to these patterns in your work.
```

(Use `time` for count == 1, `times` otherwise.)

If there are no qualifying entries, set `past_issues_briefing` to `""`.

### Graceful Degradation

If the helper command fails for any reason, set `past_issues_briefing` to `""`, `existing_patterns_snapshot` to `[]`, `memory_existed_before` to `false`, and proceed normally. Never fail the run due to memory retrieval issues.

## Step 1: Parse PR Plan DAG

Read the design document at `<design_doc_path>` using `read_file`.

Extract the `## PR Plan` section. Parse each PR entry from `### PR N:` headings into a structured representation:

```
PRNode {
    id: string           // e.g., "pr-1", "pr-2" -- derived from the PR number
    title: string        // PR title (text after "### PR N: ")
    slug: string         // URL-safe slug: lowercase, spaces to hyphens, non-alphanumeric removed, truncated to 50 chars
    description: string  // From the "Description:" or "**Description:**" bullet
    files: string[]      // From the "Files/components affected:" bullet
    dependencies: string[] // From the "Dependencies:" bullet, parsed as PR ids
    level: int           // Computed in Step 2
    status: string       // "pending"
    branch: string       // Computed after level assignment
    subagent_id: string  // null initially
    worktree_path: string // null initially
    base_sha: string     // null initially -- branch point before implementation
    commit_sha: string   // null initially -- HEAD after all commits (implementation + fixes)
    error: string        // null initially
    reviewer_subagent_id: string // null initially
    review_rounds: int   // 0 initially
    started_at: string   // null initially
    completed_at: string // null initially
    worktree_cleaned: bool // false initially; set true by Step 8a (per-PR prologue) or Step 9 (safety net) after `git worktree remove --force` succeeds. Step 9 uses this to stay idempotent across resumes.
}
```

**Parsing strategy:**

1. Find the `## PR Plan` section by scanning for the heading.
2. Split into individual PR entries at each `### PR` heading.
3. For each entry:
   - Extract the PR number and title from the heading (e.g., `### PR 1: Add retry configuration types`).
   - Extract `Files/components affected:` from the bullet list (split by comma).
   - Extract `Dependencies:` from the bullet list. Parse references like "PR 1", "PR 2" into ids like "pr-1", "pr-2". "None" means empty dependencies.
   - Extract `Description:` from the bullet list.
   - Generate the slug from the title:
     1. Lowercase the title.
     2. Replace spaces and underscores with hyphens.
     3. Remove any character that is not `[a-z0-9-]`.
     4. Collapse consecutive hyphens into a single hyphen.
     5. Strip leading and trailing hyphens.
     6. Remove the suffix `.lock` if present (defensive -- dots are already removed in step 3, but this guards against future validation changes; git rejects refs ending in `.lock`).
     7. Truncate to 50 characters. If truncation lands mid-hyphen-sequence, trim trailing hyphens after truncation.
     8. If the result is empty after all transformations, use `"unnamed"`.

     The resulting slug is safe for use in git branch names: no spaces, no `..`, no control characters, no `~^:?*[\`, no trailing `.lock`, no leading/trailing dots, no consecutive dots.
   - Set `id` to `"pr-N"` where N is the PR number.

4. Validate the DAG:
   - All dependency references must resolve to valid PR ids.
   - There must be no cycles. Detect cycles by attempting topological sort -- if it fails, report the cycle and stop.
   - Every PR must have a unique id.

If parsing fails (missing PR Plan section, malformed entries, unresolved dependencies, cycles), report the error with details and stop.

Store the parsed nodes in `dag.nodes`.

Report to the user: `"Parsed PR Plan: <N> PRs found."`

## Step 2: DAG Processing and Linearization

### Level Assignment

Assign execution levels to each PR node:

```
level(node) = 0                                              if dependencies is empty
level(node) = max(level(dep) for dep in dependencies) + 1    otherwise
```

PRs at the same level are independent and can execute in parallel.

### Linearization for Stack Assembly

Stacks (Graphite or plain-git) are strictly linear chains. The DAG is flattened into a single topologically-sorted linear sequence:

```
def linearize(dag):
    """Produce a linear order respecting all dependency edges."""
    nodes_by_level = group_by(dag.nodes, key=lambda n: n.level)
    result = []
    for level in sorted(nodes_by_level.keys()):
        for node in sorted(nodes_by_level[level], key=lambda n: int(n.id.split('-')[1])):
            result.append(node)
    return result
```

Within each level, sort by the numeric PR number (extracted from the id string via `int(n.id.split('-')[1])`) to produce a deterministic, reviewable order. Do NOT use lexicographic string sort on the id -- `"pr-10"` would sort before `"pr-2"` lexicographically.

Store the result in `linearized_order` and update the state file.

### Compute Branch Names

For each PR node, compute the branch name:
```
branch = "execute-plan/<PLAN_ID>-<pr-number>-<slug>"
```

Example: `execute-plan/a1b2c3d4-pr-1-add-retry-config-types`

Update each node's `branch` field.

### Report

Report to the user:
```
"Linearized stack order: PR1 (<title>) -> PR4 (<title>) -> PR2 (<title>) -> ...
Max parallelism: <max PRs at any single level>
Levels: <number of levels>"
```

### Dry-Run Exit

If `--dry-run` was specified, report the full execution plan (linearized order, level assignments, branch names, dependency graph) and stop. Do not proceed to implementation.

## Step 3: Branch Preparation

Before launching any subagents, prepare branches for all level-0 PRs (those with no dependencies). Branches for higher-level PRs are created just-in-time when their dependencies complete.

```bash
git fetch origin main
```

**For a PR with NO dependencies (level 0):**
```bash
git branch <pr.branch> origin/main
pr.base_sha = $(git rev-parse <pr.branch>)
```

**For a PR with a SINGLE dependency** (created when the dependency completes):
```bash
git branch <pr.branch> <dep.commit_sha>
pr.base_sha = $(git rev-parse <pr.branch>)
```

**For a PR with MULTIPLE dependencies (diamond)** (created when all dependencies complete):
```bash
git branch <pr.branch> <first-dep.commit_sha>

TEMP_WT=$(mktemp -d)
git worktree add "$TEMP_WT" <pr.branch>
git -C "$TEMP_WT" merge <second-dep.commit_sha> --no-edit
git worktree remove "$TEMP_WT"

pr.base_sha = $(git rev-parse <pr.branch>)
```

**Note:** Branch creation keys off the dependency's recorded `commit_sha`, never its branch name — this is Rule 2 of the *Subagent Worktree Protocol*. The main repo's `refs/heads/<dep.branch>` is not a reliable pointer to the dependency's latest commit; only `dep.commit_sha` is. The orchestrator still enforces ordering — dependent branches are created only after the dependency's `process_completion` step has recorded `dep.commit_sha` and fetched its objects.

After creating each branch, record `pr.base_sha` (via `git rev-parse <branch_name>`) and update the PR node's status to `"branch_created"` in the state file. The `base_sha` captures the exact commit the branch points to before any implementation work, and is used for range cherry-pick in Step 8a.

**Critical: the orchestrator must NOT switch its own checked-out branch.** Use `git branch` (no checkout) for branch creation and temp worktrees for merge operations.

Report to the user: `"Created branches for <N> level-0 PRs. Starting implementation..."`

## Step 4: Execution Loop

The orchestrator uses a ready-queue approach for maximum parallelism:

```
ready_queue = [all PRs with status "branch_created" and all dependencies completed]
in_progress = {}
completed = {}

while ready_queue is not empty OR in_progress is not empty:
    while ready_queue is not empty AND len(in_progress) < max_concurrent:
        pr = ready_queue.pop(0)
        task_id = launch_implementer(pr)
        in_progress[pr.id] = task_id

    result = wait_commands_or_subagents(task_ids=list(in_progress.values()), mode="wait_any", timeout_ms=600000)

    for pr_id, task_id in in_progress where elapsed > PR_TIMEOUT:
        kill_task(task_id)
        mark pr as "failed" with error "Implementation timed out after 15 minutes"
        cascade_skip(pr_id)
        remove from in_progress

    for each completed task in result:
        finished_pr = identify_pr_from_task_id(task)
        process_completion(finished_pr, task)
        review_pr(finished_pr)

    for pr in remaining_pending:
        if all dependencies of pr are in completed:
            create_branch(pr)
            ready_queue.append(pr)
```

### Step 4a: Launch Implementer

For each PR, launch an implementer subagent in a worktree.

After the worktree is created (by `task`'s `isolation: "worktree"`), but before the implementer reaches `git checkout`, push the branch into the worktree. The branch was created in the main repo (Step 3) only after the worktree had already been built, so the worktree may not see the ref yet — this push ensures it does. Extract `worktree_path` from the subagent's initial output and push immediately -- the subagent's prompt processing provides a sufficient time window before `git checkout` is reached:
```bash
git push <worktree_path> refs/heads/<pr.branch>:refs/heads/<pr.branch>
```

`task` parameters:
- `subagent_type`: `"general-purpose"`
- `isolation`: `"worktree"`
- `background`: `true`
- `description`: `"[implementer] <pr.id>: <pr.title>"`

Pass `persona: "implementer"` on the `task` call. Do **not** prepend persona text to the prompt.

Prompt:
```

---

You are implementing a single PR as part of a larger plan.

## Your PR
- Title: <pr.title>
- Description: <pr.description>
- Files to modify: <pr.files joined by comma>
- Branch: <pr.branch>

## Context from Design Document
<relevant sections from the design doc -- read and include the full design doc
content that pertains to this PR's scope>

<if past_issues_briefing is non-empty, include the following block verbatim:>
<past_issues_briefing>
Be proactive about avoiding these patterns in your implementation.
<end if>

<if user_instructions is non-empty, include the following block verbatim:>
## User Instructions
<user_instructions>
These instructions apply to all work in this plan. Follow them strictly.
<end if>

## Instructions

1. First, check out your branch:
   git checkout <pr.branch>
   This branch already contains changes from your dependencies.

2. Implement the changes described above.

3. Verify your code compiles and passes basic checks
   (e.g., cargo check, tsc --noEmit, python -m py_compile as appropriate).

4. Commit all changes with a descriptive message.
   If git commit fails with a lock error, wait 2 seconds and retry up to 3 times:
   git add -A
   git commit -m "<pr.title>

   <brief description of changes>"

5. Write an implementation summary to: ${scratch_dir}/kimix-exec-summary-<PLAN_ID>-<pr.id>.md
   Include: files changed, key decisions, any deviations from the plan.
```

Update the PR node's status to `"implementing"` and `started_at` to the current timestamp. Persist the state file.

Report to the user: `"Launching <pr.id> (<pr.title>)..."`

### Step 4b: Process Completion

When a subagent completes (returned by `wait_commands_or_subagents`):

1. Identify which PR finished by matching the returned `task_id` against `in_progress`. The `wait_commands_or_subagents` result contains entries with `task_id`, `status`, and `output` fields. Match `task_id` against the `in_progress` map to find the corresponding PR.
2. Extract subagent metadata from the result. The `worktree_path` and `subagent_id` are embedded in the `task` result's output text, wrapped in structured tags. Extract these values for state tracking.
3. Read the subagent's result:
   - If the subagent succeeded:
     - Record the `subagent_id` and `worktree_path` extracted from the result.
     - Note: `pr.base_sha` was already recorded in Step 3 at branch creation time. No need to compute it here.
     - Get the HEAD commit SHA:
       ```bash
       git -C <worktree_path> rev-parse HEAD
       ```
       Store as `pr.commit_sha`.
     - Fetch the subagent's commits into the main repo's object store, per
       Rule 1 of the *Subagent Worktree Protocol*:
       ```bash
       git fetch <worktree_path> HEAD --no-tags
       ```
       **No destination refspec, no `--force`.** Adding `:refs/heads/<pr.branch>`
       is forbidden — it can fail outright, and downstream steps key off
       `commit_sha` so the named ref never needs to be updated here.

       Then verify the SHA is reachable from the main repo so a failed fetch
       fails loudly here instead of much later in Step 8a (Rule 2):
       ```bash
       git cat-file -t <pr.commit_sha>   # must print "commit"
       ```
     - Update status to `"reviewing"`.
   - If the subagent failed:
     - Record the error.
     - Update status to `"failed"`.
     - Cascade-skip all transitive dependents (see Step 6).
     - Report: `"<pr.id> (<pr.title>) FAILED: <error>"`

Persist the state file after each status transition.

Report to the user on success: `"<pr.id> (<pr.title>) implemented. <N> files changed. Starting review..."`

## Step 5: Orchestrator-Level Review

After each PR's implementation completes successfully, the orchestrator **always** launches a separate reviewer subagent targeting the implementer's worktree. Independent review by the `reviewer` persona is mandatory for every PR -- there is no self-review-only mode.

### Step 5a: Launch Reviewer

Before launching the reviewer, read the PR's implementation summary at `${scratch_dir}/kimix-exec-summary-<PLAN_ID>-<pr.id>.md`. Based on it, identify 2-3 concrete areas the reviewer should pay extra attention to (e.g., "Verify error paths are tested", "Check that callers of renamed functions were updated"). Store as `reviewer_focus_areas` for this PR.

The reviewer accesses the implementer's worktree via the `cwd` parameter:

`task` parameters:
- `subagent_type`: `"general-purpose"`
- `cwd`: `<pr.worktree_path>`
- `description`: `"[reviewer] <pr.id>: <pr.title>"`

Pass `persona: "reviewer"` on the `task` call. Do **not** prepend persona text to the prompt.

Prompt:
```

---

Review the changes made for <pr.id>: <pr.title>.

The implementation summary is at: ${scratch_dir}/kimix-exec-summary-<PLAN_ID>-<pr.id>.md

Review all modified files.

<if past_issues_briefing is non-empty, include the following block verbatim:>
<past_issues_briefing>
<end if>

<if user_instructions is non-empty, include the following block verbatim:>
## User Instructions
<user_instructions>
These instructions apply to all reviews in this plan. Follow them strictly.
<end if>

<if reviewer_focus_areas is non-empty:>
## Additional focus areas (from implementation summary)
<reviewer_focus_areas>
<end if>

Write review findings to:
${scratch_dir}/kimix-exec-review-<PLAN_ID>-<pr.id>.md

Use the structured format:
### Issue N -- Severity: bug|suggestion|nit
- **File**: path/to/file.ext:LINE
- **Description**: <what is wrong>
- **Suggestion**: <how to fix>
- **Status**: open

If the code is clean, write a summary confirming no issues found and an empty Issues section.
```

Wait for the reviewer to complete. Save the returned `subagent_id` as `pr.reviewer_subagent_id`. This id is used for `resume_from` on subsequent review rounds (Step 5c).

### Step 5b: Check Review Results

Read the review file at `${scratch_dir}/kimix-exec-review-<PLAN_ID>-<pr.id>.md`. Count issues with `Status: open`.

For each open issue found, extract a concise one-line description and append it to `issue_patterns` (skip exact duplicates already in the list). These are accumulated across all PRs and all review rounds for the entire run, and used in Step 10 (Memory Flush).

Count open issues by severity (bug, suggestion, nit) and add the counts to `total_issues_by_severity`.

Increment `pr.review_rounds` (this counts total reviews performed, including the initial one).

**If 0 open issues:**
- The PR is done. Update status to `"completed"`, set `completed_at`.
- Persist the state file.
- Report: `"<pr.id> review: 0 issues found. PR complete."`
- Return to the execution loop (Step 4).

**If any open issues:**
- Enter a review-fix loop. Resume the implementer to fix (Step 5c), then
  resume the reviewer to re-review. Repeat until 0 open issues.
- There is no iteration cap. Every issue -- including nits and suggestions --
  must be addressed before the PR is marked completed.
- See Step 5c.

Report: `"<pr.id> review round <review_rounds>: <N> issues found (<X> bugs, <Y> suggestions, <Z> nits). Fixing..."`

### Step 5c: Fix Cycle

Resume the original implementer to fix all review issues:

`task` parameters:
- `subagent_type`: `"general-purpose"`
- `resume_from`: `<pr.subagent_id>`
- `description`: `"[implementer] Fix review issues for <pr.id>"`

Prompt:
```
The reviewer found issues. The review file is at:
${scratch_dir}/kimix-exec-review-<PLAN_ID>-<pr.id>.md

Read the review file. Address ALL issues with Status: open -- including nits,
suggestions, and any style or hint-level feedback. Nothing is too small to fix.

For each issue, implement the fix, then update the review file:
- Change Status: open -> Status: fixed
- Add a Response field explaining what you changed

You are encouraged to push back on feedback that doesn't make sense, is
contradictory, or would make the implementation worse. If you disagree with
an issue:
- Set Status: wontfix
- Write a clear, technical explanation of why the reviewer's suggestion is
  wrong or counterproductive
- Do NOT comply with feedback just to make a reviewer happy -- defend good
  implementation decisions

Commit all fixes:
git add -A
git commit -m "fix: address review feedback for <pr.title>"

<if user_instructions is non-empty, include the following block verbatim:>
## User Instructions
<user_instructions>
These instructions apply to all work in this plan. Follow them strictly.
<end if>
```

Wait for the implementer to complete. Update `pr.subagent_id` with the new returned id. Read the review file to note any issues the implementer marked `Status: wontfix` (needed for stalemate detection below).

After the fix, re-fetch the subagent's new commits using the same form as Step 4b (Rule 1 of the *Subagent Worktree Protocol*):
```bash
git fetch <worktree_path> HEAD --no-tags
```
Then update the commit SHA:
```bash
pr.commit_sha = $(git -C <worktree_path> rev-parse HEAD)
git cat-file -t <pr.commit_sha>   # must print "commit"
```
No destination refspec, no `--force` — both are unnecessary because `commit_sha` is what downstream steps use, and the subagent is the sole writer for this branch so the fetch is always a simple object pull.

Resume the reviewer to re-review.

`task` parameters:
- `subagent_type`: `"general-purpose"`
- `resume_from`: `<pr.reviewer_subagent_id>`
- `description`: `"[reviewer] Re-review <pr.id>: <pr.title> (round <review_rounds>)"`

Prompt:
```
The implementer has addressed the feedback. Re-review the changes.

Review file location: ${scratch_dir}/kimix-exec-review-<PLAN_ID>-<pr.id>.md

Re-read the review file and all modified files. For each existing issue:
- If fixed properly: leave Status: fixed
- If not fixed or fix introduced new problems: set Status: open with updated description

Check for regressions introduced by the fixes.

Append any NEW issues found using the structured format:
### Issue N -- Severity: bug|suggestion|nit
- **File**: path/to/file.ext:LINE
- **Description**: <what is wrong>
- **Suggestion**: <how to fix>
- **Status**: open

Accept convincing wontfix justifications -- do not reopen issues where the
implementer provided a sound technical rationale.

<if past_issues_briefing has accumulated new patterns since the previous review round, include the updated block verbatim:>
<past_issues_briefing>
<end if>

<if user_instructions is non-empty, include the following block verbatim:>
## User Instructions
<user_instructions>
These instructions apply to all reviews in this plan. Follow them strictly.
<end if>
```

Wait for completion and update `pr.reviewer_subagent_id` with the new returned id.

**Fallback on resume failure:** If `resume_from` fails (e.g., subagent expired, quota exceeded, or internal error), fall back to launching a fresh reviewer with `cwd: <pr.worktree_path>` using the same prompt template as Step 5a (including reviewer persona instructions, `past_issues_briefing`, `reviewer_focus_areas`, and `user_instructions`). Update `pr.reviewer_subagent_id` with the new task id. This ensures the review-fix loop is never blocked by a stale or expired subagent reference.

This is the next review round. Read the new review results. For each open issue found, extract a one-line description and append to `issue_patterns` (skip exact duplicates). Count open issues by severity and add to `total_issues_by_severity`. Increment `pr.review_rounds`.
- If 0 issues: mark `"completed"`.
- If issues remain: repeat the fix cycle (fix, then resume the reviewer).
- There is no iteration cap.
- **Stalemate detection:** If the implementer sets `Status: wontfix` on an
  issue and the next reviewer re-opens the same issue (matched by file
  reference and description), the implementer and reviewer have reached a
  disagreement. Escalate to the user: frame the dispute clearly, present both
  positions, and let the user decide. The user's decision is final -- resume
  the implementer with the decision and set the issue to `Status: fixed`.

Report: `"<pr.id> fix cycle complete. Re-reviewing (round <review_rounds>)..."`

Persist the state file. Return to the execution loop (Step 4).

## Step 6: Failure Handling

When a PR fails (implementation error, unfixable review issues, or branch creation failure):

### Cascade-Skip

Mark the failed PR as `"failed"` with the error details. Then cascade-skip all transitive dependents:

```
def cascade_skip(failed_pr_id, all_prs):
    for pr in all_prs:
        if pr.status == "pending" or pr.status == "branch_created":
            if failed_pr_id in transitive_dependencies(pr):
                pr.status = "skipped"
                pr.error = "Skipped: dependency <failed_pr_id> failed"
```

Remove any skipped PRs from the ready_queue. Do not abort the entire run -- independent PRs continue executing.

Report:
```
"<pr.id> (<pr.title>) FAILED: <error>
Dependent PRs skipped: <list of skipped pr ids and titles>
Independent PRs continue running."
```

Persist the state file after all status updates.

## Step 7: Resumption

When invoked with `--resume <PLAN_ID>`:

Setup is skipped on resume, so first **recompute the scratch directory** — the state file and all per-PR artifacts live under it. This assumes the same `$TMPDIR` as the original run:

```bash
scratch_dir="${TMPDIR:-/tmp}/kimix-$(id -u)"; mkdir -p "$scratch_dir" && chmod 700 "$scratch_dir" && echo "$scratch_dir"
```

1. Read the state file at `${scratch_dir}/kimix-exec-plan-<PLAN_ID>.json`.
2. If the file does not exist, report the error and stop.
3. Restore orchestrator state from the file: `design_doc_path`, `effort`, `max_concurrent`, `dag`, `linearized_order`, `user_instructions`, `no_graphite_flag`, `auto_pr_flag`, `graphite_available`, `gh_available`, `pr_urls`, `pr_create_commands`. Apply these defaults for any field missing from a pre-existing (pre-this-feature) state file:
   - `no_graphite_flag`: `false`
   - `auto_pr_flag`: `false`
   - `pr_create_commands`: `[]`
   - `graphite_available` and `gh_available`: re-run the Step 0.5 probe (both `gt` and `gh`) to populate them.
4. Evaluate each PR node's status:
   - **`completed`**: Skip. The commit SHA is already recorded.
   - **`failed`**: Clean up the old worktree first (if `worktree_path` is set and `worktree_cleaned` is false, run `git worktree remove --force <worktree_path>`), then reset to `"pending"`. Clear error, subagent_id, reviewer_subagent_id, worktree_path, base_sha, commit_sha, review_rounds, worktree_cleaned. This PR will be retried from scratch with a new worktree.
   - **`skipped`**: Re-evaluate. If all dependencies are now `"completed"`, reset to `"pending"`. Otherwise, keep as `"skipped"`.
   - **`implementing`** or **`reviewing`**: The previous run crashed mid-execution. Clean up the old worktree first (if `worktree_path` is set and `worktree_cleaned` is false, run `git worktree remove --force <worktree_path>`), then reset to `"pending"` for a clean retry. Clear subagent_id, reviewer_subagent_id, worktree_path, base_sha, commit_sha, review_rounds, worktree_cleaned.
   - **`pending`** or **`branch_created`**: Keep as-is.
5. If `stack_assembly_started` is true but `graphite_stack_submitted` is false (recall: in plain-git mode this flag means "Step 8b finished" — so the gate fires for both modes):
   - The previous run crashed during stack assembly.
   - **Graphite mode (`graphite_available == true`)**: clean up partial Graphite state — check for branches tracked by Graphite with `gt ls`, delete any partial branches with `gt delete`, reset to main.
   - **Plain-git mode (`graphite_available == false`)**: bring the orchestrator's workspace back to a known baseline *before* deleting any branches. The resumption path must tolerate arbitrary in-flight state from the prior crash — the two realistic mid-Step-8a crash sites are (a) after `git checkout -B "<pr.branch>"` with HEAD on a doomed branch, and (b) mid-cherry-pick with unstaged conflict markers in the working tree (in which case a plain `git checkout main` would refuse with "Your local changes to the following files would be overwritten by checkout."). Cleanup sequence handles both:
     ```bash
     git cherry-pick --abort 2>/dev/null || true
     git merge --abort 2>/dev/null || true
     git rebase --abort 2>/dev/null || true

     git fetch origin main
     git checkout -f main
     git reset --hard origin/main

     while IFS= read -r branch; do
       git branch -D "$branch"
     done < <(git for-each-ref --format='%(refname:short)' "refs/heads/execute-plan/<PLAN_ID>-*")
     ```
   - Set `stack_assembly_started` to false and clear `stack_assembly_progress`.
6. Re-read the design document to restore full context.
7. Resume execution from Step 3 (branch preparation for any PRs that need branches) and Step 4 (execution loop).

Report: `"Resuming PLAN_ID <PLAN_ID>. <N> completed, <M> to retry, <K> skipped."`

## Step 8: Stack Assembly

After **all** PRs in the linearized order are either `"completed"` or `"failed"`/`"skipped"`, and at least one PR completed successfully, assemble the stack. The exact mechanism branches on `graphite_available`:

- If `graphite_available == true` -> use **Step 8a (Graphite mode)** below.
- If `graphite_available == false` -> skip Step 8a and use **Step 8a (plain-git mode)** instead.

Step 8b (PR URL collection) and Step 8c (reporting) likewise branch.

Update `stack_assembly_started` to `true` in the state file before beginning.

### Step 8a (Graphite mode): Build the Stack

Work in the main workspace. The stack is built in the linearized topological order, skipping any failed/skipped PRs.

**Per-PR prologue — free the branch name.** Apply Rule 3 of the *Subagent Worktree Protocol* before touching the PR's branch ref: tear down the subagent worktree so `gt create` can move/recreate `refs/heads/<pr.branch>` cleanly. Run the cleanup unconditionally — it is a no-op when the worktree is already gone or never existed (e.g., a `failed`/`skipped` PR was never assigned one, or a `--resume` of a partial Step 8a has already cleaned it):

```bash
if [ -n "<pr.worktree_path>" ] && [ -d "<pr.worktree_path>" ]; then
  git worktree remove --force "<pr.worktree_path>"
fi
```

After cleanup, perform the per-PR stack step:

```bash
git checkout main
git pull origin main

gt create "<pr.branch>" --no-interactive

git cat-file -t <commit_sha>

git cherry-pick <base_sha>..<commit_sha> --allow-empty


gt submit --stack --no-edit --no-interactive
```

(`git checkout main` and `git pull origin main` are the one-time prologue at the very top of the loop, not per-PR. `gt submit --stack` is the one-time epilogue after all PRs have been added with `gt create`.)

Update `graphite_stack_submitted` to `true` in the state file. After the per-PR cleanup runs successfully, set `pr.worktree_cleaned = true` in the state file so Step 9 knows to skip this worktree.

**Cherry-pick conflict handling:**
- Cherry-pick conflicts should be rare because dependent PRs were implemented on branches containing their dependencies' changes.
- If a conflict occurs, the orchestrator resolves it directly — do **not** stop or ask the user:
  1. Identify the conflicting files from the cherry-pick output.
  2. Read each conflicting file in full using `read_file`. During cherry-pick, `HEAD` (top section) is the current stack tip (Graphite branch in Graphite mode, plain-git branch otherwise); the bottom section is the PR's incoming changes.
  3. Resolve by combining both sides' intent: keep non-overlapping additions in logical order, merge overlapping edits preserving the PR's intended new behavior.
  4. Remove **all** conflict markers (`<<<<<<<`, `=======`, `>>>>>>>`).
  5. Write resolved content using the `write` tool.
  6. Stage resolved files: `git add <resolved_files>`
  7. Continue the cherry-pick: `git cherry-pick --continue`
  8. After resolving, verify the result compiles (scope to affected targets, e.g. `bazel build //pkg:target`). If verification fails, attempt to fix the resolution; if still failing, treat as unresolvable (step 9).
  9. If the conflict is truly unresolvable (fundamentally incompatible changes), abort (`git cherry-pick --abort`), mark the PR as `"failed"` with conflict details, cascade-skip dependents, and continue assembling the remaining stack.

**Why range cherry-pick (`base_sha..commit_sha`):** Each PR may have multiple commits (initial implementation + fix commits from review cycles). The range cherry-pick replays all of them in order. `base_sha` is the branch point recorded in Step 3 at branch creation time (before any implementation commits). `commit_sha` is the final HEAD after all fix cycles. This ensures the diff shows exactly all changes that PR introduced -- not accumulated dependency changes, and not just the last fix commit.

### Step 8a (plain-git mode): Build the Stack

When `graphite_available == false`, assemble the same linearized stack using only plain git. Each PR becomes its own branch fast-forwarded from the previous PR's branch (or from `main` for the bottom of the stack).

Work in the main workspace. Skip any failed/skipped PRs. Track the previous successfully-pushed branch in a local variable `parent_branch` (initialized to `"main"`).

**Important Step 3 ↔ Step 8a interaction:** Step 3 already created each `<pr.branch>` in the main repo (via `git branch <pr.branch> <base>`). Step 4b deliberately does **not** advance that ref — it only pulls the subagent's objects into the main repo's store and records `pr.commit_sha` (Rule 2 of the *Subagent Worktree Protocol*). So by the time Step 8a runs, every `<pr.branch>` already exists locally and its tip is not guaranteed to be `commit_sha`. Step 8a needs to *reset* each branch to a new starting point (`parent_branch` from this stack-assembly pass), not create it from scratch — hence `git checkout -B` (uppercase) below. The per-PR `git worktree remove --force` immediately above the `checkout -B` is required by Rule 3 — without it the reset can fail with `cannot force update the branch '...' used by worktree at '<WT>'`.

```bash
test -z "$(git status --porcelain)" || { echo "Working tree dirty; aborting"; exit 1; }
git fetch origin main
git checkout main
git reset --hard origin/main
parent_branch="main"
```

Then, per PR (in linearized stack order):

```bash
# Free the branch name (Rule 3 of the Subagent Worktree Protocol).
# Idempotent — no-op when the WT is already gone, missing, or was never
# assigned (e.g., a failed/skipped PR). Without this, `git checkout -B
# "<pr.branch>"` can fail with
# `cannot force update the branch '<pr.branch>' used by worktree at '<WT>'`.
if [ -n "<pr.worktree_path>" ] && [ -d "<pr.worktree_path>" ]; then
  git worktree remove --force "<pr.worktree_path>"
fi

git checkout -B "<pr.branch>" "$parent_branch"

git cat-file -t <commit_sha>

git cherry-pick <base_sha>..<commit_sha> --allow-empty

git push --force-with-lease origin "<pr.branch>"

parent_branch="<pr.branch>"
```

After the cleanup runs successfully, set `pr.worktree_cleaned = true` in the state file so Step 9 knows to skip this worktree.

**Cherry-pick conflict handling for resolvable conflicts** is identical to Graphite mode (see the Cherry-pick conflict handling block above). The resolution protocol — read each conflicting file, combine intents, remove markers, write resolved content, `git add`, `git cherry-pick --continue`, verify compile — applies unchanged.

**For unresolvable conflicts**, plain-git mode adds an explicit cleanup step before continuing the loop (the Graphite-mode block leaves cleanup to `gt` internals; plain-git needs to do it manually so the orchestrator is in a known state):

```bash
git cherry-pick --abort
git checkout main
```

Then mark the PR `"failed"`, cascade-skip its dependents, and **do not advance `parent_branch`** — the next non-skipped PR's branch is created off `parent_branch`, which still points at the last successfully-pushed branch (or `main` if nothing has been pushed yet).

**Push failure handling:** If `git push --force-with-lease` itself fails (network blip, permission revoked, stale lease, etc.), retry once after a 5-second pause. If the retry also fails, mark the PR `"failed"` with the push error, cascade-skip dependents, and **do not advance `parent_branch`**. The next non-skipped PR continues from the last successfully-pushed branch — the half-pushed stack is recorded in `stack_assembly_progress`, so a `--resume` invocation can rebuild from the last good point.

**No equivalent of `gt submit`:** PRs are not auto-created by default. After the loop completes, jump to Step 8b (plain-git mode) for URL collection / optional `--auto-pr`.

Update `stack_assembly_started` to `true` (already done) but **do not** set `graphite_stack_submitted` in this mode yet -- in Graphite mode that flag literally means "`gt submit` succeeded"; in plain-git mode it is reused as a generic "Step 8b finished" sentinel. After Step 8b completes successfully (whether `--auto-pr` ran or not), Step 8b sets `graphite_stack_submitted = true` so Step 7 resumption logic treats the run as assembly-complete.

### Step 8b (Graphite mode): Collect PR URLs

After `gt submit` succeeds, collect the PR URLs:

```bash
gt ls --json
```

Parse the output to extract PR URLs for each branch. Store in `pr_urls` in the state file.

Report: `"Stack submitted: <N> PRs. URLs: [...]"`

### Step 8b (plain-git mode): Collect PR URLs

After all branches are pushed, derive a GitHub-style compare URL for each branch in stack order. The parsing supports both `github.com` and GitHub Enterprise (`github.<host>`) remotes.

```bash
remote_url=$(git config --get remote.origin.url)
```

Parse `remote_url` into `(host, owner, repo)`. Accept any of these forms, in order:

1. `git@<host>:<owner>/<repo>[.git]` (canonical SSH)
2. `ssh://git@<host>[:port]/<owner>/<repo>[.git]` (scheme-prefixed SSH)
3. `https://[<user>[:<pass>]@]<host>/<owner>/<repo>[.git][/]` (HTTPS, optionally with embedded credentials and/or trailing slash)

A pragmatic regex that captures all three (operate against the trimmed remote URL):

```
^(?:git@|ssh://(?:[^@]+@)?|https?://(?:[^@/]+@)?)([^:/]+)[:/](?:[^/]+/)*([^/]+)/([^/]+?)(?:\.git)?/?$
```

Capture groups: `host`, `owner`, `repo`. **Strip any `user[:pass]@` segment before logging the URL anywhere** to avoid leaking credentials embedded in the remote URL.

- If `host` does not match `github.com` or `github.*` (e.g., `github.example.com` for GHE), or if the regex fails entirely, the remote is non-GitHub. Skip compare-URL synthesis and skip `gh pr create` (even if `--auto-pr` was set). Instead, populate `pr_urls` with `{branch: <pr.branch>, url: null, kind: "pushed-only", note: "non-GitHub remote (<remote_url with credentials stripped>) — open a PR via your forge UI"}` for each pushed branch so the Final Report still tells the user what was pushed and where. Continue to the final report.

Otherwise, for each PR in linearized stack order, compute:
```
compare_url = "https://<host>/<owner>/<repo>/compare/<parent_branch>...<pr.branch>?expand=1"
```
where `<parent_branch>` is the branch immediately below this PR in the stack (or `main` for the bottom).

`pr_urls` in plain-git mode is a list of objects, one per pushed branch in stack order. The exact shape per entry is:

```json
{
  "branch": "<pr.branch>",
  "url": "<URL string or null>",
  "kind": "pr"     // a real PR opened via gh pr create
              | "compare" // a GitHub compare URL the user must open manually
              | "pushed-only", // non-GitHub remote; branch is pushed but no URL synthesizable
  "note": "<optional explanatory string, e.g., for pushed-only entries>"
}
```

The `note` field is optional: it is populated for `pushed-only` entries (to explain why no URL is available — see the non-GitHub-remote branch above) and may also be attached to `compare` entries that resulted from a failed `gh pr create` invocation (to record the original error). The Final Report renders `note` verbatim when present.

**If `auto_pr_flag == true` AND `gh_available == true` AND host is GitHub/GHE:** create draft PRs by invoking `gh` in stack order:
```bash
gh pr create --base "<parent_branch>" --head "<pr.branch>" --fill --draft
```
Capture stdout (the URL of the created PR) and append an entry with `kind: "pr"` and the captured URL. If `gh pr create` fails for a single branch (rate limit, validation error, etc.), log the failure with the branch name, append an entry with `kind: "compare"` and the synthesized compare URL instead, and continue with the next PR. The downstream PR's `--base` still points at the failed PR's branch (not a PR) — that's correct, since the branch *was* pushed; the user just needs to also open a PR for the failed branch via the compare URL. Sequential — never parallel — to preserve ordering.

**Otherwise (no `--auto-pr`, or no `gh`):** populate every entry with `kind: "compare"` and the synthesized compare URL. Also build a list of suggested copy/paste commands:
```
gh pr create --base <parent_branch> --head <pr.branch> --fill --draft
```
one per PR in stack order. Store this list as `pr_create_commands` in the state file for the final report.

Persist the state file. Set `graphite_stack_submitted = true` (generic "Step 8b finished" sentinel — see the Setup notes on the dual semantics).

Report (the variant depends on the mix of entry `kind`s):
- All `kind: "pr"`: `"Stack pushed and PRs created: <N> branches. URLs: [...]"`
- All `kind: "compare"`: `"Stack pushed: <N> branches. Open these compare URLs (or run the gh pr create commands) to open PRs: [...]"`
- Mixed `kind: "pr"` and `kind: "compare"` (some `gh pr create` calls failed during `--auto-pr`): `"Stack pushed: <N> branches. <X> PRs were created via gh, <Y> need to be opened manually via the printed compare URLs."`
- Any `kind: "pushed-only"` (non-GitHub remote): `"Stack pushed: <N> branches to non-GitHub remote. Open PRs via your forge UI."`

## Step 9: Cleanup

After successful stack assembly:

```bash
for each pr in dag.nodes:
    if pr.subagent_id is not null:
        kill_task(pr.subagent_id)
    if pr.reviewer_subagent_id is not null:
        kill_task(pr.reviewer_subagent_id)

# PR worktrees are normally cleaned during Step 8a (per-PR prologue) so the
# stack branches can be safely manipulated. This loop is the safety net for
# any worktree that escaped Step 8a — e.g., the WT belonged to a `failed` or
# `skipped` PR (Step 8a never reaches them) or Step 8a crashed before
# reaching this PR. The check on `worktree_cleaned` keeps Step 9 idempotent
# on `--resume`.
for each pr in dag.nodes where worktree_path is not null
                          and not pr.worktree_cleaned:
    if directory <worktree_path> exists:
        git worktree remove --force <worktree_path>
    pr.worktree_cleaned = true

rm -f ${scratch_dir}/kimix-exec-summary-<PLAN_ID>-*.md
rm -f ${scratch_dir}/kimix-exec-review-<PLAN_ID>-*.md
```

**Keep the state file** -- the user may want to reference PR URLs or run history. It can be manually removed:
```bash
rm ${scratch_dir}/kimix-exec-plan-<PLAN_ID>.json
```

**After failure (resumable):** Worktrees for **completed** PRs are preserved (their commits are needed for stack assembly). Worktrees for **failed** or **crashed** PRs are cleaned up during `--resume` (Step 7) before retrying, since failed PRs are retried from scratch with new worktrees.

**Note:** `git worktree remove --force` is used instead of `git worktree remove` to avoid inconsistencies with the tool's internal worktree tracking (same pattern as pr-babysit skill).

## Step 10: Memory Flush

After cleanup, update the workspace memory file with patterns from this run. The orchestrator performs this directly using its own tools -- no subagent is needed. This step follows the same protocol as the `/implement` skill's Step 6 (see `.kimix/skills/implement/SKILL.md` for full helper documentation, exit codes, and file format details).

### Step 10a: Collect & Categorize This Run's Patterns

1. Use the `issue_patterns` list accumulated during Step 5b and Step 5c re-reviews across all PRs and review rounds.
2. **Generalize each pattern.** The memory file exists to help *future* runs on *different* tasks — patterns that reference this task's specific code, variable names, or domain objects are useless noise. Strip implementation-specific details (file names, variable names, type names, function names, domain-specific terms) and rewrite each pattern as a reusable principle that applies across different codebases and tasks:
   - Bad: "Missing error type `RetryableError` in retry handler list" → Good: "Missing entries in error-type or configuration allowlists"
   - Bad: "JWT token not validated for expiration" → Good: "Missing expiration/TTL validation on tokens or credentials"
   - Bad: "`calculateTotal` function exceeds 80 lines" → Good: "Functions exceeding reasonable length without decomposition"
   - Bad: "No test for `handleUserAuth` error path" → Good: "Missing tests for error/edge case paths"
   - Bad: "Missing null check on `userId` parameter" → Good: "Missing null/undefined checks on function inputs"
   If a pattern is *already* general (e.g., "Missing null checks on function inputs"), keep it as-is. If multiple task-specific patterns generalize to the same reusable principle, collapse them into one entry.
3. Categorize each generalized pattern into one of: Error Handling, Testing, Security, Code Quality, Naming, Documentation, Performance, or another short category name. Reuse existing category names from `existing_patterns_snapshot` (captured in Step 0) whenever the pattern fits.
4. For each pattern, write a concise one-line description. Keep descriptions on a single line.

### Step 10b: Harmonize Phrasing Against Existing Entries

Before handing generalized patterns to the helper, dedup at the *phrasing* level using `existing_patterns_snapshot`:

1. For each of this run's patterns, scan `existing_patterns_snapshot` for a semantically equivalent entry. If a match exists, replace this run's description with the **exact existing description string**.
2. If no match exists, keep the concise description as-is.
3. Within this run's own list, also dedup by phrasing -- collapse patterns that mean the same thing to a single entry.

### Step 10c: Build the Update Spec

Construct a JSON object:

```json
{
  "patterns": [
    {"category": "Error Handling", "description": "Missing null/undefined checks on function inputs"},
    {"category": "Testing", "description": "Missing tests for error/edge case paths"}
  ],
  "run": {
    "date": null,
    "description": "Execute plan: <design doc title or short description>",
    "rounds": <sum of review_rounds across all completed PRs>,
    "issues_by_severity": {"bug": <total>, "suggestion": <total>, "nit": <total>},
    "key_patterns": ["<2-3 most significant patterns from this run>"],
    "specializations": ["general"]
  }
}
```

Field notes:
- `run.date`: pass `null` and the helper fills in today's UTC date.
- `run.rounds`: sum of `review_rounds` across all completed PRs.
- `run.issues_by_severity`: derived from `total_issues_by_severity` (accumulated in Step 5b and Step 5c re-reviews across all PRs).
- `run.key_patterns`: pick 2-3 most-significant patterns (severity-ranked: bugs > suggestions > nits). **Apply the same generalization rules as Step 10a** — strip task-specific names and rewrite as reusable principles.
- `run.specializations`: always `["general"]` for execute-plan (only the `reviewer` persona is used).

### Step 10d: Invoke the Helper

Use the `write` tool to create `${scratch_dir}/kimix-exec-mem-${PLAN_ID}.json` with the JSON spec above, then invoke:

```bash
python3 "${MEMORY_HELPER}" update < ${scratch_dir}/kimix-exec-mem-${PLAN_ID}.json
```

The helper acquires the lock, merges, compacts, writes atomically, and prints a JSON stats summary on stdout:

```json
{
  "file": "/path/to/implement-memory/<workspace-id>.md",
  "existed_before": true,
  "stats": {
    "new_patterns": 2,
    "merged_patterns": 5,
    "categories_touched": ["Error Handling", "Testing"],
    "categories_capped": {},
    "recent_runs_dropped": 0
  }
}
```

Use these stats to report to the user:
> "Memory updated: 2 new patterns, 5 merged into existing entries."

Or if `existed_before` is `false`:
> "Memory file created with N patterns."

### Graceful Degradation

If the helper exits non-zero, log a warning with stderr and skip the memory update. The implementation is already complete and reviewed -- never fail the run due to memory flush issues. Clean up the temp JSON file regardless:

```bash
rm -f ${scratch_dir}/kimix-exec-mem-${PLAN_ID}.json
```

## Final Report

After stack assembly, cleanup, and memory flush, present a final report:

1. **Summary** -- which PRs were implemented, total files changed across all PRs.
2. **Execution stats**:
   - Total PRs: N (M completed, K failed, J skipped)
   - Levels: L, Max parallelism achieved: P
   - Effort level: E
3. **Review stats** -- total review rounds across all PRs, total issues found and fixed.
4. **Stack** -- linearized order, branch names, and URLs.
   - **Graphite mode**: label this section "Graphite stack" and list the PR URLs from `gt ls --json`.
   - **Plain-git mode**: label this section "Git branch stack" and render `pr_urls` entries by `kind`. If a `note` field is present on the entry, append it verbatim to the rendered line (in parentheses or on a continuation line).
     - `kind: "pr"`: print as "PR: `<url>`" (plus `note` if present — no current code path attaches one, but the renderer accepts a `note` field on this kind to keep the rendering rules uniform across all three kinds).
     - `kind: "compare"`: print as "Compare URL: `<url>` — open this to create the PR", append the matching command from `pr_create_commands`, and append `note` if present.
     - `kind: "pushed-only"`: print as "Pushed: `<branch>` — `<note>`" (the `note` is always present for this kind; it carries the user-facing explanation, e.g., "non-GitHub remote (<remote_url>) — open a PR via your forge UI").
5. **Failed PRs** (if any) -- which PRs failed and why, which were skipped as a result.
6. **Resumption** -- if any PRs failed: `"Resume with: /execute-plan --resume <PLAN_ID>"`
7. **Memory update** -- what patterns were written or updated in the workspace memory file. Use the `existed_before` flag from the helper's Step 10d update output to choose between "file created" and "file updated" wording.
8. **Next steps**:
   - **Graphite mode**: suggest `/pr-babysit add <bottom-pr-number>` to monitor CI and reviews.
   - **Plain-git mode**: do NOT suggest `/pr-babysit` (it depends on `gt` for stack walking — see its Step 2/3). The Stack section above already lists the URLs / commands; the Next-Steps text just adds a one-line note: "No automated CI/review monitoring is available in plain-git mode; open the PR URLs / compare URLs above and monitor them in the GitHub UI."

## In-Progress Reporting

Give the user a brief status update after each significant event:

- After parse: `"Parsed PR Plan: <N> PRs, <L> levels, max parallelism: <P>"`
- After linearize: `"Linearized stack order: PR1 -> PR4 -> PR2 -> PR5 -> PR3"`
- After branch creation: `"Created branches for <N> level-0 PRs. Starting implementation..."`
- After launching a PR: `"Launching <pr.id> (<pr.title>)..."`
- After PR implementation completes: `"<pr.id> (<pr.title>) implemented in <duration>. <N> files changed. Starting review..."`
- After review (0 issues): `"<pr.id> review: 0 issues found. PR complete."`
- After review (N issues): `"<pr.id> review: <N> issues (<X> bugs, <Y> suggestions, <Z> nits). Fixing..."`
- After fix cycle: `"<pr.id> fix applied. Re-reviewing (round <N>)..."`
- After PR fully complete: `"<pr.id> complete. <M> of <total> PRs done. <K> newly ready."`
- After failure: `"<pr.id> FAILED: <reason>. Skipping <N> dependents. <M> independent PRs continue."`
- After all PRs done: `"All PRs complete. Building <Graphite|git branch> stack..."` (pick the label based on `graphite_available`)
- After stack submission (Graphite mode): `"Stack submitted: <N> PRs. URLs: [...]"`
- After stack push (plain-git mode, no `--auto-pr`): `"Stack pushed: <N> branches. Open these compare URLs (or run the gh pr create commands) to open PRs: [...]"`
- After stack push (plain-git mode, with `--auto-pr`, all succeeded): `"Stack pushed and draft PRs created: <N> branches. URLs: [...]"`
- After stack push (plain-git mode, with `--auto-pr`, mixed results): `"Stack pushed: <N> branches. <X> PRs created via gh, <Y> need manual creation via the printed compare URLs."`
- After stack push (plain-git mode, non-GitHub remote): `"Stack pushed: <N> branches to non-GitHub remote. Open PRs via your forge UI."`
- After cleanup: `"Cleanup complete."`
- After memory flush: `"Memory updated with N patterns from this run."` (or `"Memory file created with N patterns."` for first run)
- After final report (Graphite mode only): `"Run '/pr-babysit add <bottom-pr-number>' to monitor CI."`

## Safety Guardrails

Follow these rules strictly:

- **Never force-push without `--force-with-lease`.** Always use `git push --force-with-lease`, never `git push --force`. `gt submit` handles this internally. In plain-git mode the orchestrator uses `git push --force-with-lease origin <branch>` unconditionally — the lease is a no-op for newly-created branches (no upstream to lease against), but protects the resume case, the rare concurrent-push case, and the re-resolved-conflict case.
- **Never modify the main branch.** The orchestrator checks out `main` only during stack assembly (to cherry-pick onto stack branches). Never commit directly to main.
- **Orchestrator owns all git/stack-tooling operations, including merge conflict resolution.** Subagents implement code and commit in worktrees. They never create branches, push, or interact with Graphite or `gh`. The orchestrator resolves merge conflicts directly during branch preparation (Step 3) and stack assembly (Step 8a) using `read_file` and `write` — this is the sole exception to the "no source file modification" rule.
- **Sequential stack assembly operations.** All branch-mutating and remote-mutating operations during Step 8 (Graphite: `gt create`, `gt submit`; plain-git: `git checkout -B`, `git cherry-pick`, `git push --force-with-lease origin`, `gh pr create`) run sequentially. Never run them concurrently — ordering is load-bearing for stack consistency.
- **Follow the Subagent Worktree Protocol exactly.** It is the only contract for moving commits and refs between a subagent's worktree and the orchestrator's main repo:
  - Step 4b/5c fetch with `git fetch <WT> HEAD --no-tags` — no destination refspec, no `--force`.
  - Step 3, 4b, 5c, 8a all treat `pr.commit_sha` (not `refs/heads/<pr.branch>`) as the authoritative reference for a PR's changes.
  - Step 8a tears down each PR's subagent worktree with `git worktree remove --force` immediately before manipulating that PR's branch ref.

  See the *Subagent Worktree Protocol* section near the top of this document for the full rules. There is no git lock contention between worktrees. Subagent prompts instruct retry on git lock errors as a general safety measure, but contention is not expected.
- **All fix work happens in worktrees, never in the main workspace.** The orchestrator's workspace tree must not be modified by subagents.
- **Use `git worktree remove --force` for subagent worktrees** (created by `isolation: "worktree"`). This avoids inconsistencies with the tool's internal worktree tracking. **Use `git worktree remove` for temporary merge worktrees** created directly by the orchestrator in Step 3 (diamond dependency handling) -- these are not tracked by the tool's internal registry.
- **Persist state after every status transition.** If the session crashes, the state file enables resumption.
- **Never merge PRs.** The skill creates and submits PRs. Merging is a human decision.
- **Explicit composition with `/pr-babysit` (Graphite mode only).** After stack submission, the user runs `/pr-babysit add <bottom-pr-number>` separately. The skill does not auto-chain. In plain-git mode, `/pr-babysit` is not suggested because it depends on `gt` for stack walking (it invokes `gt checkout`, `gt bottom`, `gt up`, `gt sync`).

## Rules

- **Inject personas into prompts** -- always prepend the `implementer` or `reviewer` persona instructions (from the shared personas file) to the subagent prompt on initial launches. Pass the persona **name** via `task.persona`. On `resume_from` follow-ups, the persona is already in the subagent's transcript from the initial launch.
- **Prefix `description` with a bracketed role tag** -- `[implementer]` for implementer launches and fix cycles, `[reviewer]` for reviewer launches and re-reviews. The pager parses the first `[tag]` of the description and uses it as the row label (the bracketed prefix is stripped from the displayed description). Without this, every row falls through to the generic "General" label because `subagent_type` is `general-purpose` and no persona/role is plumbed through the spawn args. Keep the tag on `resume_from` follow-ups so the label stays stable across rounds.
- **`isolation: "worktree"` for implementers only** -- implementers get fresh worktrees. Reviewers use `cwd=<worktree_path>` to access an existing worktree.
- **Include design context in initial prompts** -- the initial implementer and initial reviewer launches should include all relevant design context in their task prompts. All subsequent rounds use `resume_from` instead, which carries the subagent's own transcript forward.
- **`resume_from` for fix and re-review cycles** -- when resuming an implementer to fix review issues, use `resume_from` with the stored `subagent_id` (implementer -> implementer). When resuming a reviewer for subsequent review rounds, use `resume_from` with the stored `reviewer_subagent_id` (reviewer -> reviewer). Both are same-persona resumptions.
- **`background: true` for parallel launches** -- implementation subagents are launched in background. Use `wait_commands_or_subagents(task_ids=[...], mode="wait_any")` for event-driven waiting.
- **Save every subagent_id** returned by `task` -- needed for `resume_from` on both implementer fix cycles and reviewer re-review rounds. Update `pr.subagent_id` after each implementer task and `pr.reviewer_subagent_id` after each reviewer task.
- **Read review files yourself** after each review to count open issues and decide whether to continue.
- **Do not modify the implementation yourself** -- all code changes go through the implementer persona subagent.
- **Explicitly tell subagents to write their output files** -- include file paths and expected content in every prompt.
- **Thread the same file paths** across all rounds per PR -- never generate new paths between iterations.
- **Include the design doc context in subagent prompts** -- read the design document and include the relevant sections for each PR in the implementer prompt.
- **Branch creation is just-in-time** -- create branches for dependent PRs only after their dependencies complete. Level-0 branches are created upfront.
- **State file is the source of truth** -- persist after every status transition. On resume, the state file determines what to retry.
- **Cascade-skip, do not abort** -- a failed PR causes its transitive dependents to be skipped, but independent PRs continue.
- **Error handling** -- if a subagent fails or cannot be resumed, mark the PR as failed, cascade-skip dependents, and continue with independent PRs. Only stop the entire run if no PRs can make progress.
- **Context management for resumed subagents** -- `resume_from` carries the full prior transcript. If a review-fix loop exceeds 4 rounds, consider killing the reviewer task and launching a fresh reviewer (with `cwd`) to prevent token overflow. Reset `pr.reviewer_subagent_id` to the new task id. The same applies to implementers if fix cycles are unusually long.
- **Stack assembly is batch, not incremental** -- wait for all PRs to complete (or fail/skip) before assembling the stack. Do not assemble incrementally.
- **Respect concurrency limits** -- never launch more than `max_concurrent` subagents simultaneously. The ready-queue loop enforces this.
- **`past_issues_briefing` must be consistent and capped** -- before launching any implementer or reviewer, the orchestrator must build the `past_issues_briefing` from the `issue_patterns` list accumulated so far. If the list exceeds 20 patterns, include only the top 20 by occurrence count (most frequently seen patterns first). Never partially include a pattern or truncate mid-description. If `issue_patterns` is empty (first PR, no prior reviews), omit the block entirely rather than including an empty block.
- **Memory is best-effort** -- if the `memory.py` helper fails (lock contention, malformed spec, disk full, etc.), proceed without it. Never fail a run due to memory issues.
- **Use the implement skill's memory.py helper** -- derive the absolute path from the implement skill's SKILL.md path announced in the system context (see Step 0 for derivation), not from `$(pwd)`. The execute-plan skill shares the same workspace-scoped memory file as `/implement`.

