# Design tokens

Read when you write tokens, convert them, or decide where they live.

## Where the tokens live

The source of truth is what the project already has, in this order of preference when several exist: the file the app's build reads (a global stylesheet with `--*` custom properties, Tailwind 4 `@theme`, `tailwind.config.*`, SCSS variables, `*.tokens.json`). Extend it in its own syntax and keep one source: a second copy drifts. TermF's Design system view reads all of these.

Without any, create `design/tokens.json` in the Design Tokens format (Community Group report 2025.10, stable) and generate the project's native form from it:

```
design/tokens.json        light values, or values that do not change with the theme
design/tokens.dark.json   the same paths, dark values only
design/tokens-to-css.mjs  copied from <skill-dir>/assets/
design/tokens.css         generated: node design/tokens-to-css.mjs
```

Write the regenerate command in `DESIGN.md`. A Tailwind 4 project keeps its tokens in its `@theme` block instead: that block is already its native form and gives both the utilities and the custom properties. Change the `darkBlock` function of the copied script when the project switches themes with a class or an attribute rather than the system preference.

## The format, in short

- A file is JSON; extension `.tokens.json` or `.tokens`; media type `application/design-tokens+json`.
- A token is an object with `$value`; a group is an object without it. `$type` set on a group applies to the tokens inside it; `$description` documents either. Names never start with `$` and never contain `{`, `}` or `.`.
- An alias is a string `"{group.token}"`; the generated CSS keeps it as `var(--group-token)`.
- `color`: `{ "colorSpace": "srgb", "components": [0.169, 0.298, 0.878], "alpha": 1, "hex": "#2b4ce0" }`. sRGB components go from 0 to 1; `oklch` is `[L 0–1, C, H degrees]`; `hex` is an optional six-digit fallback.
- `dimension`: `{ "value": 16, "unit": "px" }` (`px` or `rem`; the unit is required, even for 0). `duration`: `{ "value": 150, "unit": "ms" }`.
- `fontFamily`: a string or a list. `fontWeight`: a number from 1 to 1000 or a keyword. `number`, `cubicBezier` (four numbers).
- Composites: `typography` (fontFamily, fontSize, fontWeight, letterSpacing, lineHeight), `shadow` (color, offsetX, offsetY, blur, spread, inset), `border`, `transition`, `gradient`, `strokeStyle`.

```json
{
  "color": {
    "$type": "color",
    "brand": { "$value": { "colorSpace": "srgb", "components": [0.169, 0.298, 0.878], "hex": "#2b4ce0" } },
    "link": { "$value": "{color.brand}" }
  },
  "space": { "$type": "dimension", "2": { "$value": { "value": 8, "unit": "px" } } }
}
```

## Naming and scales

- Two levels are enough for most products: primitives (`color.blue.600`) and roles (`color.text`, `color.surface`, `color.action`), the roles aliasing the primitives. Components use the roles, so a theme changes only the aliases.
- One spacing scale (4, 8, 12, 16, 24, 32, 48, 64 is common), one radius scale, one type scale with line heights: every value off a scale is a choice nobody made.
- Name by role, never by look (`color.danger`, not `color.red`): the look changes, the role stays.
