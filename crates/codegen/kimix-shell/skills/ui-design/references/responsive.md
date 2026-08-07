# Responsive Design `/ui-design responsive`

Responsive design is not making a desktop layout smaller. It is preserving the story, task, and controls across different stages, hands, viewports, and environments.

Screen size is only one variable. Input mode often matters more.

---

## Discipline files

Responsive drives the recomposition pass — consult these when each dimension comes up during responsive work, not as separate passes to layer on:

- [layout.md](layout.md) — for correct grid, spacing rhythm, and composition logic when adapting breakpoints
- [interaction.md](interaction.md) — for correct touch target sizing and input mode behavior when adapting controls
- [button.md](button.md) — for correct full-width and stacked button patterns when adapting narrow viewports

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

## Composition Changes Across Context

The work pattern stays. The composition can change.

Monitor screens may collapse from a wall of status into priority strips, alerts, and drill-down panels.

Operate screens may move primary commands near the thumb and hide secondary tools behind stable drawers.

Compare screens may switch from wide tables to pinned labels, cards with aligned fields, or focused comparison pairs.

Configure screens may become grouped sections with sticky save and clear dependency feedback.

Learn screens may lengthen into a readable article path with progress cues.

Decide screens may reduce to claim, proof, action, and escape.

Explore screens may shift filters into drawers, maps into lists, and galleries into search-led flows.

Responsive work preserves the job, not the desktop shape.

---

## Adaptation Bar

`/ui-design responsive` recomposes the interface across contexts. It is not a max-width tweak.

At minimum, I verify narrow phone, ordinary phone, tablet, small laptop, desktop, and wide desktop behavior when the app can be rendered.

I adapt layout, order, density, navigation, actions, tables, media, forms, type, touch targets, hover dependencies, focus path, safe areas, and environmental preferences where they apply.

If the same desktop composition merely shrinks, the responsive pass failed.

---

## My Starting Bias

I start from the smallest reasonable canvas and add complexity as space earns it. I do not worship the phrase mobile-first. I care that the base experience is sturdy and that wider contexts get more structure rather than stretched leftovers.

Breakpoints come from content pressure. When the content breaks, the layout changes.

---

## Viewports I Respect

I test narrow phones, ordinary phones, tablets, small laptops, standard desktop, and ultrawide screens.

At small widths, I look for overflow, crushed labels, unreachable actions, tiny targets, and broken reading order.

At wide widths, I look for runaway line length, lonely content, stretched controls, and sections that lose composition.

---

## Input Modes

I do not equate phone with touch or desktop with mouse.

Touch needs larger targets, no hover-only functionality, visible gesture alternatives, and controls placed where hands can reach.

Mouse and trackpad can use hover, precision controls, drag, resize, and denser affordances.

Keyboard needs logical focus, visible focus, reachable controls, and shortcuts when the product is dense.

Stylus and hybrid devices need both touch generosity and pointer precision.

---

## iOS Safari Input Zoom

On iOS Safari, focusing any `input`, `select`, or `textarea` with a font-size below 16px triggers an automatic viewport zoom. The page widens, horizontal scroll appears, and the user must pinch back to normal. This is a silent layout break.

I check all form elements at the narrow breakpoint. If font-size is below `1rem` (16px) on screens under 640px, I flag it and correct it.

**Specificity warning:** A bare type-selector rule like `input { font-size: 1rem }` loses to any utility class (e.g. Tailwind's `text-sm`). A class selector has higher specificity (0,1,0) than a type selector (0,0,1), so the computed font size remains 14px and iOS Safari still auto-zooms. A rule that does not win the cascade is not a fix.

**Fix — replace the utility with a responsive variant:**

In Tailwind, swap `text-sm` for `text-base sm:text-sm` on every affected form element. The mobile breakpoint stays at 16px; desktop keeps 14px. No global CSS needed, no specificity fight.

```html
<!-- before -->
<input class="text-sm" />

<!-- after -->
<input class="text-base sm:text-sm" />
```

In vanilla CSS projects with no utility classes, a plain media query works because nothing is fighting the type selector:

```css
@media (max-width: 639px) {
  input, select, textarea {
    font-size: 1rem;
  }
}
```

For third-party or generated components where the class cannot be changed, flag it as a component-level issue to address at the source. Do not reach for `!important` — it breaks the cascade and creates a specificity arms race.

Never use `maximum-scale=1` in the viewport meta tag — it disables pinch-to-zoom for all users, which is an accessibility failure.

---

## Thumb Reach

On phones, the bottom of the screen is easier to reach than the top. Primary actions belong where hands naturally operate. Destructive or rare actions can sit farther away when intention matters.

This is not a law for every app shell, but it is a pressure I account for.

---

## Component Adaptation

Components should adapt to their container when possible.

A card in a sidebar, a card in a main column, and a card in a wide split view should not all use the same composition. Page breakpoints cannot solve every component context cleanly.

I prefer components that respond to available space and keep their meaning.

---

## Environmental Preferences

Responsive includes the user's environment.

Dark mode is a real theme. Reduced motion gets an authored alternative. High contrast gets stronger boundaries and no reliance on transparency. Inverted colors and zoom must not destroy meaning.

Safe areas matter. Fixed controls cannot sit under notches, rounded corners, browser chrome, or home indicators.

---

## Text Direction

A product that ships Arabic, Hebrew, Farsi, or Urdu is not "the same layout, flipped by the browser." Direction is a layout input, the same as viewport or input mode.

I set `dir="rtl"` (or `dir="auto"` per field) at the right scope and build with logical CSS, not physical sides: `margin-inline-start/end`, `padding-inline-start/end`, `border-inline-start/end`, `inset-inline-start/end`, `text-align: start/end`. In Tailwind, that is `ms-/me-`, `ps-/pe-`, `border-s/border-e`, `start-/end-`, `text-start/text-end` instead of `ml-/mr-`, `pl-/pr-`, `border-l/border-r`, `left-/right-`, `text-left/text-right`. Flex and grid order, `justify-start/end`, and `divide-x` (paired with `rtl:divide-x-reverse`) follow direction automatically once the ancestor has `dir` set — I don't hand-mirror markup for these.

Not everything flips. Icons that encode reading direction — back/forward chevrons, reply/forward arrows, progress and breadcrumb arrows, drag handles aligned to text flow — mirror in RTL. Icons that encode a universal meaning — play, pause, checkmarks, the brand mark, a clock face, a video timeline scrubber, charts and graphs — stay fixed. Numerals, phone numbers, and embedded LTR content (code, URLs, Latin brand names) stay LTR even inside an RTL paragraph; that's what `dir="auto"` or a `<bdi>` wrapper is for, not a layout hack.

I test the actual rendered RTL surface, not just the LTR one with a mental flip. Mixed-direction text (an English product name inside an Arabic sentence), numerals in dates and prices, and long Arabic/Hebrew words at narrow widths are where this breaks first.

---

## Responsive Type

Product UI keeps predictable type scales. Brand display can be fluid. Body text remains readable, not theatrical.

I do not let a heading scale so aggressively that it breaks zoom, wraps into nonsense, or overwhelms smaller screens.

---

## Tables

Tables need a chosen strategy.

Data-heavy tables usually keep horizontal scroll because preserving columns matters. Small comparison tables can become cards. Mixed-importance tables can hide lower-priority columns. Very small screens may need stacked cells.

I do not mix strategies randomly on the same table.

---

## What I Refuse

- Core features hidden on mobile
- Calling a width tweak a responsive pass
- Desktop composition simply squeezed smaller
- Hover-only controls
- Touch targets sized for cursors
- Separate mobile and desktop code paths with different IA
- Fixed widths that cause horizontal scroll
- Ultrawide pages with unreadable line length
- Safe-area ignorance on fixed bars
- Device detection when feature detection is the real need
- Fluid type in dense product UI because it seems modern
- Hardcoded `left`/`right`, `ml-`/`mr-`, `pl-`/`pr-` instead of logical start/end on a product that supports RTL languages
- Mirroring an icon that carries universal meaning (play, checkmark, brand mark) just because the page is RTL
- Input, select, or textarea elements with font-size below 16px on narrow screens — iOS Safari zooms the viewport on focus, breaking layout
- Using `maximum-scale=1` in the viewport meta tag to suppress iOS zoom — it disables pinch-to-zoom for all users

---

## How I Know Responsive Works

- The rendered UI was checked at multiple viewport classes
- Composition adapts where the job or content pressure changes
- The same story survives across viewports
- Controls fit the input mode
- Nothing overflows at narrow widths
- Wide layouts stay composed
- Keyboard and touch both complete the core task
- Safe areas protect fixed controls
- Dark, reduced-motion, high-contrast, and zoom states remain usable
- Tables keep their data meaning
- RTL locales render mirrored where direction carries meaning and fixed where it doesn't, with logical CSS doing the work
- All input, select, and textarea elements use at least 16px font-size on narrow screens — no iOS Safari viewport zoom on focus

STRICT RULE — NEVER BREAK THIS
Do not create report.md, any kind of report, summary, analysis file,
or extra documentation. This applies every time this file is used.
Generate no reports unless explicitly asked.
