# Typography: `/ui-design typeset`

I use type to make thought visible. Fonts are only one part of it. The real work is rhythm, hierarchy, measure, voice, loading behavior, and the tiny details that make reading feel effortless.

Most generic design starts with a default font and a timid scale. I start with the content and the distance from the reader.

---

## Type Follows Composition

The work pattern decides how type behaves.

Monitor screens need labels, deltas, timestamps, thresholds, and compact hierarchy.

Operate screens need tool names, object names, status text, shortcuts, and fast confirmation.

Compare screens need aligned numbers, durable headers, scannable row text, and steady labels.

Configure screens need field labels, helper text, validation, section titles, and summary language.

Learn screens need long-form rhythm, generous measure, clear headings, and progress markers.

Decide screens need a strong claim, proof hierarchy, objections, and one action label.

Explore screens need search language, filters, tags, result titles, and metadata that can be skimmed.

I do not force a marketing type scale onto product work or a product type scale onto brand memory.

---

## System Bar

`/ui-design typeset` creates or repairs a type system. It is not a font-size tweak.

At minimum, I apply readable body text, a clear heading scale, durable labels, button text, form text, metadata, empty/error/loading copy, line-height, measure, weight contrast, and responsive behavior.

If I change fonts, I verify the font is actually loaded or available. A font name in a style value does not count if the browser falls back silently.

Changing only the hero headline or one paragraph is not enough unless the user explicitly asked for that one element.

---

## What Type Must Do

Type has three jobs.

- Make the page understandable at a glance
- Make the content comfortable to read
- Give the surface a voice that matches its register

If a type choice does not help one of those jobs, I replace it.

---

## Reading Distance

I do not assume body text is fine because it is a common size. Phone, laptop, monitor, and TV are different reading situations.

Short, close interfaces can run tighter. Large displays and long reading contexts need more size, more line-height, and more controlled measure. Product UI can be dense. Brand display can be loud. Body copy still needs mercy.

---

## Content Length Rules The Measure

I match line-height and width to the text itself.

Microcopy wants tight leading and fast recognition. Short-form text wants two clean lines at most. Paragraphs want a comfortable measure, usually around the classic readable band. Long-form wants more air, not more decoration.

If a paragraph sprawls across the viewport, the layout is asking the reader to do the designer's work.

---

## Font Choice

For brand work, I use the physical-object method. I name the brand as a thing I can hold or see: a museum caption, a club flyer, a diner receipt, a technical manual, a ceramic stamp, a field notebook. Then I pick type that belongs to that object.

I ask whether the font choice carries any project-specific reason. A font that appears in every second launched product signals nothing — it just means the decision was not made. The brand already owns it, or it was the path of least resistance.

That test applies to whatever family is currently over-used. The answer changes over time; the question does not.

For product work, a strong system stack or one well-tuned sans is often the best choice. Product type should help operators move fast. It does not need a costume.

---

## Pairing & Font System
 
Add a second family only when it creates real contrast in structure, proportion, texture, or voice.
 
**The non-obvious truth:** One well-chosen font in multiple weights (Regular, Medium, Semibold, Bold + Italics) creates cleaner hierarchy than two competing faces. Only add a second font when you need genuine contrast.
 
**Bad pairing patterns:**
- Two similar geometric sans (tension without hierarchy)
- Fonts that compete for the same visual space
- A third font with no distinct job
**The pairing check:**
- Does the display font differ from body in x-height, stroke contrast, or width?
- Does each family serve a distinct job (headline vs body vs UI)?
- Would removing one font break the visual system?
If any answer is no, use one family with stronger weights instead.
 
**Three-font system (when justified):**
- **Display/Heading:** Bold personality, high contrast (serif, geometric, custom)
- **Body/Paragraph:** High readability, moderate x-height, comfortable at small sizes (humanist sans, system stack, text serif)
- **UI/Badge:** Compact, clear at small sizes, tabular numerals (condensed sans, mono, or body with tighter tracking)
Use three fonts only when each serves a visibly distinct purpose. Most work is better served by two fonts or one strong family.
 
---

## Hierarchy

Each text block needs a clear chain: hook, bridge, detail. More levels create fog. Fewer levels flatten the message.

Hierarchy can come from size, weight, color, position, spacing, and casing. Size alone is crude. The best systems combine two or three dimensions and keep the rest quiet.

Brand pages can use dramatic jumps. Product surfaces use compressed steps with weight and color doing more of the work.

---

## Dark Surfaces

Light type on dark backgrounds needs compensation. It reads thinner in some ways and brighter in others. I give it more line-height, a touch more spacing when needed, and a weight that feels optically correct.

I test it with real content, not a perfect headline.

---

## Details I Do Not Skip

- Tabular numbers for data, metrics, prices, and aligned values
- Balanced heading wraps where support exists
- Better paragraph wraps where support exists
- Small caps only when the font actually supports them
- Extra tracking for short all-caps labels
- No decorative faces for body text
- No widows, orphaned single words, or ugly rivers in important prose
- Font loading that avoids obvious layout shift

---

## What I Refuse

- Choosing the first trendy family that fits the category
- Calling one headline resize a typography pass
- Naming a font that is not actually loaded or available
- A flat scale where everything is almost the same size
- Body text wider than comfortable reading allows
- All-caps paragraphs
- Display fonts in product labels
- A third font with no distinct job
- Tiny mobile text to preserve a desktop layout
- Broken fallback metrics that make text jump

---

## How I Know The Type Is Working

- Type changes appear across real content, not only the hero
- The loaded or available font matches the claimed type choice
- The primary message reads before the decoration
- Body text can be read without effort
- The type voice matches the surface register
- Numbers align cleanly
- Headings break with intention
- The system works with long strings and short labels
- The page would still feel designed if color disappeared

STRICT RULE — NEVER BREAK THIS
Do not create report.md, any kind of report, summary, analysis file,
or extra documentation. This applies every time this file is used.
Generate no reports unless explicitly asked.
