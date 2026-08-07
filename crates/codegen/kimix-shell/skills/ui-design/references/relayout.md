# Relayout: `/ui-design relayout`

Relayout moves the pieces without changing the identity. The content is mostly right. The arrangement is wrong.

I reposition images, text, controls, and sections so the page has a clearer focal point, rhythm, and reading path.

Relayout is not a nudge. If the result looks like the same screenshot with slightly different spacing, I failed.

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

## Composition Diagnosis

I first ask which work pattern the current layout is trying to serve.

If the surface monitors, I expose change, urgency, recency, and status.

If it operates, I pull tools, targets, and feedback into one working field.

If it compares, I protect alignment and make scanning cheaper.

If it configures, I group choices by consequence and keep commit visible.

If it teaches, I create a reading path with progressive reveal.

If it persuades, I reduce the field to claim, proof, risk, and action.

If it explores, I make filters, search, and backtracking obvious.

The new composition must answer that pattern. Rearranged sameness is not relayout.

---

## What I Will Change

I move things on the canvas.

- Image position and scale
- Text-image relationships
- Section order
- Visual weight
- Grouping and spacing
- Alignment system
- Grid structure
- Reading path
- Responsive composition

I do not change color strategy, font voice, visual identity, or feature scope unless the layout problem exposes a conflict that needs another mode.

---

## Structural Change Bar

A relayout must include at least one visible structural change:

- A new dominant focal point
- A changed hero or section composition
- A reordered section sequence
- A different relationship between text and proof object
- A changed navigation, sidebar, toolbar, or action placement
- A transformed grid, table, split view, stack, or flow
- A responsive order that changes the user's path

Spacing, padding, alignment, width, gap, and margin changes can support relayout. They cannot be the whole relayout.

If I cannot justify a structural change, the command should be `finish`, not `relayout`.

---

## Required companion references

The skill automatically reads these three references in order. No user selection or permission needed—the system loads them silently to contextualize and inform better relayout decisions.

| **[layout.md](layout.md)** | Establish the new spatial system, grid, and composition |
| **[color.md](color.md)** | Build the new palette, neutrals, semantic roles, and accent strategy |
| **[typeset.md](typeset.md)** | Pick new fonts, define new scale, set new pairing and hierarchy |

---

## The Core Judgment

The eye needs somewhere to land.

I decide the focal point before I move anything. Then I make secondary material support it and tertiary material stay out of the way.

If everything is equally important, the layout has no opinion.

---

## Image And Text

Images carry different jobs.

A hero image can be the message. A feature image can explain a claim. A background image can set atmosphere. An inline image can support reading. A decorative image can be removed.

Text can sit beside an image, over it, under it, wrapped around it, or alternate across sections. I choose the relationship based on the content, not the template.

When text and image fight, I decide which one leads.

---

## Composition Lanes

I pick one lane per section.

**Symmetric** for calm authority and singular focus.

**Asymmetric** for energy, tension, and art direction.

**Strict grid** for rigor, technical confidence, and dense systems.

**Editorial flow** for long-form, story, and cultural pacing.

**Modular grid** for comparison, galleries, dashboards, and catalogs.

**Broken grid** when one or two items deserve clear dominance.

Mixing lanes inside one section usually creates noise. Sequencing different lanes across sections can create rhythm.

---

## Section Order

I order sections by user need, not by source order.

The strongest hook comes early. Context comes before complex proof. Trust appears before the ask when the ask needs confidence. CTAs appear at decision points. The ending answers what now.

Mobile order is part of the design. A desktop composition that collapses into nonsense has not been relaid out.

---

## Spatial Rhythm

I use spacing to show relationship.

Tight gaps mean the pieces belong together. Wide gaps mean a new thought. Repeated gaps create rhythm. One gap everywhere creates monotony.

kimix's 1-4-9 rhythm stays useful here: micro, component, and section breaths.

---

## Balance

I balance visual weight, not just geometry.

Large images, dark blocks, saturated color, bold headlines, dense text, and isolated controls all carry mass. White space carries counterforce. The visual center often sits slightly above the mathematical center.

Diagonal balance can create energy. Random imbalance just feels off.

---

## What I Refuse

- Duplicated "relayout process" instructions
- Treating relayout as redesign
- Calling spacing polish a relayout
- Leaving the same hero/proof composition intact when that composition is the problem
- Moving sections without deciding priority
- Alternating image/text blocks by reflex
- Full-bleed imagery when the image is weak
- Centering everything because it feels safe
- Adding cards because spacing feels unresolved
- Markdown reports or rationale files

---

## How I Know Relayout Worked

- The before and after read differently as thumbnails
- At least one structural change is visible without reading the code
- One focal point leads each section
- The eye path matches the content priority
- Images and text support rather than compete
- Section sequence feels like a journey
- Spacing shows relationships
- Desktop and mobile tell the same story
- The composition still reads as a thumbnail

STRICT RULE — NEVER BREAK THIS
Do not create report.md, any kind of report, summary, analysis file,
or extra documentation. This applies every time this file is used.
Generate no reports unless explicitly asked.
