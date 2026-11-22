---
name: brand-identity
description: >-
  Builds a product's visual identity as files the project keeps: three directions compared on a review page,
  brand guidelines (brand/guidelines.html printed to an A4 landscape PDF with headless Chrome), BRAND.md,
  DESIGN.md and the design tokens in the project's own format, or in W3C Design Tokens with CSS variables
  generated from them. Use when a task asks for a brand, a charter or style guide, a colour palette, a type
  scale, design tokens, a design system or a DESIGN.md. Not for drawing or exporting the logo files and icons
  (logo-export), nor for building screens with the tokens (the stack's own skill).
license: MIT
metadata:
  termf-stacks: "*"
  termf-roles: "brand ux front"
  termf-versions: "*"
  termf-verified: "2026-11-22 Design Tokens format 2025.10, Chrome headless --print-to-pdf"
  termf-sources: "https://www.designtokens.org/TR/2025.10/format/ https://www.designtokens.org/TR/2025.10/color/ https://developer.chrome.com/docs/chromium/headless https://chromedevtools.github.io/devtools-protocol/tot/Page/#method-printToPDF https://www.w3.org/TR/WCAG22/"
---
# Brand identity and design system

`<skill-dir>` below is the folder of this file (Claude Code prints it as the skill's base directory).

1. **Read what exists.** Look for the tokens where the project keeps them: custom properties (`--*`) in a global stylesheet (`:root` and its dark theme), Tailwind 4 `@theme` blocks or `tailwind.config.*`, SCSS variables, `*.tokens.json`; then `brand/`, `BRAND.md`, `DESIGN.md`, logo and icon files, the fonts the project ships, the favicon and app icons. Done when you can name, for colour, type, spacing, radius, shadow and motion, the one file that is the source of truth, or say there is none.
2. **Understand the product.** Read the README, the main screens and the copy: what it does, for whom, what sets it apart. Write it in two sentences and three traits, each with what it means visually ("calm: one strong colour, generous space"). Done when every later choice can point to one of them.
3. **Three directions on one page.** Write `brand/review.html` (self-contained): for each direction, the mark on light and dark, at full size and at 16 px, its palette with contrast ratios, its type pairing and one screen or card of the product dressed in it. When the ticket names a model ("the same style as X"), take only the elements it names and make the rest the product's own. Render the page with headless Chrome (step 7) and look at it. When the ticket leaves the choice open, report `blocked` with a choice question: the directions as options, the one you recommend as the proposed answer. Done when one direction is chosen, by the ticket or by the user.
4. **Tokens.** Keep the source of truth found in step 1 and extend it in its own syntax. Without one, write `design/tokens.json` in the Design Tokens format (see `references/tokens.md`), copy `<skill-dir>/assets/tokens-to-css.mjs` to `design/`, run `node design/tokens-to-css.mjs` to write `design/tokens.css`, and import that file where the app loads its global styles. Every colour token has its dark value (`design/tokens.dark.json`). Done when the project builds with the tokens and the regenerate command is written in `DESIGN.md`.
5. **Base components.** When the task asks for components, build or restyle them with the tokens only (button, field, select, checkbox, card, badge, dialog…), each with its states: default, hover, focus-visible, active, disabled, invalid, loading, in light and dark. Reuse the project's component folder and conventions; front builds them into screens. Done when no value in them is hard-coded.
6. **The guidelines.** Copy `<skill-dir>/assets/guidelines.html` to `brand/guidelines.html` and make it the product's, page by page (see `references/guidelines.md` for what each page holds and the bar to reach): set the tokens in its `:root` (colours as hex), the fonts (`@font-face` with files next to the page, or `data:` URIs), replace the `#mark` and `#wordmark` symbols with the real logo paths, rewrite every text in the language of the ticket, delete a page the brand does not need. The page measures and prints every colour value and contrast ratio itself. Done when no sample content is left (`grep -n "Acme" brand/guidelines.html` finds nothing) and every recommended text pair shows Pass.
7. **Print to PDF.** Run headless Chrome on the page (command in `## Verify`), then look at every page: open the PDF with the Read tool, or turn it into images (`pdftoppm -r 60 -png brand/guidelines.pdf <folder>/p`) and read them. Fix what you see: overflow, an empty or extra page, a heading alone at a page's foot, a fallback font, a missing background. Done when the page count and size are right (A4 landscape: 297 × 210 mm) and every page looks finished.
8. **BRAND.md and DESIGN.md.** Write them as `references/guidelines.md` describes: `BRAND.md` short, pointing to the files; `DESIGN.md` the design system in the code (principles, tokens and where they live, components and where they live, usage rules, the regenerate command). Done when every path they cite exists.
9. Run the commands of `## Verify` and put each one with its result in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- "The same style as X" is a reference for the elements named, never a copy: a brand that looks like another one says nothing about this product, and can infringe the other's trade dress.
- Brand colours often fail as text: a bright accent on white is usually under 3:1. Keep it for shapes and give the brand a darker text variant token; the guidelines page shows Fail until it is right.
- The dark theme is a palette of its own, not the light one inverted: lower the saturation of large surfaces, lift the text colours, and check every pair again on the dark background.
- Design Tokens 2025.10 writes a colour as an object (`colorSpace`, `components`, optional `alpha` and `hex`) and a dimension as `{ "value": 16, "unit": "px" }`; the older forms (`"#2b4ce0"`, `"16px"`) are no longer valid there.
- A font from a font service or CDN fails offline and silently falls back: the PDF then shows a font the brand does not use. Ship the font files with the guidelines.
- Print colours (CMYK, Pantone) are not a conversion of the screen values: write that they come from the printer's proof rather than inventing them.
- A stock photo never goes into a logo: the Pexels and Pixabay licences forbid it, and no free licence gives a trademark.
- `--print-to-pdf` without `--no-pdf-header-footer` stamps the date and the file path on every page.

## Verify

- `"<chrome>" --headless --no-pdf-header-footer --print-to-pdf=brand/guidelines.pdf brand/guidelines.html` (`<chrome>`: `google-chrome`, `chromium`, or `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` on macOS); the same with `--screenshot=<folder>/review.png --window-size=1440,2400` for `brand/review.html`.
- `node design/tokens-to-css.mjs` when the project uses `design/tokens.json`, then the project's build and its style lint.
- The contrast page of the PDF, read: every pair recommended for text shows Pass; say in the report which pairs are for large text or shapes only.
- Every file cited by `BRAND.md` and `DESIGN.md` exists (`ls` each one).

## Security

- The guidelines and the review page load nothing from the network: a shared PDF or HTML file that calls a third party leaks who opens it, and breaks offline.
- An SVG from outside (a client's file, a download) can carry scripts, `foreignObject` or event handlers: keep only shapes and paths before you put it in the guidelines or the app.
- Leave out of shared guidelines what is not public: unreleased product names, internal URLs, customers' logos, real people's data in the examples.
- Each outside image used in the guidelines has a licence checked and credited (skill `stock-media`).

## References

- Read `references/tokens.md` when you write or convert design tokens.
- Read `references/guidelines.md` before you fill the guidelines, `BRAND.md` or `DESIGN.md`.
- Templates: `<skill-dir>/assets/guidelines.html` (14 A4 landscape pages, measures its colours), `<skill-dir>/assets/tokens-to-css.mjs` (Design Tokens to CSS custom properties, no dependency).
