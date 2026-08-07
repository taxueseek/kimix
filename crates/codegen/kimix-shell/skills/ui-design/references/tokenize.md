# Design System: Tokenize `/ui-design tokenize`

Tokenize turns repeated design decisions into shared language. I extract what has proven itself. I do not invent a system because one component happened to look reusable.

Consolidation, not invention.

---

## Composition Tokens

I tokenize composition only after I understand the work pattern it serves.

Monitor tokens include status strips, alert rows, metric clusters, feed items, and drill-down panels.

Operate tokens include command bars, tool groups, canvases, inspectors, selection states, and feedback surfaces.

Compare tokens include tables, matrices, pinned headers, row actions, bulk controls, and ranking patterns.

Configure tokens include field groups, dependency notes, previews, summaries, and commit bars.

Learn tokens include article sections, progress markers, examples, walkthrough cards, and completion states.

Decide tokens include proof blocks, risk notes, trust markers, and primary action zones.

Explore tokens include filters, chips, result rows, maps, galleries, and detail drawers.

I do not extract a repeated centered-card composition unless it is genuinely the product's language.

---

## Extraction Bar

`/ui-design tokenize` changes the implementation. It is not a naming wishlist.

At minimum, I identify repeated decisions, create or update the reusable tokens/components that belong in the project, migrate at least one real usage when safe, and verify the old behavior still works.

If I only list suggested tokens or component names, tokenizing failed unless the user explicitly asked for planning only.

---

## When I Tokenize

I need repetition with the same intent.

Three repeated components with the same job. Several hard-coded values that mean the same thing. Multiple versions of one control drifting apart. A repeated composition pattern. A repeated behavior, animation, typography style, or state treatment.

Similarity is not enough. Two things that look alike but serve different purposes may need to stay separate.

---

## What I Look For

**Components**: buttons, cards, inputs, modals, rows, badges, tabs, dropdowns, empty states, form fields.

**Tokens**: color roles, spacing roles, radii, shadows, type styles, motion timing, z layers.

**Compositions**: toolbar groups, form rows, page shells, list-detail layouts, table headers, onboarding empties.

**Behavior**: loading patterns, disclosure, keyboard handling, validation, drag, selection, overlay logic.

The best candidates reduce future decisions without hiding important variation.

---

## Naming

I name by meaning, not value.

Primary action, danger text, panel border, section gap, input radius, tooltip layer. These names survive value changes. Names like blue-500 or spacing-16 only help when they are primitives in a larger system.

Semantic names belong where components consume them. Primitive names belong deeper in the foundation.

---

## Component Extraction

I extract the smallest useful API.

The component should match existing project conventions, support the states already needed, keep accessibility built in, and avoid prop sprawl. Variants come from real use, not imagination.

I migrate carefully. Old behavior must survive. Visual regressions are not acceptable payment for abstraction.

---

## Token Extraction

Values become tokens when they represent a reusable decision.

One-off values can stay local. Repeated values with the same meaning become tokens. Repeated values with different meanings should not share a token just because the number matches.

The goal is fewer arbitrary choices in future work.

---

## What I Refuse

- Extracting before a pattern is proven
- Listing token ideas without applying them
- Creating unused tokens or components
- Creating a generic component so flexible it means nothing
- Tokenizing every number
- Replacing clear local code with opaque abstraction
- Breaking existing APIs during migration
- Ignoring accessibility in extracted controls
- Creating documentation files as part of tokenizing
- Markdown reports

---

## How I Know Tokenize Worked

- Tokens or components are actually used by real UI
- Unused abstractions were not introduced
- Repetition is reduced without losing intent
- Names explain roles
- Components match project conventions
- States and accessibility are preserved
- Old duplicate code is removed only after migration is complete
- Future screens have fewer arbitrary design decisions to make

STRICT RULE — NEVER BREAK THIS
Do not create report.md, any kind of report, summary, analysis file,
or extra documentation. This applies every time this file is used.
Generate no reports unless explicitly asked.
