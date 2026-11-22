# Guidelines, BRAND.md and DESIGN.md

Read before you fill `brand/guidelines.html`, `BRAND.md` or `DESIGN.md`.

## The bar

The guidelines are what a brand studio hands over when it is paid for the PDF: someone who never met the team can apply the brand from it alone. Every rule is shown, not only written (a misuse is drawn, a clear space is measured, a colour carries its values), every number is exact, and every page looks finished when printed.

## Pages of the template, and what each holds

1. **Cover**: the logo, the title, version, date, audience.
2. **Contents**: the files beside the document and the page list (update the numbers when you delete a page).
3. **The brand**: what the product does and for whom, in two sentences; three traits, each with what it means visually; the visual tone in four tiles.
4. **Logo**: the primary logo large, why it looks this way (one paragraph), the source files.
5. **Variants**: primary on light, reversed on dark, white on the brand colour, one colour (black), mark alone, stacked. Only the variants that exist as files.
6. **Clear space and sizes**: the clear-space unit drawn around the logo, minimum sizes for screen (px) and print (mm), the mark rendered at 48, 32, 24 and 16 px. Pick the minimums by looking at the renders: the size where a detail closes up is the limit.
7. **Misuse**: six drawn mistakes, each the real logo transformed (stretched, recoloured, rotated, effects, busy ground, retyped name).
8. **Colour**: each colour with its name, token, use; the page prints HEX, RGB and OKLCH from the tokens. The proportion bar shows a typical layout.
9. **Contrast**: every text and background pair the brand recommends, measured by the page. A pair that fails 4.5:1 is either removed or labelled for large text or shapes only, and the note says what to use instead.
10. **Typefaces**: each family with its role, weights, glyph set, a sample sentence, its licence and where its files are.
11. **Type scale**, with spacing and corner radius: token, size and line height, weight, a sample in the real font.
12. **Icons and imagery**: the icon grid and stroke, the icon set the project uses, photo rules, credits.
13. **In use**: app icon, favicon in a tab, social image (1200 × 630), with the real exports.
14. **Files**: every export with its size and use, versions, who to ask.

## Writing

- The language of the ticket (or of the product's README when the ticket does not say); short sentences, rules as instructions ("Keep a clear space of x"), no marketing adjectives.
- Positioning sentences, the tagline and any text the product will show are the user's to approve: propose them in the report when the ticket does not give them.
- Leave no sample content: names, numbers, captions and file names are the product's.

## BRAND.md (short)

```markdown
# <Product> brand
One paragraph: what the identity says.
- Guidelines: brand/guidelines.pdf (source: brand/guidelines.html)
- Logos: brand/logo/ (logo, reversed, mono, mark)
- Exports: brand/exports/ (favicon, app icons, social image)
- Colours and type: the tokens, see DESIGN.md
```

## DESIGN.md

```markdown
# Design system
## Principles        three to five, each a rule the code can follow
## Tokens            where they live (file), how to regenerate, the families and their naming
## Components        each base component: path in the code, variants, states, when to use it
## Usage rules       spacing, layout, motion (with prefers-reduced-motion), accessibility (contrast, focus)
## Brand             link to BRAND.md
```

Every path cited must exist; TermF's Design system view reads `DESIGN.md`, the tokens and `brand/` to show and check them.
