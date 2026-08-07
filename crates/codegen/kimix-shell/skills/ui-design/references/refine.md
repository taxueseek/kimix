# Refine

Refine changes the character of an existing design without pretending it is a new brief. I choose the move that fits the diagnosis, not the move that sounds fun.

Refinement is a designer's hand on the volume, density, clarity, resilience, and pleasure of a surface.

---

## Pre-execution checklist

Before proceeding, check for existing reports in the `.kimix/ui-design/` directory. Look for these files:

- `checkup-report.md`
- `review-report.md`
- `smell-report.md`

If any of these files exist, read the report content and use it as context for your analysis. Prioritize issues flagged in the reports and reference specific findings when making changes.

If no reports are found, proceed with the task normally.

---

---

## Composition Pressure

Refinement starts by checking whether the composition matches the work pattern.

Monitor should become clearer about priority and change.

Operate should bring tools, targets, and feedback closer together.

Compare should strengthen alignment, sorting, filtering, and scan rhythm.

Configure should clarify groups, dependencies, preview, and commit.

Learn should improve pacing, measure, and progression.

Decide should remove distraction around proof and action.

Explore should make paths, filters, and backtracking easier.

I refine toward the job. I do not polish the wrong composition.

---

## Transformation Bar

`/ui-design refine` must visibly change the surface in the chosen direction.

At minimum, I name the move, apply it to the relevant parts of the surface, and verify that the rendered result changed in character, clarity, resilience, or delight.

One small detail, one hover effect, or one copy edit does not count as refinement unless the user explicitly asked for that exact detail.

---

## How I Choose The Move

If the design is safe and flat, I **push** it — stronger commitment, sharper point of view.

If the design is loud and exhausting, I **settle** it — less competition, more control.

If the design is cluttered, I **strip** it — remove what does not serve the primary job.

If the design breaks under real data, I **proof** it — test the edges, fill the gaps.

If first-time users cannot reach value, I **activate** them — shorter path, useful defaults.

If the design works but has no joy, I add **texture** — moments that feel memorable.

If the surface should feel technically extraordinary, I **push past limits** — cinematic or invisible speed.

I do not combine opposing moves unless the surface has separate zones that need separate treatment.

---

## Push

Bolder means more committed, not more effects.

For brand, I increase distinctiveness: stronger type, unexpected color, sharper composition, a hero moment with a point of view, and a lane that is not the median generated page.

For product, I increase clarity: stronger hierarchy, clearer primary action, better density, tighter inactive states, more useful contrast. Product boldness usually looks like confidence, not spectacle.

I refuse purple gradients, glass panels, neon on dark, gradient text, and bounce as the answer to "make it stronger."

I know it worked when the focal point jumps out and the surface still feels coherent.

---

## Settle

Quieter means reducing intensity without erasing character.

I lower saturation where the eye has no rest. I reduce competing focal points. I flatten unnecessary depth. I remove motion that does not explain state. I let typography and spacing carry more of the hierarchy.

Quiet does not mean gray. It means controlled.

I know it worked when one thing leads, everything else supports, and the brand voice is still present.

---

## Strip

Stripping means removing obstacles, not gutting capability.

I find the primary job of the surface. Then I remove, combine, hide, or demote everything that does not help that job happen.

Redundant copy goes. Repeated actions merge. Decorative containers flatten. Secondary controls move behind disclosure when they are not needed yet. The palette shrinks. The type system loses unnecessary steps.

If cutting something changes the product's promise, I stop and ask.

I know it worked when the path is faster and the remaining elements feel inevitable.

---

## Proof

Proofing means the interface survives reality.

I test long names, empty content, huge lists, extreme numbers, slow networks, offline states, permissions, rate limits, validation failures, concurrent actions, RTL (see [responsive.md](responsive.md#text-direction)), CJK, accents, emoji, and translated strings.

I expect German to expand. I expect a user to click twice. I expect the API to fail. I expect a tiny screen and a huge one. I expect data to be missing.

I know it worked when ugly data no longer creates ugly UI.

---

## Activate

Activation is not a tour. It is the shortest path to first value.

I identify the aha moment and design toward it. I teach by letting the user do real work. I use empty states, defaults, templates, sample data, and contextual help before I reach for a ceremony.

Skip is real. Power users should not be trapped in training.

I know it worked when users can reach value quickly and still find their way after skipping.

---

## Texture

Texture earns a place at moments, not everywhere.

Completion, first-use, milestones, meaningful waiting, useful recovery, tactile interaction, or a small discovery can carry texture. Routine work does not need celebration on every click.

Brand can distribute texture across the page. Product should concentrate it at moments where emotion is already present.

I know it worked when the moment feels memorable without slowing the task.

---

## Push Past Limits

Pushing past limits means technical ambition in service of the interface.

For a brand surface, it might be a scroll scene, shader, cinematic transition, generative visual, or advanced type treatment. For a product surface, it might be instant search over huge data, a dialog that morphs from its trigger, a table that stays smooth under massive load, or validation that feels immediate.

Before building at this level, I name the ambition and the risk. Some surfaces need spectacle. Some need invisible speed.

I know it worked when removing the effect would make the experience feel meaningfully worse.

---

## What I Refuse

- Push as more generic effects and more noise
- Calling small detail polish a character change
- Claiming a change that is not visible in the rendered result
- Settle as making everything gray
- Strip as removing necessary capability
- Proof as theory with no real edge data
- Activate as a forced feature tour
- Texture on critical errors or destructive moments
- Pushing past limits in ways that hurt performance, accessibility, or context
- Markdown refinement reports

---

## How I Know Refinement Is Done

- The chosen move matches the diagnosis
- The rendered result visibly changed in the chosen direction
- The design has changed in character, not just detail
- The primary task is clearer than before
- The register still fits brand or product
- Edge states are not worse
- The result does not need another opposite refine pass to recover

STRICT RULE — NEVER BREAK THIS
Do not create report.md, any kind of report, summary, analysis file,
or extra documentation. This applies every time this file is used.
Generate no reports unless explicitly asked.
