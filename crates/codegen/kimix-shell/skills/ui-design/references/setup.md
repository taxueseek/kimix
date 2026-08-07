# Setup

Setup gives the project one design memory. I use it when the repo needs a `.kimix/ui-design/brief.md` that future design work can trust.

This is not a report. It is not a research artifact. It is the working design constitution for the repo.

---

## Composition Defaults I Capture

When I create the design constitution, I record the project's likely work patterns.

Brand pages may decide, teach, compare, or explore.

Product surfaces may monitor, operate, compare, configure, learn, decide, or explore.

The project can have more than one pattern, but each screen needs a dominant one.

I document the allowed composition lanes so future design work does not collapse into the same centered hero, card grid, and pill controls.

---

## Applied Setup Bar

`/ui-design setup` creates or updates the actual `.kimix/ui-design/brief.md`. It is not a conversational setup checklist.

At minimum, I read the available project files, extract durable design facts, write `.kimix/ui-design/brief.md`, and make future design commands more specific than they were before.

If I only ask questions or describe what `.kimix/ui-design/brief.md` should contain, setup failed.

---

## What I Create

I create or update one file at `.kimix/ui-design/brief.md`.

That file carries the durable answers:

- Register: brand or product
- Users and context
- Product purpose
- Voice
- Anti-references
- Design principles
- Accessibility expectations
- Visual foundation
- Component rules

I do not create separate product and design documents. One file, one source of truth.

---

## What I Read Before Asking

I read the repo first.

README, package metadata, routes, existing styles, tokens, assets, logo, favicon, CSS variables, theme files, previous design notes, and old product or style documents. If the answer exists in the repo, I use it.

I form a register hypothesis from the code. Marketing routes, big heroes, pricing, blog, docs, and portfolio shapes point brand. App routes, dashboards, settings, forms, tables, and authenticated shells point product.

I ask only for what I cannot infer.

---

## What I Ask

I keep setup short and strategic.

- Is the register hypothesis right?
- Who is the primary user and what state are they in?
- What is the single most important job?
- What should the voice feel like in concrete physical words?
- What should this not look or feel like?
- Are there special accessibility or motion needs?

I do not ask for fonts, colors, radii, or minor styling preferences before I understand the purpose.

---

## How I Write `brief.md`

I make it concise enough to stay useful.

It should tell future design work what kind of surface this is, who it serves, what it must become, what it must avoid, and what visual system exists or should be respected.

If an old `.kimix/ui-design/brief.md` exists, I show the intended change before overwriting. I never silently replace the project's design memory.

If older context files exist at the project root (any `.md` file that reads like a product brief, style guide, or brand document), I merge useful content into `.kimix/ui-design/brief.md` and ask before deleting anything.

---

## What I Refuse

- Creating surface.md as a separate file
- Talking about setup without creating or updating `brief.md`
- Writing generic principles not grounded in the repo
- Splitting context into multiple files instead of one `brief.md`
- Overwriting brief.md without confirmation
- Asking questions the repo already answers
- Writing a long strategy document nobody will read
- Treating setup as a design review
- Markdown reports

---

## How I Know Setup Is Done

- `brief.md` exists at `.kimix/ui-design/brief.md`
- `brief.md` includes facts found in the repo, not just generic design advice
- Register is explicit
- Users, purpose, voice, and anti-references are clear
- Principles guide decisions rather than restating taste
- Visual foundation reflects the actual repo when possible
- Future design commands can proceed without re-asking basics

STRICT RULE — NEVER BREAK THIS
Do not create report.md, any kind of report, summary, analysis file,
or extra documentation. This applies every time this file is used.
Generate no reports unless explicitly asked.
