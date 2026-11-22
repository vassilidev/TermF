# Tailwind v3 and v4: telling them apart, keeping v3, upgrading

Read when the project is on v3, when a v3 config meets a v4 install, or when the task is an upgrade. Facts checked on 2026-11-22 against tailwindcss@4.3.5 and the official upgrade guide (https://tailwindcss.com/docs/upgrade-guide), restated here because Tailwind's docs are not open-licensed.

Contents: 1. Which version · 2. Working on a v3 project · 3. Upgrade procedure · 4. Config mapping · 5. Class changes · 6. Behaviour changes

## 1. Which version
| Look at | v3 | v4 |
|---|---|---|
| `tailwindcss` in the lockfile | 3.x | 4.x |
| Entry stylesheet | `@tailwind base/components/utilities` | `@import "tailwindcss";` |
| Tokens | `tailwind.config.*` → `theme.extend` | `@theme { --color-…: … }` in CSS |
| Build plugin | `tailwindcss` + `autoprefixer` in `postcss.config` | `@tailwindcss/postcss`, `@tailwindcss/vite` or `@tailwindcss/cli` |
| JS plugins | `plugins: [...]` in the config | `@plugin "…";` in CSS |

A v4 install with a leftover `tailwind.config.js` only uses it when a stylesheet says `@config "<path>";`. Do not "fix" such a project by moving tokens around: report the mix and keep to the style the entry stylesheet uses.

## 2. Working on a v3 project
- Keep writing v3: `theme.extend` tokens, `@layer components` for shared classes, opacity utilities if the code already uses them.
- New files with classes must be covered by the `content` globs, or their classes are missing in production.
- Container queries need the `@tailwindcss/container-queries` plugin; use them only if it is installed.
- v3 browser support is broader; it is the right choice for projects that still target Safari before 16.4.

## 3. Upgrade procedure (only when the ticket asks)
1. Check the browser targets (v4 needs Safari 16.4+, Chrome 111+, Firefox 128+) and that no stylesheet relies on Sass, Less or Stylus features (v4 does not run behind them). Stop and report if either blocks.
2. On a clean branch, run `npx @tailwindcss/upgrade` (Node.js 20+). It rewrites the entry CSS, the config and the class names in templates.
3. Read the whole diff. The tool cannot see class names built at runtime or stored in data, nor `@apply` in framework `<style>` blocks that need `@reference`.
4. Swap the build integration: `@tailwindcss/vite` for Vite projects, `@tailwindcss/postcss` otherwise; `postcss-import` and `autoprefixer` can go, v4 does both.
5. Go through section 6 and add the explicit colors and cursors the design relied on.
6. Build, then look at every page or story with borders, rings, shadows, placeholders, buttons and `space-*` / `divide-*` lists, in both themes.

## 4. Config mapping
| v3 config | v4 equivalent |
|---|---|
| `theme.extend.colors.brand` | `--color-brand` in `@theme` |
| `theme.extend.fontFamily`, `spacing`, `borderRadius`, `screens` | `--font-*`, `--spacing` (and `--spacing-*`), `--radius-*`, `--breakpoint-*` in `@theme` |
| `content: [...]` | automatic detection; add `@source "<path>"`, exclude with `@source not "<path>"`, set the base with `@import "tailwindcss" source("<dir>")` |
| `safelist` | `@source inline("…")` (brace expansion allowed) |
| `darkMode: 'class'` | `@custom-variant dark` with a `:where(.dark, .dark *)` selector (or a `data-` attribute selector) |
| `prefix: 'tw-'` | `@import "tailwindcss" prefix(tw);`, classes written `tw:flex` |
| `important: true` / leading `!` | trailing `!` on the class (`flex!`) |
| `corePlugins`, `separator` | not supported |
| `container.center`, `container.padding` | redefine with `@utility container { … }` |
| `@layer utilities { .x {} }` | `@utility x { … }` |
| `theme('colors.red.500')` in CSS | `var(--color-red-500)`; in media queries `theme(--breakpoint-xl)` |
| `resolveConfig` in JS | removed: read CSS variables (`getComputedStyle(document.documentElement).getPropertyValue('--color-brand')`) |
| `addVariant` for a simple variant | `@custom-variant name (selector);` |

Remove a default breakpoint with `--breakpoint-2xl: initial;`, and all of them with `--breakpoint-*: initial;` before defining your own. Keep one unit for all breakpoints so they sort correctly.

## 5. Class changes
Renamed (v3 → v4):
| v3 | v4 |
|---|---|
| `shadow-sm` | `shadow-xs` |
| `shadow` | `shadow-sm` |
| `drop-shadow-sm` | `drop-shadow-xs` |
| `drop-shadow` | `drop-shadow-sm` |
| `blur-sm` | `blur-xs` |
| `blur` | `blur-sm` |
| `backdrop-blur-sm` | `backdrop-blur-xs` |
| `backdrop-blur` | `backdrop-blur-sm` |
| `rounded-sm` | `rounded-xs` |
| `rounded` | `rounded-sm` |
| `outline-none` | `outline-hidden` |
| `ring` | `ring-3` |

Removed: `bg-opacity-*`, `text-opacity-*`, `border-opacity-*`, `divide-opacity-*`, `ring-opacity-*`, `placeholder-opacity-*` (use `/<opacity>` on the color), `flex-shrink-*` (`shrink-*`), `flex-grow-*` (`grow-*`), `overflow-ellipsis` (`text-ellipsis`), `decoration-slice` / `decoration-clone` (`box-decoration-slice` / `box-decoration-clone`).

Deprecated in 4.2: `start-*`, `end-*` → `inset-s-*`, `inset-e-*`. Added in 4.2: logical sizing and spacing such as `inline-*`, `block-*`, `pbs-*`, `mbs-*`, `border-bs-*`. Added in 4.3: `@container-size`, `scrollbar-*`, `scrollbar-gutter-*`, `zoom-*`, `tab-*`.

Syntax:
- CSS variable shorthand in arbitrary values: `bg-(--brand)` instead of `bg-[--brand]`.
- Commas inside `grid-cols-[…]`, `grid-rows-[…]` and `object-[…]` stay commas: separate with underscores (`grid-cols-[max-content_auto]`).
- Variant order is left to right: v3 `first:*:pt-0` becomes `*:first:pt-0`.
- `transform-none` no longer resets `rotate-*`, `scale-*`, `translate-*` (they are separate CSS properties now): use `scale-none` and the like; `transition-[opacity,transform]` becomes `transition-[opacity,scale]`.
- `transition` and `transition-colors` also animate `outline-color`: give the outline its color in both states, or set it unconditionally.
- Gradients: a variant overriding `from-*` keeps the other stops; reset a middle stop with `via-none`.

## 6. Behaviour changes to compensate
| Change in v4 | What to add when the design relied on v3 |
|---|---|
| `border-*` and `divide-*` default to `currentColor` | an explicit `border-gray-200` (or the project's token), or a base-layer rule setting the old default |
| `ring` is 1px, `currentColor` | `ring-3 ring-blue-500` (or the project's focus token) |
| `outline` is 1px solid; `outline-none` sets `outline-style: none` | `outline-hidden` where the v3 code meant "no visible outline but keep it for forced colors" |
| placeholder = current text color at 50% | an explicit placeholder color |
| buttons use `cursor: default` | `cursor-pointer` on the button component, or a base-layer rule |
| `<dialog>` margins reset | `m-auto` on dialogs that were centred |
| `hidden` attribute beats `flex` / `block` | remove the attribute to show the element |
| `hover:` only inside `(hover: hover)` | nothing for most designs; `@custom-variant hover (&:hover);` only if touch hover was intended |
| `space-*` and `divide-*` use `:not(:last-child)` and bottom margins | switch the list to `flex` or `grid` with `gap-*` |
| separately built stylesheets (Vue, Svelte, CSS modules) do not see the theme | `@reference "<main stylesheet>";` before `@apply`, or use `var(--…)` directly |
