# Deslop: `/ui-design deslop`

Deslop removes AI slop from a design surface. I don't repaint, I don't polish, and I don't add new effects. I read all three diagnostic reports, find every generic tell, and replace each one with a real decision.

This mode is the treatment that follows the full diagnostic pass. Reports find the rot. Deslop cuts it out and replaces it with authored work.

---

## Prerequisites: All Three Reports

Before I touch anything, I check `.kimix/ui-design/` for all three reports:

- `smell-report.md`
- `checkup-report.md`
- `review-report.md`

**If any report is missing**, I generate it before proceeding:

- Missing `smell-report.md` → read [smell.md](smell.md) and run `/ui-design smell`
- Missing `checkup-report.md` → read [checkup.md](checkup.md) and run `/ui-design checkup`
- Missing `review-report.md` → read [review.md](review.md) and run `/ui-design review`

I never skip a missing report. Each one surfaces a different class of problem and the fix priority depends on all three together.

---

## How I Work From The Reports

I read all three reports front to back, then extract and rank every finding by severity:

1. **Critical checkup vitals first** — anything scored Critical in `checkup-report.md` blocks shipping and gets fixed before anything else
2. **Review failures second** — low-scoring lenses in `review-report.md` (first impression, hierarchy, color voice, type voice, interaction feel)
3. **Strong smells last** — clustered or strong tells from `smell-report.md`; faint isolates get cleaned after everything above is resolved

For each finding I address:

1. **I name the reflex it came from** — what lazy default produced this pattern
2. **I pick a deliberate replacement** — not the opposite, not a different default, but a choice tied to the project
3. **I apply the change to real files** — no markdown mockups, no commentary-only passes
4. **I verify the result** — the old pattern is gone and the new choice reads as intentional

I fix every finding the reports name. I do not skip faint smells. A few faint smells clustered in one section is a strong smell in waiting.

---

## The Odors And Their Antidotes

### Tech Gradient

Blue-violet, indigo-cyan, purple-to-teal glossy energy. The visual shorthand for "AI startup."

**Fix**: I build a palette with a specific reason. The hue must come from the product's domain, not the industry. A logistics tool might earn slate and amber. A reading app might earn warm paper and ink. I refuse purple-to-cyan and blue-violet-to-indigo as category reflexes.

### Generic Tech Hue

Blue-purple as the primary identity for anything vaguely technical.

**Fix**: I replace the identity hue with one that has project-level justification. If the product handles water data, teal is earned, not guessed. If it manages gardens, green is earned. If there is genuinely no domain anchor, I pick an unexpected saturation or temperature and commit to it fully.

### Feature Tile Grid

Icon, heading, one sentence, repeated in a uniform grid until the section stops meaning anything.

**Fix**: I break the grid. I give the feature section hierarchy — lead with the strongest feature, vary the layout rhythm, or kill the section entirely if it's filler. Cards are not automatically wrong. Equal cards with no priority are always wrong.

### Accent Rail

A colored stripe on one side of cards or callouts that simulates structure.

**Fix**: I remove the rail. If the card needs differentiation, I use density, type weight, or border treatment. If it doesn't need differentiation, the rail was decoration.

### Unearned Blur

Frosted glass panels applied because the surface never committed to a depth system.

**Fix**: I remove the blur. If depth is needed, I build a real elevation system with shadow, border, or opacity layers. If depth isn't needed, the element becomes opaque and sits flat in the composition.

### Stat Monument

An oversized number cluster filling space where a real product story belongs.

**Fix**: I replace stats with proof. A number alone proves nothing. I convert stat blocks into case language — a before/after, a customer outcome, a specific metric with context, or a product capability shown in action. If the stat is real and meaningful, I give it a sentence, not a monument.

### Icon Topper

A rounded-square icon placed above every section heading with no function beyond filling the template.

**Fix**: I remove the icon. Section headings do not need a decoration above them. If the icon carries real meaning, I bring it inline with the heading text. If it doesn't, it goes.

### Bounce Everywhere

Motion that turns every interaction into a toy. Elastic easing applied because it was available.

**Fix**: I audit every animated element. I keep motion that reveals state, responds to input, or marks meaningful transitions. I strip motion that is purely decorative. What remains uses sharp deceleration curves — quart, quint, or expo out. No bounce. No elastic.

### Default Type

A common family used with no voice, no scale, no reason. The font that appeared because no choice was made.

**Fix**: I either commit to the current family with a real reason and a tuned scale, or I switch to a family that has project-level intention. Inter is not wrong. Inter with no scale, no weight contrast, and no voice is wrong. System fonts for product UI are legitimate. Brand surfaces need a deliberate type choice.

### Center Stack

Everything aligned to the safe middle because no composition decision was made.

**Fix**: I choose a composition based on the dominant work pattern — monitor, operate, compare, configure, learn, decide, or explore. Centered is valid when symmetry is the right answer. Centered as the default is not.

---

## Domain Default Trap

If the visual direction can be guessed from the industry alone, the design hasn't found itself yet.

A note-taking app as cream and rounded sans. A developer tool as dark with terminal mono. A health product as white and calm blue. A legal platform as navy and serif.

**Fix**: I identify the domain default and break it in at least one dimension — unexpected saturation, a different temperature, an unusual composition, a deliberate texture, or a specific art direction choice that only fits this product.

---

## Fixing Order

I work in this order because each fix ripples into the next:

1. **Composition** — center stack, feature tile grid, accent rail. These are structural.
2. **Color** — tech gradient, generic tech hue, domain default trap. These set the mood.
3. **Type** — default type. This gives the surface its voice.
4. **Depth** — unearned blur, stat monument. These are spatial decisions.
5. **Motion** — bounce everywhere. Motion comes last because it reacts to the settled composition.
6. **Decoration** — icon topper. These are the last things to address.

I don't jump to color before fixing composition. A new palette on a broken layout is still broken.

---

## What A Fixed Smell Looks Like

A smell is fixed when:

- The old pattern is not visible anywhere on the surface
- The replacement is not another template reflex — it's a specific choice
- The choice would survive the prompt changing — it belongs to the product, not the request
- The section or element still does its job, now with intention
- The fix does not create a new smell in another category

---

## Cohesion Check

After fixing all named smells, I scan the full surface for new tension.

Sometimes removing one smell exposes another. Sometimes the fixes create visual whiplash between sections. If two sections now feel like different products, I tune the border between them until they share the same DNA.

---

## Verify Like A Designer

After applying all fixes, I run a designer verification pass before calling it done.

**Stranger test**: Would a stranger still say "AI made that" without hesitation? If yes, I missed something. I return to the reports and look for tells I dismissed as faint.

**Regression check**: Did the fixes break anything new? I scan every section I touched for new tension, whiplash, or introduced smells.

**Reality check**: Are the changes real and visible in actual files? No commentary-only passes count.

**Judgment check**: Would a working designer approve each decision? Not just accept — actively approve it as the right call for this product.

If any check fails, I return to the reports and fix what was missed.

---

## Ship or Iterate

If all four verification checks pass, deslop is done.

If any check fails, I go back to the reports — not to polish, but to find what the reports named that I didn't fully address. I do not invent new work. I finish the work the reports already defined.

---

## What I Refuse

- Fixing a smell by swapping in a different AI default
- Removing a smell without replacing it with a real decision
- Polishing around the smell instead of removing it
- Calling a recolor pass a deslop pass
- Fixing only the hero section when the smell is structural
- Treating Inter as wrong when it is clearly intentional
- Treating all centered layouts as bad when symmetry is the right lane
- Adding decoration to hide generic structure
- Creating any reports, summaries, or documentation beyond the work itself

---

## How I Know Deslop Worked

- Every finding from all three reports has been addressed in real files
- The surface reads as authored, not generated
- The color strategy cannot be guessed from the industry
- The type has a project-specific reason
- The composition is not the median generated landing page
- Repeated sections have hierarchy and variation
- A stranger would not immediately say the page was generated
- No new smells were introduced
- The product still works and still does its job

STRICT RULE — NEVER BREAK THIS
Deslop checks `.kimix/ui-design/` for all three reports before touching any file.
Missing reports are generated on the spot by running the corresponding design mode.
It never creates extra reports, summaries, analysis files, or documentation beyond the three diagnostic reports.
After fixes, it always runs the designer verification pass and ships or iterates based on the result.
