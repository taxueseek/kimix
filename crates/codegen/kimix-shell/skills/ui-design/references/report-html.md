# HTML Report Layout

<!-- MODEL-ONLY BOUNDARY:
This file is guidance for the visual layout of generated HTML report
artifacts for smell, checkup, and review. Do not use this report aesthetic,
layout, Tailwind classes, borders, score blocks, or metadata treatment as
inspiration for product UI, landing pages, dashboards, app screens,
components, or generated interfaces.
Do not copy this comment into generated output.
-->

The HTML report must be a designed report page, not markdown text placed in an
HTML file.

Use Tailwind CSS to create an authored layout that makes the audit easy to
scan. The report should feel like a kimix diagnostic artifact: dark,
sharp, structured, dense, and readable.

## Tailwind Runtime

Use Tailwind through the CDN script in the generated HTML report:

```html
<script src="https://cdn.tailwindcss.com"></script>
```

The HTML report is expected to be opened with network access. Offline rendering
is not guaranteed. If the user explicitly asks for an offline or self-contained
report, do not use the CDN. Instead, embed the small CSS needed for the exact
classes and helpers used by the report.

Do not reference a local Tailwind build, local package, or project stylesheet.
The report must not depend on the target app's build pipeline.

## Core Requirement

Do not output the report as paragraphs only.

Every generated report must have visible layout structure:

- Page container with max width and responsive padding.
- Header block with report label, project name, mode, and date.
- Score or verdict block with a large visual score or status.
- TL;DR block with concise diagnosis and next action.
- Section containers with borders and clear headings.
- Tables for score matrices and comparable heuristics.
- Issue blocks for priority findings.
- Lists or rows for positive and negative signals.
- Footer line with generation metadata.

If the content is just a sequence of paragraphs under headings, the report is
wrong. Convert prose into structured blocks, rows, tables, badges, and grouped
sections.

## Tailwind Rules

Use Tailwind utility classes for the report layout.

Required:

- Load Tailwind with `<script src="https://cdn.tailwindcss.com"></script>`
  unless the user explicitly asks for an offline report.
- Use `bg-black` or near-black canvas.
- Use bordered section containers.
- Use responsive padding such as `p-6 md:p-10`.
- Use a readable max width such as `max-w-5xl` or `max-w-6xl`.
- Use `overflow-x-auto` around wide tables.
- Use `grid`, `flex`, and responsive breakpoints for layout.
- Use monospace labels for report metadata, section tags, and scores.

Allowed:

- A small `<style>` block for font family and tiny reusable helpers.
- Tailwind arbitrary values for exact report colors.

Avoid:

- Large custom CSS systems.
- Decorative gradients.
- Rounded marketing cards.
- Centered hero-page composition.
- Floating ornamental backgrounds.

## Canonical Scaffold

Use this structure as the default report scaffold. Replace bracketed values and
repeat rows or issue blocks as needed. Keep the class system and section shape
stable unless the report content truly requires an adjustment.

```html
<!DOCTYPE html>
<html lang="en">
<head>
	<meta charset="UTF-8" />
	<meta name="viewport" content="width=device-width, initial-scale=1.0" />
	<title>[PROJECT] - [MODE] Report</title>
	<script src="https://cdn.tailwindcss.com"></script>
	<style>
		body { font-family: Inter, ui-sans-serif, system-ui, sans-serif; }
		.mono { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
		.report-section { border: 1px solid #222226; background: #0a0a0b; position: relative; }
		.corner-box { position: absolute; width: 20px; height: 20px; border: 1px solid #222226; background: #000; }
	</style>
</head>
<body class="bg-black text-[#fafafa]">
	<main class="mx-auto w-full max-w-6xl px-5 py-8 md:px-10 md:py-12">
		<header class="report-section mb-8 p-6 md:p-8">
			<div class="corner-box left-0 top-0 -translate-x-1/2 -translate-y-1/2"></div>
			<div class="corner-box right-0 top-0 translate-x-1/2 -translate-y-1/2"></div>
			<div class="flex flex-col gap-6 md:flex-row md:items-end md:justify-between">
				<div>
					<div class="mono mb-2 text-xs tracking-widest text-[#666]">// [MODE]</div>
					<h1 class="text-4xl font-bold tracking-tight md:text-6xl">[PROJECT]</h1>
				</div>
				<div class="mono text-left text-xs text-[#666] md:text-right">
					<div>COMMANDCODE REPORT</div>
					<div class="text-[#444]">[DATE]</div>
				</div>
			</div>
		</header>

		<section class="report-section mb-8 p-6 md:p-8">
			<div class="corner-box left-0 top-0 -translate-x-1/2 -translate-y-1/2"></div>
			<div class="corner-box right-0 top-0 translate-x-1/2 -translate-y-1/2"></div>
			<div class="grid gap-8 md:grid-cols-[minmax(0,0.8fr)_minmax(0,1.2fr)]">
				<div>
					<div class="mono mb-2 text-xs tracking-widest text-[#666]">// OVERALL</div>
					<div class="text-6xl font-bold tracking-tight md:text-7xl">[SCORE]</div>
					<div class="mt-1 text-lg text-[#666]">/ [MAX]</div>
					<div class="mt-4 inline-flex border border-[#fbbf24]/30 bg-[#fbbf24]/10 px-3 py-1 text-sm text-[#fbbf24]">[VERDICT]</div>
				</div>
				<div>
					<div class="mono mb-3 text-xs tracking-widest text-[#666]">// TL;DR</div>
					<p class="text-sm leading-6 text-[#a0a0a0]">[SHORT DIAGNOSIS]</p>
					<p class="mt-4 text-sm leading-6 text-[#fbbf24]">[PRIMARY RECOMMENDATION]</p>
				</div>
			</div>
		</section>

		<section class="report-section mb-8 p-6 md:p-8">
			<div class="mono mb-6 text-xs tracking-widest text-[#666]">// HEURISTIC SCORES</div>
			<div class="overflow-x-auto">
				<table class="w-full min-w-[720px] border-collapse text-sm">
					<thead>
						<tr class="border-b border-[#222226] text-left mono text-xs text-[#666]">
							<th class="py-3 pr-4 font-normal">#</th>
							<th class="py-3 pr-4 font-normal">HEURISTIC</th>
							<th class="py-3 pr-4 font-normal">SCORE</th>
							<th class="py-3 font-normal">KEY FINDING</th>
						</tr>
					</thead>
					<tbody>
						<tr class="border-b border-[#1a1a1e]">
							<td class="py-4 pr-4 text-[#555]">1</td>
							<td class="py-4 pr-4 text-[#fafafa]">[HEURISTIC]</td>
							<td class="py-4 pr-4 text-[#fbbf24]">[SCORE]</td>
							<td class="py-4 text-[#a0a0a0]">[FINDING]</td>
						</tr>
					</tbody>
				</table>
			</div>
		</section>

		<section class="report-section mb-8 p-6 md:p-8">
			<div class="mono mb-6 text-xs tracking-widest text-[#666]">// COGNITIVE LOAD / RISK</div>
			<div class="grid gap-6 md:grid-cols-2">
				<div>
					<div class="mb-3 text-xl font-semibold">[LEVEL]</div>
					<ul class="space-y-3 text-sm text-[#a0a0a0]">
						<li><span class="mono mr-2 text-[#34d399]">PASS</span>[POSITIVE SIGNAL]</li>
						<li><span class="mono mr-2 text-[#fbbf24]">WATCH</span>[RISK SIGNAL]</li>
						<li><span class="mono mr-2 text-[#ef4444]">FAIL</span>[NEGATIVE SIGNAL]</li>
					</ul>
				</div>
				<div>
					<div class="mono mb-3 text-xs tracking-widest text-[#666]">// NEXT MODES</div>
					<div class="flex flex-wrap gap-2">
						<span class="border border-[#333] px-3 py-1 text-sm text-[#a0a0a0]">/[MODE]</span>
					</div>
				</div>
			</div>
		</section>

		<section class="report-section mb-8 p-6 md:p-8">
			<div class="mono mb-6 text-xs tracking-widest text-[#666]">// WHAT'S WORKING</div>
			<div class="grid gap-4 md:grid-cols-3">
				<article class="border border-[#1f1f24] p-4">
					<h2 class="mb-2 text-sm font-semibold">[TITLE]</h2>
					<p class="text-sm leading-6 text-[#a0a0a0]">[DESCRIPTION]</p>
				</article>
			</div>
		</section>

		<section class="report-section mb-8 p-6 md:p-8">
			<div class="mono mb-6 text-xs tracking-widest text-[#666]">// PRIORITY ISSUES</div>
			<div class="space-y-5">
				<article class="border border-[#2a1d22] bg-[#110b0d] p-5">
					<div class="mono mb-2 text-xs text-[#f43f5e]">[P0/P1/P2]</div>
					<h2 class="mb-2 text-lg font-semibold">[ISSUE TITLE]</h2>
					<p class="mb-4 text-sm leading-6 text-[#a0a0a0]">[EVIDENCE]</p>
					<div class="border-l border-[#fbbf24] pl-4 text-sm leading-6 text-[#fbbf24]">FIX: [RECOMMENDATION]</div>
				</article>
			</div>
		</section>

		<footer class="mono py-8 text-center text-xs tracking-widest text-[#444]">
			Generated with CommandCode - [DATE]
		</footer>
	</main>
</body>
</html>
```

## Responsive Requirements

The report must work on mobile and desktop:

- Stack header and score columns on small screens.
- Keep tables horizontally scrollable.
- Use readable text sizes on mobile.
- Do not rely on hover to reveal essential content.
- Keep body copy line length readable.

## Quality Bar

Before finishing the report, check:

- Can the user understand the verdict in five seconds?
- Are the most important issues visually prominent?
- Are scores presented in a table, not prose?
- Are fixes attached to the issues they solve?
- Is the report scannable without reading every paragraph?
- Does the page still work when opened as a static HTML file?

If any answer is no, redesign the report layout before returning it.
