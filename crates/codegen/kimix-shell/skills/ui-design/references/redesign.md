# Redesign: `/ui-design redesign`

Redesign makes the same product feel like a different visual product. The job, content, and core flows stay intact. The visual world changes.

This is not polish. This is not relayout. This is not color work. Redesign replaces the surface language end to end.

---

## Discipline files

This mode drives the full redesign. These files are reference material — consult them when each dimension comes up during the redesign, not as a separate pass:

- [color.md](color.md) — for correct palette construction when choosing the new color system
- [typeset.md](typeset.md) — for correct type hierarchy when building the new type voice
- [layout.md](layout.md) — for correct spatial logic when choosing the new composition
- [border.md](border.md) — for correct edge language when choosing the new radius and frame system
- [shadow.md](shadow.md) — for correct depth decisions when setting or removing elevation
- [motion.md](motion.md) — for correct timing and easing when defining the new motion personality
- [button.md](button.md) — for correct control states when replacing the component language

---

## Pre-execution checklist

Before proceeding, check for existing reports in the `.kimix/ui-design/` directory. Look for these files:

- `checkup-report.md`
- `review-report.md`
- `smell-report.md`

If any of these files exist, read the report content and use it as context for your analysis. Prioritize issues flagged in the reports and reference specific findings when making changes.

If no reports are found, proceed with the task normally.

---

## Composition Reset

I do not redesign by repainting the old layout.

I identify the product's dominant work pattern and choose a new composition from that job: monitor, operate, compare, configure, learn, decide, or explore.

The new direction must change the spatial premise. A dashboard can become a command center, a comparison wall, or a focused operator console. A landing page can become a proof trail, an editorial story, a product demo, or a decision surface. A settings page can become a guided configuration path or a dense control panel.

The composition shift must be visible before color, type, or motion are noticed.

---

## Full-Surface Bar

`/ui-design redesign` must transform the surface as a system. It is not a hero-only makeover.

At minimum, I change composition, color roles, type hierarchy, component language, buttons or controls, edges, depth, states, and responsive behavior where those parts exist.

If the old design is still recognizable after swapping only colors and font sizes, the redesign failed.

---

## What Stays

I preserve the user's goals, task logic, content priority, accessibility bar, performance expectations, and core flow.

If the interface worked functionally before, it should still work after. Ideally it works with more clarity.

---

## What Changes

Every visual system is open.

Color gets a new mood and semantic vocabulary. Typography gets a new voice and hierarchy. Layout gets a new composition logic. Components get new shape, density, and affordance. Borders and radii get a new edge language. Shadows get a new depth model, or disappear. Motion gets a new personality. Imagery and iconography get a new treatment.

Old tokens do not carry forward by accident. Anything reused is deliberately re-adopted.

---

## The Direction

A redesign needs a creative direction before it needs values.

I name the concept in plain language. I name mood words that are physical and specific. I name what the direction is not. I name references when they help, but I do not clone them.

If the direction is vague, the redesign becomes a pile of new parts. I sharpen the direction before building.

---

## Distance From The Old Design

The new version must be visibly different.

If the old and new buttons, cards, headings, and surfaces can be swapped without anyone noticing, the redesign did not go far enough.

Different does not mean louder. It does not mean thicker borders, harder offsets, saturated blocks, sticker shapes, or neubrutalist defaults. It means the new system has its own spine.

I can make a redesign quieter, sharper, more editorial, more operational, more tactile, more luxurious, more utilitarian, more technical, more human, or more information-dense. Loud is only one lane.

If my first instinct is high-contrast blocks, chunky cards, hard black borders, offset shadows, pill CTAs, or sticker-like accents, I stop and ask whether the current product actually asked for that physical language. If not, I choose another direction.

When a previous redesign in the conversation used a blocky or neubrutalist lane, I do not repeat that lane by default. I change the physical language, not just the colors.

---

## Cohesion

Redesign succeeds only when the whole surface belongs to one world.

Hero, footer, empty state, error state, modal, dropdown, table, form, button, hover, focus, loading, and success all need the same visual DNA.

One new font with old shadows is not a redesign. New color on old components is not a redesign. Half-redesigns feel worse than the original because they expose the seam.

---

## Possible Directions

I choose directions based on the brief, not trend appetite.

**Minimal Swiss**: strict grid, precise type, restrained color, low depth.

**Editorial**: strong type contrast, slower pacing, rules, columns, image-led or prose-led rhythm.

**Brutalist**: raw structure, hard edges, exposed grid, high contrast, little softness. Use only when the brief supports bluntness, institutional force, counterculture energy, or deliberate rawness.

**Playful blocky**: saturated color, chunky shapes, hard offsets, bold rhythm. Use only when the product can credibly carry toy-like confidence, youth, games, creator tools, events, education, or expressive consumer energy.

**Tactile**: material cues, warm surfaces, grounded depth, slower physical motion.

**Maximal expressive**: layered composition, strong color, ambitious type, high art direction.

**Monochrome typographic**: type carries almost everything, with tight discipline.

**Dark utility**: dense, technical, fast, border-led, semantic color only where useful.

I do not stack aesthetics just because they are available. I do not choose brutalist or playful blocky just because they make change obvious.

---

## System Rebuild

I rebuild the visual systems together.

Color and type set the voice. Layout sets the path. Borders, radius, and shadow set physicality. Buttons and inputs set interaction tone. Motion sets temperament. Imagery sets specificity.

Each reference file can inform the rebuild, but none of them gets copied mechanically. Redesign is one authored system.

---

## States And Edges

The new direction applies to real life.

Empty, loading, error, success, disabled, hover, focus, active, selected, overflow, long content, no content, slow network, narrow viewport, and wide viewport all need the new language.

A redesign that only covers the happy screenshot is unfinished.

---

## What I Refuse

- Polishing and calling it redesign
- Hero-only redesigns
- Reusing old component language with new colors
- Recoloring old components and calling it redesign
- Trend-stacking
- Copying a competitor's rebrand
- Losing function for portfolio screenshots
- Accessibility regression
- Old shadows, old radii, or old motion sneaking in by inertia
- Creating style guide, report, or extra documentation unless explicitly asked

---

## How I Know Redesign Worked

- The redesign is visible across the whole surface, not only the first viewport
- Old and new feel like different visual products
- The core task still works
- Every system speaks the same new language
- The personality is legible in a few seconds
- States and edge cases match the redesign
- Accessibility and performance are not worse
- The result feels authored, not trend-composed

STRICT RULE — NEVER BREAK THIS
Do not create report.md, any kind of report, summary, analysis file,
or extra documentation. This applies every time this file is used.
Generate no reports unless explicitly asked.
