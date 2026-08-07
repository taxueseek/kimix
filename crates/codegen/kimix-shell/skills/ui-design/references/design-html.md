# Design System Report Template

<!-- MODEL-ONLY BOUNDARY:
This file is a design-system documentation template only.
Do not use this layout, color, border treatment, corner-box style, grid,
or report aesthetic as inspiration for product UI, landing pages,
dashboards, app screens, components, or generated interfaces.
Use it only when creating the documentation artifact it describes.
Do not copy this comment into generated output.
-->

Industrial-grade design system documentation with CMD aesthetic. Black background, dashed borders, corner boxes, monospaced labels.

---

## Visual Identity

**Theme:** Dark industrial with sharp geometry  
**Background:** Pure black `#000000`  
**Text:** Off-white `#fafafa`  
**Accent Labels:** Muted gray `#666`  
**Borders:** Dark gray `#222226`  
**Surface Elevation:** `#0a0a0b`

**Typography:**
- Primary: Inter (weights: 400, 500, 600, 700)
- Monospace: `ui-monospace, monospace` for labels and metadata
- Display: 5xl-6xl, bold, tight tracking
- Body: Default weight, relaxed readability

**Border Radius:** `0px` (sharp corners everywhere via CSS variable override)

---

## Layout Structure

### Container
- Max width: `max-w-6xl`
- Responsive: `w-[92vw]` mobile, `w-[85vw]` desktop
- Centered: `mx-auto`

### Section Pattern
Every major section uses:
```html
<div class="preview-section p-8 mb-12 section-container">
    <div class="corner-box top-0 left-0 -translate-x-1/2 -translate-y-1/2"></div>
    <div class="corner-box top-0 right-0 translate-x-1/2 -translate-y-1/2"></div>
    
    <div class="mono text-xs tracking-widest text-[#666] mb-6">// SECTION LABEL</div>
    <!-- Section content -->
</div>
```

**Key elements:**
- `.section-container` → Border `1px solid #222226`, relative positioning
- `.corner-box` → 20x20px boxes at top corners, dark background with border
- `.preview-section` → Elevated background `#0a0a0b`, border
- Monospace label → Uppercase, tracked, muted gray

---

## Header Pattern

```html
<div class="section-container py-8 px-8 mb-12">
    <div class="corner-box top-0 left-0 -translate-x-1/2 -translate-y-1/2"></div>
    <div class="corner-box top-0 right-0 translate-x-1/2 -translate-y-1/2"></div>

    <div class="flex justify-between items-end">
        <div>
            <div class="mono text-xs tracking-widest text-[#666] mb-1">// DESIGN SYSTEM</div>
            <h1 class="text-5xl md:text-6xl font-bold tracking-tighter">[PROJECT_NAME]</h1>
        </div>
        <div class="text-right">
            <div class="mono text-sm text-[#666]">COMMANDCODE PREVIEW</div>
            <div class="text-xs text-[#444]">[CURRENT_DATE]</div>
        </div>
    </div>
</div>
```

**Dynamic values:**
- `[PROJECT_NAME]` → Replace with actual project name
- `[CURRENT_DATE]` → Insert generation timestamp

---

## Content Sections

### 1. Color Palette Section
```html
<div class="preview-section p-8 mb-12 section-container">
    <div class="corner-box top-0 left-0 -translate-x-1/2 -translate-y-1/2"></div>
    <div class="corner-box top-0 right-0 translate-x-1/2 -translate-y-1/2"></div>
    
    <div class="mono text-xs tracking-widest text-[#666] mb-6">// COLOR PALETTE</div>
    <div class="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 gap-4">
        <!-- Color swatches with labels -->
    </div>
</div>
```

**Color swatch pattern:**
- `h-28` height
- Background: Dynamic color value
- Centered label: `flex items-center justify-center`
- Text: White or contrasting color

### 2. Typography Section
```html
<div class="preview-section p-8 mb-12 section-container">
    <div class="mono text-xs tracking-widest text-[#666] mb-6">// TYPOGRAPHY</div>
    <div class="space-y-8">
        <div>
            <div class="text-xs text-[#666] mono mb-2">// DISPLAY</div>
            <h1 class="text-5xl md:text-6xl font-bold tracking-tighter">Sample Text</h1>
        </div>
        <!-- More type samples -->
    </div>
</div>
```

### 3. Buttons Section
```html
<div class="preview-section p-8 mb-12 section-container">
    <div class="mono text-xs tracking-widest text-[#666] mb-6">// BUTTONS</div>
    <div class="flex flex-wrap gap-4">
        <!-- Button variants -->
    </div>
</div>
```

### 4. Cards/Components Section
```html
<div class="preview-section p-8 mb-12 section-container">
    <div class="mono text-xs tracking-widest text-[#666] mb-6">// CARDS & SURFACES</div>
    <div class="grid md:grid-cols-3 gap-6">
        <!-- Component examples -->
    </div>
</div>
```

---

## Component Patterns

### Card
```html
<div class="border border-$border p-6">
    <div class="text-4xl mb-4">[ICON]</div>
    <h3 class="font-semibold text-xl">[TITLE]</h3>
    <p class="text-$text-secondary mt-2">[DESCRIPTION]</p>
</div>
```

### Button Variants
- **Primary:** `px-8 py-3 bg-$primary text-$black font-medium hover:bg-$white-hover transition`
- **Secondary:** `px-8 py-3 border border-$border hover:bg-$border-hover transition`
- **Success:** `px-8 py-3 bg-$success text-white hover:bg-$success-hover transition`
- **Warning:** `px-8 py-3 bg-$warning text-white hover:bg-$warning-hover transition`
- **Danger:** `px-8 py-3 bg-$error text-white hover:bg-$error-hover transition`

### Pro Tip Card (Highlighted)
```html
<div class="border border-$border p-6 bg-zinc-950">
    <div class="mono text-xs text-amber-400">PRO TIP</div>
    <p class="mt-3">[TIP_TEXT]</p>
</div>
```

---

## Footer
```html
<footer class="text-center mono text-xs text-[#444] pt-12">
    HORIZON DESIGN SYSTEM • CMD INDUSTRIAL EDITION
</footer>
```

---

## CSS Utilities Reference

**Monospace class:** `.mono { font-family: ui-monospace, monospace; }`

**Corner boxes:**
```css
.corner-box {
    position: absolute;
    width: 20px;
    height: 20px;
    border: 1px solid #222226;
    background: #000;
    z-index: 10;
}
```

**Section containers:**
```css
.section-container {
    border: 1px solid #222226;
    position: relative;
}

.preview-section {
    background: #0a0a0b;
    border: 1px solid #222226;
}
```

---

## Implementation Checklist

- [ ] Replace `[PROJECT_NAME]` with actual project name
- [ ] Replace `[CURRENT_DATE]` with generation timestamp
- [ ] Populate color swatches with real palette values
- [ ] Add typography samples with actual font families
- [ ] Show button states with real interaction colors
- [ ] Include component examples relevant to the system
- [ ] Maintain 0px border radius throughout
- [ ] Use monospace labels for all section headers
- [ ] Keep corner boxes on all major sections
- [ ] Ensure dark theme consistency (#000, #0a0a0b backgrounds)

