# Surface `/ui-design surface`

Product design serves the work. The user is not here to admire the interface. They are here to finish a task, trust the system, and come back tomorrow without relearning it.

The best product UI becomes familiar faster than it becomes impressive.

---

## Discipline files

Surface drives the product hardening pass — consult these for correct execution when each dimension comes up, not as separate passes to layer on:

- [button.md](button.md) — for correct button states and control hierarchy when hardening interactive elements
- [interaction.md](interaction.md) — for correct state coverage, keyboard path, and touch targets when auditing component behavior
- [border.md](border.md) — for correct edge language and surface separation when tightening component boundaries
- [shadow.md](shadow.md) — for correct elevation decisions when adjusting depth and layering
- [writing.md](writing.md) — for correct label, error, and empty state copy when hardening operator-facing text

---

## Composition Starts With The Operator's Job

I name the operator's task before choosing structure.

Monitor: expose status, alerts, trends, and freshness.

Operate: keep commands, target objects, and feedback in one working field.

Compare: preserve columns, labels, filters, and scan paths.

Configure: group settings by consequence and show what will change.

Learn: onboard through progressive disclosure without hiding core controls.

Decide: surface proof, risk, tradeoffs, and the next safe action.

Explore: support search, filter, sort, and reversible browsing.

Product composition is not decoration. It is workflow made visible.

---

## Hardening Bar

`/ui-design surface` hardens the working surface. It is not a visual polish pass.

At minimum, I cover the primary task, real data density, focus path, loading, empty, error, success, disabled, overflow, mobile structure, keyboard use, and copy for recovery where those states apply.

If I only changed color, spacing, or type while product states remain missing, the product pass failed.

---

## The Register

I use this file for app UIs, dashboards, admin panels, settings, authenticated routes, tools, tables, forms, editors, and operational screens.

In product, familiarity can be a feature. Surprise has to earn its place.

---

## What Good Feels Like

An experienced operator should open the screen for the eleventh time that day and move without hesitation.

Controls stay where they are expected. Labels use the same nouns. Rows keep stable density. Primary actions are obvious. Secondary actions are reachable but not loud.

If the user pauses because the UI is clever, I made the wrong trade.

---

## Type

I usually pick one strong sans or the system stack. Product labels, tables, settings, and forms do not need display type.

Scale stays compressed. Weight, color, and spacing carry hierarchy with size. Dense surfaces can use smaller type when the audience is skilled and the row structure is clear.

Numbers align. Tables deserve tabular figures. Metadata deserves restraint.

---

## Color

Product color has three jobs.

**Chrome** gives the app stable surfaces: page, sidebar, panel, row, input, divider.

**State** teaches the user the system vocabulary: selected, dirty, focused, loading, success, warning, error, disabled.

**Primary action** gets the strongest accent. If everything is accented, no action is primary.

Product starts restrained. A welcome flow or onboarding moment can carry more color. The working surface gets calmer after login.

---

## Components

Every interactive component needs its real life, not just its resting state.

Default, hover, focus-visible, active, disabled, loading, selected, error, success, empty, overflow. Some components use all of these. Some use fewer. None get to ignore the states they can enter.

Same control, same vocabulary. If two save buttons look unrelated, one is wrong.

---

## Density

Product surfaces breathe less than marketing. Density is not clutter when the relationships are clear.

Rows, panels, filters, toolbars, and tables should fit the work. Power users often need more on screen, not more air. Comfortable and compact modes can be a real feature.

---

## Motion

Product motion is short and functional.

Hover, press, selection, drawer open, popover close, loading, save confirmation. That is enough. No page-load ceremony. No scrolling performance. No animation that delays the task.

---

## What Product Can Own

- System fonts
- Standard navigation
- Tables with many rows
- Predictable sidebars and tabs
- Keyboard shortcuts
- Monochrome charts with one emphasized series
- Dense but organized screens
- Repetition, when repetition teaches the system

---

## What I Refuse

- Display fonts in labels, tables, or form controls
- Calling visual polish product hardening
- Leaving real data, error, empty, loading, or overflow states untested
- Marketing hero motion inside a working app
- Custom controls that lose native keyboard behavior
- Modals for long forms that need a route
- Five gray scales across one product
- Accent color on cancel, secondary, decorative, and inactive elements
- Custom scrollbars that make scrolling harder
- Cleverness where predictability would build trust

---

## How I Know Product UI Works

- The surface survives real data and real states
- The primary task is clear without reading instructions
- Focus, loading, error, empty, and success states exist
- A keyboard user can complete the core flow
- Tables and dense data stay aligned and scannable
- Mobile layout changes structure rather than hiding work
- The same pattern means the same thing across screens
- The interface feels calmer the more often it is used

STRICT RULE — NEVER BREAK THIS
Do not create report.md, any kind of report, summary, analysis file,
or extra documentation. This applies every time this file is used.
Generate no reports unless explicitly asked.
