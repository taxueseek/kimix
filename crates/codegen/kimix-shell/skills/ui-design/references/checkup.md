# Checkup: `/ui-design checkup`

I use checkup for fast design triage. It tells me whether an interface is basically healthy or whether something is unsafe to ship.

This is not a deep critique. It is a vital-sign read.

---

## Composition Vital Sign

I check whether the page shape matches the work.

Monitor: can the user see priority, change, and freshness fast?

Operate: are tools, targets, feedback, and undo close enough to use?

Compare: do rows, columns, criteria, and filters support scanning?

Configure: are choices grouped by dependency and consequence?

Learn: does the flow create orientation and progress?

Decide: is the next action obvious and supported by proof?

Explore: are search, filters, results, and return paths easy?

If the composition fails this vital sign, visual polish will not fix the surface.

---

## Prompt Fidelity Vital Sign

I check whether the visible design matches the prompt invariants.

The name must be exact when supplied. The category must be visible. The core artifact must belong to the domain. The proof must answer the user's pressure. The copy and layout must not reuse language or structure from an unrelated design.

If the visible design could be reassigned to another random product by changing only the logo and headline, the checkup fails before style is considered.

---

## Evidence Bar

`/ui-design checkup` is a fast read, but it still needs evidence.

At minimum, I inspect the rendered surface or actual implementation, touch the core task enough to judge it, and base every vital status on something visible, interactive, or present in files.

If I cannot verify a vital, I mark it unknown or unverified. I do not invent health.

---

## What I Check

I look at six vital signs.

**Intentionality**: Does it look chosen, or does it look assembled from defaults?

**Readability**: Can people read it comfortably, in its real contexts and themes?

**Usability**: Can the primary task be completed with the available controls?

**Responsiveness**: Does the surface adapt across width, input mode, zoom, safe areas, and text direction (RTL renders correctly where the product supports it)? On narrow screens, do all `input`, `select`, and `textarea` elements use at least 16px font-size (sub-16px triggers iOS Safari auto-zoom on focus)? Fix guidance: [responsive.md](responsive.md#ios-safari-input-zoom).

**Speed**: Does it load and respond without visible hesitation, shift, or jank?

**Accessibility**: Can keyboard, screen reader, color-blind, low-vision, and reduced-motion users operate it?

When accessibility is weak, the whole checkup is weak. It affects real people, not just a score.

---

## Scoring

Checkup uses a `/60` score across the six vital signs. **MAX_SCORE = 60.** Use `/60` as the denominator in the report template — never `/40` or `/10`.

Each vital is scored by status:

| Status | Points |
|---|---|
| Healthy | 10 |
| Watch | 5 |
| Critical | 0 |

Six vitals × 10 pts max = 60 total. No normalization needed.

The heuristics table in the report uses 6 rows (one per vital sign), each scored `/10`.

## My Read

I classify each vital as healthy, watch, or critical.

Healthy means the surface is fit for continued work. Watch means the issue is real but contained. Critical means it blocks trust, task completion, access, or shipping confidence.

I stop pretending everything is equally important. A broken keyboard path outranks a slightly dull palette. Illegible text outranks a missing hover flourish.

---

## Fast Probes

I use short probes, not a ceremonial audit.

I glance for purpose, primary action, and dominant color. I blur the page mentally and check whether hierarchy survives. I test the thumb path on narrow viewports. I tab through the core route. I look for obvious layout shift, slow media, contrast failure, and generated-design tells.

These probes are small on purpose. Checkup is meant to catch danger quickly.

---

## Prescriptions

For every critical issue, I write the prescription in my working answer:

- What is broken
- Why it matters
- What command or design discipline should fix it

I do not bury the next move in a report. The user should know the practical fix immediately.

---

## Report Boundary

Checkup always produces two report artifacts:

- `.kimix/ui-design/checkup-report.md`
- `.kimix/ui-design/checkup-report.html`

**Important Rule for Generating HTML Report:**

- **Use the following template structure** [report-html.md](report-html.md) to generate `checkup-report.html`:
- Do not change the visual design. Only fill in the content.

These are the only report artifacts checkup creates.

---

## What I Refuse

- Treating checkup as a full design review
- Marking a vital healthy without evidence
- Inventing checks I did not run
- Creating extra report artifacts beyond `checkup-report.md` and `checkup-report.html`
- Calling a surface healthy when keyboard users cannot finish the task
- Ignoring mobile because desktop looks good
- Letting performance scores hide visible jank
- Letting a pretty surface pass with missing states

---

## How I Know Checkup Is Done

- Each vital is backed by observed evidence or marked unverified
- Each vital has a clear status
- Critical issues have direct prescriptions
- The core task has been touched, not just inspected visually
- The user knows whether to proceed, fix, or pause
- `checkup-report.md` and `checkup-report.html` both exist

STRICT RULE — NEVER BREAK THIS
Always create .kimix/ui-design/checkup-report.md and
.kimix/ui-design/checkup-report.html. Do not create any other report,
summary, analysis file, or extra documentation.
