---
title: Design system
order: 4
---

The Design system view reads a repository's design system from its code, without running anything, and lists every inconsistency with its file and line.

## What it reads

Its tokens, its components and their variants, and what drifted: colours typed by hand, sizes off the scale, a dark theme half done. A **Brand guidelines** tab shows the logos and the guidelines, and exports them to PDF.

![The Design system view: tokens by family and the inconsistencies found, contrast, hard-coded values, sizes off the scale](shot:design "Tokens and inconsistencies of a repository. Sample data.")

- **Tokens**: CSS custom properties, Tailwind, SCSS and W3C design tokens, in both themes.
- **Components**: Svelte, Vue, Astro and JSX, with their variants and stories.

## Find and fix inconsistencies

1. Open **Design system** among the project's views, or from the palette. Pick the repository when the project has several.
2. **Inconsistencies** lists hard-coded values with the token to use, sizes off the scale, near-identical colours, unused or undefined tokens, an incomplete dark theme, contrasts under WCAG AA and duplicate components.
3. Each one opens at its line in TermF's editor.
4. **Fix with the team** writes a message to the CTO with the findings you picked. Nothing is sent or started without you.

## Defaults

- Read-only: nothing from the repository runs, no Node, no git; a `tailwind.config` is read as plain text.
- Follows `.gitignore` and skips dependencies, builds, tests and generated files.
- Bounded at 4,000 files and 8 seconds.
- The Brand guidelines tab reads `brand/` and `BRAND.md`, where the brand designer writes.

> [!NOTE]
> Components are listed, not rendered: rendering a Storybook would mean running the repository's code. CSS-in-JS (styled-components, MUI themes, vanilla-extract) is not read. The view was checked with a simulated backend, not yet in the app's real window; the PDF export was tried with Chrome on macOS only.
>
> **Tried so far.** Covered by automated tests on throwaway repositories and by reading TermF's own app and site; the PDF export ran with a real Chrome on macOS.
