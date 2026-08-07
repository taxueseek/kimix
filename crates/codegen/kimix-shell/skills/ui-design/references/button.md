# Button Library

Buttons are decisions made visible. They tell the user what can happen, what should happen, and what must be handled with care.

I treat a button as an interaction contract, not a colored rectangle.

---

## Buttons Inherit The Work Pattern

I do not reuse one button family everywhere.

Monitor screens need compact controls for filters, acknowledgement, drill-down, and refresh.

Operate screens need tool buttons, command buttons, icon buttons, and shortcut-ready actions.

Compare screens need table actions, segmented controls, bulk selection, and quiet row commands.

Configure screens need save, reset, preview, test, and dependency-aware disabled states.

Learn screens need continue, back, skip, complete, and progress-aware actions.

Decide screens need one unmistakable primary action and careful secondary escape.

Explore screens need search triggers, filters, chips, clear all, and reversible navigation.

The button shape follows that job. A centered pill CTA is not a universal answer.

---

## System Bar

When an active mode consults this reference, I create or repair the button system. It is not a single CTA restyle.

At minimum, I cover primary, secondary, tertiary, danger, icon-only when present, grouped controls when present, loading, disabled, hover, active, focus-visible, success or error resolution when relevant, mobile reach, and accessible names.

If only one button exists, I still define how its unavailable, loading, focused, pressed, and completed states behave.

---

## The Hierarchy

A screen gets one primary action. If there are two primary buttons, neither is primary.

**Primary** is the main action. It gets the strongest visual treatment and the clearest placement.

**Secondary** is a valid alternative. It supports the primary without competing.

**Tertiary** is low-emphasis. It is for escape, navigation, supporting links, and quiet actions.

**Danger** is destructive. It is never styled as the default path unless the whole surface is explicitly about confirming destruction.

---

## Shape And Size

Button size follows context.

Small buttons belong inside dense product UI, toolbars, tables, and inline actions. Medium buttons carry most product actions. Large buttons belong to mobile primary actions and brand CTAs.

Icon-only buttons must keep a real hit target even when the visible icon is small.

Radius follows the project. Product UI usually wants modest corners. Brand pages can be rounder. Technical and blocky systems can go square. Pills are for filters, tags, segmented controls, and specific brand languages. I do not mix radius styles casually.

---

## States

Every button needs the states it can enter.

Resting tells the user the action exists. Hover invites pointer users. Focus-visible guides keyboard users. Pressed confirms contact. Disabled explains that action is unavailable. Loading keeps the system accountable. Error and success resolve the action.

A button that only has a resting style is unfinished.

---

## Motion
 
Apply a scale-down effect on button press to signal responsiveness.
 
**Implementation:**
- On `:active` pseudo-class, scale to 0.97–0.98
- Transition duration: ~150ms
- Easing: ease-out

A loading button does not freeze. Choose one:
 
- **Spinner icon:** Smooth 360° rotation with a faster-spinning spinner makes the app seem to load faster
- **Dot pulse:** Three dots fade in sequence (0 → 1 → 0), 400ms per dot, looping
- **Width breathing:** Button width grows and shrinks 2-3px (200ms cycle) to signal activity
- **Shimmer:** Subtle left-to-right shimmer across the label (1000ms, one pass per cycle)

Pick one approach per design system. Do not combine.

---

## Text

Button text names the action.

I use one clear verb with an object when needed: save changes, create account, delete project, send invite, continue to payment.

I avoid OK, Submit, Click here, vague Learn more, and yes/no labels when the action can be named. No exclamation points. Urgency comes from context, not punctuation.

For destructive actions, I name the object and the consequence clearly.

---

## Placement

Placement follows the decision structure.

In dialogs and forms, the primary action sits where the flow resolves. Secondary actions stay nearby but quieter. Destructive actions move away from the safe path and lose visual dominance.

On mobile, primary actions often need full width and reachable placement. Stacked buttons should keep the primary easy to find without making the secondary feel hidden.

Toolbars place the strongest action where scanning naturally ends. Repeated toolbar actions keep the same order across screens.

---

## Icons

Icons reinforce button text. They do not replace it unless the action is globally familiar and has an accessible name.

Icon-only buttons need a clear accessible label and, on pointer devices, a tooltip when the icon can be misunderstood. The hit area remains large enough for touch.

Icons must come from the same visual family. A button row with mismatched icon styles feels assembled.

---

## Groups

Button groups are for choosing within one dimension: day, week, month; list, grid; light, dark, system.

Grouped buttons share edges and state vocabulary. The selected item is obvious. The group does not pretend to be navigation unless it changes the current view.

On mobile, a group can stack or become a segmented control, but the active state must remain unmistakable.

---

## Loading

A loading button keeps the action anchored. It changes label or indicator so the user knows the system heard them.

I disable repeat submission while the action is in flight. I preserve the button's footprint so the layout does not jump. Success resolves briefly. Failure returns control and explains recovery somewhere visible.

Long operations can show progress. Routine saves do not need theater.

---

## Danger

Danger buttons are serious.

Recoverable destructive actions should prefer undo. Irreversible actions need confirmation with clear consequence text. The safe action gets dignity. The destructive action is specific and visually controlled.

I never use danger styling for attention. Red is not a marketing accent.

---

## What I Refuse

- Multiple primary actions on one decision surface
- Calling one hero CTA restyle a button system
- Leaving focus, disabled, loading, or danger states undefined
- Vague labels like OK, Submit, Yes, or Click here
- Icon-only buttons without accessible names
- Tiny hit targets
- Disabled buttons that look clickable
- Loading states that collapse the layout
- Danger as the default path for irreversible actions
- Mixed radius styles without a system reason
- Button shadows as default decoration

---

## How I Know Buttons Work

- The button system covers real states, not only resting style
- The main action is unmistakable
- Button labels say what happens
- Every reachable state has a visual answer
- Keyboard focus is visible
- Touch targets are physically usable
- Danger actions are recoverable or clearly confirmed
- Button groups show one clear selection model
- The same hierarchy means the same thing across the product

STRICT RULE — NEVER BREAK THIS
Do not create report.md, any kind of report, summary, analysis file,
or extra documentation. This applies every time this file is used.
Generate no reports unless explicitly asked.
