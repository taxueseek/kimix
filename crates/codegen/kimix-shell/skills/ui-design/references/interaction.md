# Interaction Design `/ui-design interaction`

Interaction is behavior under pressure. Hover, focus, press, touch, typing, waiting, failure, recovery. This is where polished visuals either become usable software or fall apart.

Motion shows behavior. This file decides what the behavior is.

---

## Discipline files

Interaction drives the behavior pass — consult these when each dimension comes up during interaction work, not as separate passes to layer on:

- [button.md](button.md) — for correct button states and the full control system when auditing interactive elements
- [motion.md](motion.md) — for correct transition timing and easing when specifying state-change animation

---

## Interaction Follows Composition

The work pattern decides where controls live and how feedback returns.

Monitor screens prioritize filters, alert acknowledgement, drill-down, and live refresh.

Operate screens prioritize command bars, shortcuts, direct manipulation, undo, and fast feedback.

Compare screens prioritize sort, filter, selection, pinned context, and preserved scroll position.

Configure screens prioritize dependencies, validation, preview, save state, and reversible commits.

Learn screens prioritize progress, reveal, completion, and graceful exit.

Decide screens prioritize one primary path, visible reassurance, and clear escape.

Explore screens prioritize search, filter, browse, reset, and backtracking.

Controls do not float around the page because they look balanced. They sit where the work needs them.

---

## Behavior Bar

`/ui-design interaction` adds missing behavior. It is not hover polish.

At minimum, I inspect and repair hover, active, focus-visible, keyboard path, touch targets, disabled, loading, empty, error, success, overflow, destructive recovery, and overlay behavior where those states apply.

If an interaction cannot be triggered in the current UI, I either wire a visible trigger or say the state is implemented but not currently reachable.

---

## Every Control Has A Life

I design controls beyond the resting state.

- Idle tells the user what exists
- Hover invites pointer users
- Focus guides keyboard users
- Active confirms touch or press
- Disabled explains unavailable action
- Loading keeps the system accountable
- Selected marks current context
- Error explains a problem
- Success confirms completion
- Empty teaches what belongs here
- Overflow keeps real content from breaking the surface

If a component can enter a state, I account for it.

---

## Focus Is Architecture

Focus rings are not decoration. They are the map for keyboard users.

I make focus visible, consistent, and confident. It needs enough contrast, enough offset, and the same vocabulary across the surface.

I never remove focus without a replacement. I never make focus so subtle that only the designer can find it.

---

## Touch Is Physical

The visual icon can be small. The hit area cannot.

On touch devices, controls need enough room for fingers, not cursors. Adjacent links need breathing room. Swipe gestures need visible alternatives. Pinch-to-zoom stays available for readable text.

If a feature only exists on hover, it does not exist for touch users.

---

## Direction-Aware Controls

On a product that supports RTL languages, controls that encode reading direction must mirror: back/forward navigation, chevrons, breadcrumbs, carousels, swipe-to-dismiss, and drag handles aligned to text flow. Controls with universal meaning do not: play/pause, checkmarks, the brand mark, media scrubbers.

I check this with the actual `dir="rtl"` rendering, not by eyeballing the LTR version. Logical sizing and spacing belong in layout; see [responsive.md](responsive.md#text-direction).

---

## Keyboard Path

A complete keyboard path is a product feature.

Tab order follows the visual and conceptual flow. Enter and Space activate. Arrow keys move within grouped widgets. Escape closes temporary surfaces. Focus returns to the trigger after overlays close.

I test the primary task without a mouse. If I get stuck, the design is broken.

---

## Forms

Forms are where users quit, so I make them calm.

Labels stay visible. Placeholders show examples, not identity. Required fields are clear. Errors appear near the field and preserve input. Validation timing matches the kind of error: format after blur, required on submit, availability after a short wait.

Every error answers what happened and how to recover. Blame is forbidden.

On mobile, I verify every `input`, `select`, and `textarea` uses a font-size of at least 16px. A sub-16px form element triggers iOS Safari auto-zoom on focus, shifting layout and forcing the user to pinch back — it breaks the interaction, not just the visuals. Fix guidance and the specificity gotcha: [responsive.md](responsive.md#ios-safari-input-zoom).

---

## Overlays

Modals are for decisions that need interruption. They are not default containers.

Long forms get pages. Information gets inline disclosure. Success gets a toast or inline confirmation. Destructive, legal, financial, or irreversible moments can earn a dialog.

Temporary surfaces escape clipping, stack correctly, close predictably, and keep focus sane.

---

## Undo Beats Confirm

When an action is recoverable, I prefer undo. Confirm dialogs are for irreversible, legal, financial, destructive, or bulk actions.

A good destructive flow names the object, states the consequence, and gives the safe escape equal dignity.

---

## Loading And Failure

The interface must respond quickly, even when the system cannot finish quickly.

Blank waiting is not acceptable. A loading state appears soon. Long waits get progress or expectation. Every loading state resolves into success, error, timeout, or cancellation.

Failure keeps the user's work. Recovery is visible. Retry is available when retry makes sense.

---

## What I Refuse

- Hover with no focus equivalent
- Calling hover tweaks an interaction pass
- Unreachable states described as finished
- Placeholder-only labels
- Disabled controls with no explanation when context matters
- Spinners replacing whole layouts without shape
- Dropdowns clipped by parent containers
- Menus that cannot be used with keyboard
- Modals that trap nothing
- Errors that say only "Error" or "Invalid"
- Gesture-only actions with no visible fallback

---

## How I Know Interaction Works

- Every claimed interaction can be triggered or is honestly marked unreachable
- The core task works by mouse, touch, and keyboard
- Focus is always visible and logical
- Loading, empty, error, success, and overflow states are handled
- Destructive paths are reversible or clearly confirmed
- Overlays close and return focus correctly
- Form errors help without punishing
- The interface feels responsive even during waits
- Form inputs on mobile use at least 16px font-size — no iOS Safari auto-zoom on focus

STRICT RULE — NEVER BREAK THIS
Do not create report.md, any kind of report, summary, analysis file,
or extra documentation. This applies every time this file is used.
Generate no reports unless explicitly asked.
