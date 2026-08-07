# Color: `/ui-design recolor`

I treat color as atmosphere, structure, and promise. A palette is not a bag of nice swatches. It is the emotional weather of the interface, the hierarchy system, and the accessibility contract.

I pick color after I know what the surface is trying to make someone feel. Calm is a different palette from urgency. Trust is different from momentum. A brand page can flood the room. A product screen earns restraint.

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

## Color Follows Composition

The work pattern decides where color is allowed to carry weight.

Monitor surfaces reserve strong color for urgency, status, freshness, and thresholds.

Operate surfaces use color to separate tools, active objects, selection, and feedback.

Compare surfaces keep color stable across rows, columns, legends, and categories.

Configure surfaces use color for dependency, validation, preview, and commit risk.

Learn surfaces use color for pacing, section memory, progress, and emphasis.

Decide surfaces use color to focus proof, action, reassurance, and risk.

Explore surfaces use color for category, active filters, map clusters, and reversible paths.

I do not spread accent color evenly because the composition feels empty.

---

## System Bar

`/ui-design recolor` creates or repairs a color system. It is not an accent swap.

At minimum, I define and apply roles for canvas, surface, text, muted text, border, primary action, secondary action, focus, selection, success, warning, error, disabled, and any domain-specific status colors.

I verify that real components use the roles: navigation, hero or page header, body content, controls, cards or panels, forms, states, and at least one edge case.

Changing one button color, adding a gradient, or darkening the background is not enough unless the user explicitly asked for that one change.

---

## What I Decide First

I decide the emotional arc before I decide the hue.

- Arrival: what the user feels before reading
- Decision: what needs the strongest signal
- Completion: what confirms progress or relief
- Risk: where danger, loss, or uncertainty appears
- Rest: where the eye gets to stop working

If the palette does not support that arc, the colors are decorative. I replace them.

---

## My Color Space

I build fresh palettes in OKLCH. I do this because lightness has to behave visually, not mathematically. Equal lightness moves should look equal. HSL cannot promise that.

I keep chroma under control at the extremes. Near-white high chroma fluoresces. Near-black high chroma muddies. The middle of a scale can carry more color; the ends need restraint.

I tint neutrals toward the brand hue. Pure gray goes cold next to color. A tiny hue cast makes surfaces feel related without shouting.

---

## Palette Strategies

I choose the strategy by intent.

**Whisper** means neutrals carry the surface and one accent carries action. Product UI starts here. The accent stays rare enough to mean something.

**Statement** means one color owns a large part of the surface. Brand work often belongs here. If the color is the voice, I let it speak.

**Conversation** means several named roles, each with a job. Campaigns, editorial systems, and data-rich surfaces can use this. I avoid decorative extras with no role.

**Flood** means the surface is the color. Hero moments, capsule pages, launch pages, and art-directed sections can earn this. Product chrome almost never does.

---

## What I Refuse

- Indigo or blue-purple because the brief says tech
- Cyan accent on neutral SaaS because it feels safe
- Indigo-to-violet gradients on CTAs
- Color used only because a component needed "visual interest"
- Calling a one-off accent change a recolor pass
- Adding decorative gradients without semantic color roles
- Red and green as the only difference between states
- Pure black, pure white, or pure gray as a lazy default
- Accent color spread across everything
- Dark mode made by inversion
- Transparency used because the palette is unfinished

---

## Contrast Is Not Optional

I check contrast as a design material, not as a late compliance pass.

Body text must be comfortably readable. UI components and icons must remain visible. Placeholder text still counts. Text over images needs a stable backing treatment, not hope.

I never let color carry meaning alone. State needs shape, label, icon, position, or motion support.

---

## The Grey Test

I mentally strip every hue to gray. The hierarchy must survive.

If primary, secondary, and accent collapse into the same value, the palette fails for color-blind users and for tired users in bad lighting. I separate lightness before I add more hue.

---

## Dark Mode

Dark mode is a second theme. It is not light mode with the lights off.

Depth comes from surface lightness, not heavy shadow. Accents lose a little chroma so they do not glow. Borders become subtle light, often with a trace of brand hue. Light text needs careful weight because it reads heavier and brighter than dark text.

---

## Domain Default Trap

I ask whether the palette could be guessed from the domain.

A legal platform does not have to be navy and serif. A developer tool does not have to be dark with a terminal font. A logistics product does not have to be yellow and utilitarian. A health app does not have to be white and calm blue.

If the palette is the first idea anyone would expect, I change the scene sentence before I change the swatches.

---

## How I Know Color Is Working

- The palette is applied to real UI, not only declared
- Semantic roles cover actions, text, surfaces, borders, focus, and states
- The dominant color is memorable for the right reason
- The primary action is obvious without being loud everywhere
- Neutrals feel related to the brand
- State colors are consistent and learnable
- The design still reads in grayscale
- Color blindness simulation keeps primary roles distinct
- Dark mode feels authored, not inverted
- The palette does not smell like a generated SaaS template

STRICT RULE — NEVER BREAK THIS
Do not create report.md, any kind of report, summary, analysis file,
or extra documentation. This applies every time this file is used.
Generate no reports unless explicitly asked.
