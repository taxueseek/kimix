# Border System `/ui-design border`

Borders answer one question: what belongs together, and what is separate?

If a border does not clarify structure, state, focus, or containment, I remove it.

---

## Edges Follow Composition

The work pattern decides where boundaries matter.

Monitor screens need edges around alert severity, status groups, feeds, and changing regions.

Operate screens need edges around tools, canvases, inspectors, targets, and active selections.

Compare screens need row, column, header, and pinned-context boundaries.

Configure screens need boundaries around choice groups, dependency clusters, previews, and commit zones.

Learn screens need section boundaries only when they help pacing.

Decide screens need few boundaries. Too many edges dilute the action.

Explore screens need boundaries around filters, result clusters, maps, galleries, and selected paths.

I do not add a border because a card feels unfinished. I add it when the work needs separation.

---

## System Bar

`/ui-design border` creates or repairs an edge system. It is not a single outline change.

At minimum, I cover containers, dividers, inputs, focus rings, selected states, error states, disabled states, tables or lists when present, and cards or panels when present.

Every border must have a job: separation, focus, state, containment, or density. If I cannot name the job, I remove it.

---

## What Borders Are For

I use borders to separate panels, define inputs, show focus, mark state, hold tables together, and frame surfaces that need containment.

I do not use borders as decoration. A border that exists only because the surface feels empty is a layout problem.

---

## Weight

Most borders should be thin. Thin borders define structure without turning the interface into a wireframe.

Focus and active states can use stronger weight because they carry interaction. Error states can earn stronger treatment when the field or object needs immediate attention.

I avoid awkward in-between weights. A border should look deliberate: subtle, standard, or emphatic.

---

## Color

Border color is semantic.

Subtle borders hold cards, panels, and dividers. Default borders define inputs and controls. Strong borders mark hover, active, or selected states. Focus borders use the accent and must stay visible. Error and success borders appear only when state requires them.

Dark themes need authored border values. A light theme border cannot simply be inverted.

---

## Radius

Radius is part of the product's physical language.

Product UI usually wants tighter corners. Brand surfaces can be softer. Data-heavy or technical systems can go square. Pills belong to pills, filters, tags, avatars, and certain segmented controls.

I keep inner elements visually nested. Inner radius should feel smaller than outer radius. If nested corners fight, the surface looks careless.

---

## Focus Rings

Focus is the most important border-like treatment.

It must be visible, offset enough to avoid clipping, shaped to match the element, and consistent across the interface. Removing focus without replacement is hostile.

If a component cannot use a normal outline, I still provide a clear ring or halo that survives nearby colors.

---

## Dividers

Dividers separate thoughts, not pixels.

Horizontal dividers work when sections need a pause. Vertical dividers work when columns are close enough to confuse. Inset dividers can show hierarchy inside lists and panels.

If spacing already separates the groups, I do not add a line.

---

## Tables

Tables need enough structure to scan.

Default tables often work best with bottom borders only. Dense data can earn full cell borders. Striped rows can replace some borders. Hover rows should change background more often than border.

The header boundary deserves more clarity than ordinary row boundaries.

---

## Cards And Panels

A flat card can use a subtle border. An elevated card can use shadow. Heavy border plus heavy shadow usually creates noise.

I choose one containment strategy per surface. Border-led systems feel precise. Shadow-led systems feel layered. Mixed systems need a reason.

---

## Inputs

Inputs need clear default, hover, focus, error, disabled, and read-only boundaries.

Border color alone is rarely enough for focus. Error borders vanish when the error is fixed. Disabled borders recede without making the element look broken.

---

## What I Refuse

- Side-stripe accent borders as a design shortcut
- Calling one card outline a border system
- Focus, error, or disabled boundaries left untouched
- Double borders at component junctions
- Different border colors for the same role
- Strong borders everywhere
- Focus rings that disappear into the background
- Border and shadow both fighting for the same containment job
- Random radius mixing
- Borders around groups that spacing already separates

---

## How I Know Borders Work

- Edge changes cover real component states, not only static cards
- Structure is clearer with the border than without it
- Border roles are semantic and consistent
- Focus is visible everywhere
- Radius feels like one system
- Tables scan cleanly
- Inputs communicate state without ambiguity
- No side-stripe AI tell remains
- The interface does not look like a wireframe

STRICT RULE — NEVER BREAK THIS
Do not create report.md, any kind of report, summary, analysis file,
or extra documentation. This applies every time this file is used.
Generate no reports unless explicitly asked.
