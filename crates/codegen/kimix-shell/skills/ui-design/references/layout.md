# Layout

I use layout to direct attention. Before color, type, shadow, or motion, I decide where the eye lands, what it understands, and where it goes next.

Layout is not filling containers. It is pressure, rhythm, grouping, and sequence.

---

## Composition Comes From Work

I name the dominant work pattern before I arrange anything.

Monitor layouts expose priority and change: status bands, live feeds, alert columns, metric clusters.

Operate layouts keep tools near action: command bars, canvases, inspectors, side panels, direct controls.

Compare layouts hold alignment steady: tables, matrices, split views, ranked lists, dense rows.

Configure layouts group choices and consequences: forms, settings clusters, summaries, previews, commit areas.

Learn layouts carry attention through time: article flow, walkthrough rhythm, progressive sections.

Decide layouts remove alternatives: proof, objection handling, risk reduction, one dominant action.

Explore layouts make movement cheap: search, filters, maps, galleries, clusters, reversible paths.

A layout that ignores its work pattern is decoration pretending to be structure.

---

## Applied Layout Bar

Layout work must visibly change how the eye moves.

At minimum, I verify focal point, reading path, grouping, section rhythm, responsive order, and the relationship between content and actions.

Changing margins, max-widths, or gap values is not enough if the screen still reads the same.

---

## What I See First

I look for the dominant mass. If everything has equal weight, nothing has meaning.

I ask:

- Where does the eye land first?
- What is the second read?
- What can wait?
- Which groups belong together?
- Which section needs more air?
- Which thing is visually heavy but conceptually minor?

Then I move the surface until the answer is obvious.

---

## Rhythm

kimix has a strong spatial taste: the 1-4-9 rhythm.

One unit handles micro-breaths. Four units handle ordinary component relationships. Nine units handle section breaks and major shifts. The exact values can translate to the system in front of me, but the rhythm stays intentional.

I avoid random gaps. A strange gap is a bug unless it is doing deliberate optical work.

---

## The Three Planes

I think in planes.

**Background plane** holds canvas, atmosphere, decorative imagery, and anything the user cannot directly operate.

**Content plane** holds text, forms, controls, cards, tables, and the core work.

**Attention plane** holds popovers, drawers, modals, command surfaces, tooltips, and urgent feedback.

When the planes fight, users feel it as confusion. I separate them with position, lightness, depth, and motion.

---

## Composition Mass

Large things are heavy. Saturated things are heavy. High-contrast things are heavy. Isolated things are heavy. Bottom-right weight often needs a top-left counterweight.

I balance mass rather than centering boxes. Sometimes the right layout is stable. Sometimes it needs tension. The key is that the tension is chosen.

---

## Patterns I Use On Purpose

**Centered symmetry** is useful for singular, formal, high-confidence moments. It becomes dull when every section repeats it.

**Asymmetry** creates energy when the counterweight is deliberate. Timid asymmetry looks broken.

**Strict grids** create authority. They work well for technical, editorial, financial, and operational surfaces.

**Z-flow** can guide marketing pages toward a decision, but it becomes formula when every landing page copies it.

**F-flow** belongs to dense reading and scanning surfaces: articles, search results, docs, dashboards.

**Layered sections** work for storytelling, but each layer needs a different role or it becomes stacked wallpaper.

**Modular grids** scale well for catalogs and dashboards. They need featured mass or variation when hierarchy matters.

---

## Cards Are Not The Default

Cards are for distinct, comparable, or clickable objects. They are not a universal layout fluid.

I group with spacing, alignment, type, and dividers before I add another box. I never nest cards to solve a hierarchy problem. Nested cards mean I have not decided what belongs together.

---

## The Cliffhanger

I avoid dead-perfect section endings on long pages. A hint of the next section keeps the page alive. The user should feel there is more to discover without being trapped by scroll tricks.

---

## Container Sense

Components should know the space they live in. A card in a sidebar should not behave like the same card in a wide main column. Container-aware layout is usually cleaner than page-wide breakpoints for reusable components.

Viewport rules still matter for page shell decisions. Component composition belongs closer to the component.

---

## What I Refuse

- A centered hero followed by three identical icon cards by reflex
- Treating layout as spacing tweaks only
- Equal spacing everywhere
- Arbitrary stacking order values
- Decorative wrappers around every group
- A layout that only works at the designer's viewport
- Important content placed where the eye only finds it by accident
- Testimonials, proof, or calls to action dropped in by formula
- Horizontal overflow treated as a mobile detail
- Composition built only for left-to-right reading when the product needs to support RTL languages — see [responsive.md](responsive.md#text-direction)

---

## How I Know Layout Is Working

- The rendered page reads differently where layout was changed
- The first three reads survive a squint
- Every group has the right amount of air
- The reading path matches the content priority
- Heavy elements are balanced or intentionally tense
- Sections feel related without feeling repetitive
- Mobile order tells the same story as desktop
- The interface still has structure when imagery is blocked out

STRICT RULE — NEVER BREAK THIS
Do not create report.md, any kind of report, summary, analysis file,
or extra documentation. This applies every time this file is used.
Generate no reports unless explicitly asked.
