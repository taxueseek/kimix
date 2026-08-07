# Smell: `/ui-design smell`

I use smell to catch generic design before it spreads. This is not a broad review. It is a detector for reflex, template, and generated sameness.

When a surface smells wrong, I fix the category reflex first. Cosmetic edits do not remove the odor.

---

## Composition Smell

I check whether the composition came from the work or from habit.

Monitor smell: status hidden inside equal cards, alerts without priority, live data treated as static decoration.

Operate smell: tools far from objects, no inspector, no command surface, no fast feedback.

Compare smell: comparison forced into unrelated cards, missing alignment, weak filters, unstable scan paths.

Configure smell: settings scattered by visual balance instead of dependency and consequence.

Learn smell: sections arranged as feature tiles when the user needs a path.

Decide smell: too many equal calls to action, proof buried below ornament.

Explore smell: search, filters, results, and backtracking treated as afterthoughts.

Centered grid is not automatically wrong. It is wrong when it has no work-pattern reason to exist.

---

## Prompt Drift Smell

I treat prompt drift as a severe smell.

Wrong name, recycled logo mark, reused headline structure, inherited proof object, generic visual artifact, and domain objects from an unrelated brief all mean the design is not listening.

The fix is not copy editing. The fix is returning to the current prompt's name, category, user, job, artifact, evidence, and refused drift.

---

## Evidence Bar

`/ui-design smell` names generic design tells from the actual surface. It does not invent odor.

At minimum, I identify the visible pattern, the reflex behind it, why it weakens this specific brief, and the right mode to fix it.

If a smell is only a suspicion, I mark it as a suspicion. I do not report it as observed.

---

## What Smell Means

A smell is a choice that looks unchosen.

It may be competent. It may even be accessible. The problem is that it could have come from any prompt, any template, any average SaaS homepage, any safe default.

I look for the moment where the design stopped making project-specific decisions.

---

## The Odors I Track

**Tech gradient**: blue-violet, indigo-cyan, and purple-to-teal glossy energy plastered on heroes, CTAs, cards, or text. The visual shorthand for "AI startup."

**Generic tech hue**: blue-purple as the primary identity for anything vaguely technical or software-adjacent.

**Feature tile grid**: icon, heading, one sentence, repeated in a uniform grid until the section stops meaning anything. Every card equal, nothing prioritized.

**Accent rail**: a colored stripe on one side of cards or callouts added to simulate structure. Decoration pretending to be organization.

**Unearned blur**: frosted glass panels applied because the surface never committed to a depth system.

**Stat monument**: an oversized number cluster filling space where a real product story belongs.

**Icon topper**: a rounded-square icon placed above every section heading with no function beyond filling the template.

**Bounce everywhere**: motion that turns every interaction into a toy. Elastic easing applied because it was available.

**Default type**: a common family used with no voice, no scale, no reason. The font that appeared because no choice was made.

**Center stack**: everything aligned to the safe middle because no composition decision was made.

---

## The Domain Default Trap

I ask whether the visual direction could be guessed from the industry.

A note-taking app as cream and rounded sans. A developer tool as dark with terminal mono. A health product as white and calm blue. A legal platform as navy and serif. A food app as warm orange. A payments product as clean white with green accents.

If the answer was obvious before I opened the page, the design has not found itself yet.

---

## What I Look For Instead

The design needs at least a few real decisions:

- A color strategy that is not the domain's first reflex
- Type with a reason
- A composition that chooses tension, rigor, image, or editorial pacing
- Imagery or visual material tied to the subject
- Motion that reveals character or state
- Copy with a specific voice
- An interaction or detail that could only belong here

Unexpected is not automatically good. But a page with nothing unexpected is usually forgettable.

---

## How I Judge Severity

I treat faint smells as cleanup. I treat clustered smells as identity failure.

A single generic icon card can be replaced. A whole page made of generic cards, indigo gradients, centered hero, Inter, and vague copy needs a new lane.

If the smell is structural, I do not patch. I change the direction.

---

## What I Do After Finding Smell

I name the dominant smell. I identify the root reflex. Then I pick the right design tool.

- Color reflex goes to recolor
- Type reflex goes to typeset
- Composition reflex goes to relayout or redesign
- Generic brand lane goes to voice or redesign
- Missing state and interaction smell goes to interaction
- Unclear copy goes to writing

If the design smells in several systems at once, I suggest redesign is usually cleaner than incremental repair.

**Important:** Smell never executes any mode. It only produces `.kimix/ui-design/smell-report.md` and `.kimix/ui-design/smell-report.html`.

---

## Scoring

Smell uses a `/10` score. The score is **inverted** — finding nothing is perfect.

| Tells found | Score |
|---|---|
| 0 | 10/10 — CLEAN |
| 1–2 | 7–8/10 — FAINT |
| 3–4 | 5–6/10 — PRESENT |
| 5–6 | 3–4/10 — STRONG |
| 7+ | 0–2/10 — IDENTITY FAILURE |

**MAX_SCORE = 10.** Use `/10` as the denominator in the report template. When zero tells are found, the score is `10/10` and the verdict is CLEAN. Never output `0/10` to mean "no smells detected."

The heuristics table in the report uses 10 rows (one per odor tracked). Each row scores `1` if the odor is absent, `0` if detected.

---

## Report Boundary

Smell always produces two report artifacts:

- `.kimix/ui-design/smell-report.md`
- `.kimix/ui-design/smell-report.html`

**Important Rule for Generating HTML Report:**

- **Use the following template structure** [report-html.md](report-html.md) to generate `smell-report.html`:
- Do not change the visual design. Only fill in the content.

These are the only report artifacts smell creates.

---

## What I Refuse

- Calling a design clean because it passes technical checks
- Reporting a smell I cannot point to
- Treating personal taste as evidence of generated design
- Fixing an AI gradient by choosing a different AI gradient
- Treating Inter as wrong when it is clearly intentional
- Treating all centered layouts as bad when symmetry is the right lane
- Adding decoration to hide generic structure
- Creating extra report artifacts beyond `smell-report.md` and `smell-report.html`

---

## How I Know The Smell Is Gone

- Every named smell maps to a visible choice
- The palette cannot be guessed from the domain alone
- The type has a project-specific reason
- The composition is not the median generated landing page
- Repeated sections have hierarchy and variation
- The strongest visual idea belongs to this brief
- A stranger would not immediately say the page was generated
- `smell-report.md` and `smell-report.html` both exist

STRICT RULE — NEVER BREAK THIS
Always create .kimix/ui-design/smell-report.md and
.kimix/ui-design/smell-report.html. Do not create any other report,
summary, analysis file, or extra documentation.
