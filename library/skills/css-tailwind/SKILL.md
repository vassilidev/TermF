---
name: css-tailwind
description: >-
  Styles web UI with the project's own CSS setup: Tailwind CSS v3 or v4 (detected, never mixed),
  the project's design tokens, and modern CSS for focus, reduced motion, logical properties and
  container queries. Use when a task changes classes, stylesheets, themes or tokens (*.css,
  tailwind.config.*, @theme, class attributes in components) or upgrades Tailwind. Not for component
  logic or state, and not for a full accessibility review.
paths: "**/*.css, **/*.scss, **/tailwind.config.*, **/postcss.config.*, **/*.html, **/*.tsx, **/*.jsx, **/*.vue, **/*.svelte"
license: MIT
metadata:
  termf-stacks: "tailwind web"
  termf-roles: "front ux"
  termf-versions: ">=3.4"
  termf-verified: "2026-11-22 tailwindcss@4.3.5"
  termf-sources: "https://tailwindcss.com/docs/upgrade-guide https://tailwindcss.com/docs/detecting-classes-in-source-files https://tailwindcss.com/docs/dark-mode https://tailwindcss.com/docs/responsive-design https://github.com/tailwindlabs/tailwindcss/releases"
---
# CSS and Tailwind

1. Read the installed version in the lockfile entry for `tailwindcss`, and keep it as the reference for every later step. Confirm it in the entry stylesheet: `@import "tailwindcss";` and `@theme` mean v4; `@tailwind` directives and a `tailwind.config.*` with `content` mean v3. Without Tailwind, follow what the project uses (plain CSS, CSS modules, a CSS-in-JS library). Done when you can name the version and the file it came from.
2. Open the docs of that version before writing code: Tailwind publishes nothing for agents (no `llms.txt`, MCP server or official skill on 2026-11-22), so use the pages under `https://tailwindcss.com/docs/` (they document v4; v3 lives at `https://v3.tailwindcss.com/docs/`) and, for a v3 project or an upgrade, `references/v3-to-v4.md`. Done when the page for the task is open.
3. Use the tools the project already has (build plugin, Stylelint or Prettier plugin, design system, class-merging helper), because a second tool splits the project. Done when every command you plan to run comes from the project.
4. Find the tokens and variants: v4 `@theme` variables (`--color-*`, `--font-*`, `--radius-*`, `--breakpoint-*`, `--container-*`), v3 `theme.extend`, `:root` custom properties in plain CSS, and the variant maps of the design system's components (`cva`, `tv`, a `variants` object). Done when every color, size, radius and font in your change maps to one of them.
5. Write the styles with those tokens. An arbitrary value (`p-[13px]`, `text-[#3b82f6]`) only when no token fits, and say so in your report; adding a token is a design decision to raise, not to make silently. Extend the component's variant map rather than concatenating conditional classes. Dark mode follows the project's strategy (v4 follows the system preference unless a `@custom-variant dark` declares a class or `data-` toggle; v3 uses `darkMode` in the config). Done when no new hard-coded value duplicates a token and no v3 class appears in a v4 project, or the reverse.
6. Keep the page usable: every interactive element keeps a visible keyboard focus (`focus-visible:` with a ring or outline, never a removed outline without replacement); animations and transitions sit behind `motion-safe:` or a `prefers-reduced-motion: reduce` block; logical sides (`ms-*`, `me-*`, `ps-*`, `pe-*`, `inset-s-*`, `text-start`) where the app supports right-to-left text or already uses them; a container query (`@container` on the parent, `@sm:` or `@max-md:` on the child in v4; in v3 only with the `@tailwindcss/container-queries` plugin installed) when a component's layout depends on its own width. Done when the states named in the task (hover, focus, disabled, error, empty) render as intended at the project's breakpoints, in light and dark themes; a TermF run with visual review shows them before and after.
7. Run the commands of `## Verify` and put each one with its result in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- v4 shifted the size scale down one step: v3 `shadow-sm`, `shadow`, `rounded-sm`, `rounded`, `blur-sm`, `blur` are v4 `shadow-xs`, `shadow-sm`, `rounded-xs`, `rounded-sm`, `blur-xs`, `blur-sm` (same for `drop-shadow` and `backdrop-blur`). `outline-none` now removes the outline for real; the old invisible outline is `outline-hidden`. A bare `ring` is 1px in `currentColor`: write `ring-3` and a color for the v3 look.
- v4 removed `bg-opacity-*`, `text-opacity-*` and the other `*-opacity-*` classes: use the slash modifier, `bg-black/50`.
- v4 defaults changed: borders and dividers default to `currentColor` (always give a color), placeholders use the text color at half opacity, buttons get `cursor: default`, `hover:` only applies on devices that can hover, and the `hidden` attribute wins over `flex` or `block`.
- v4 tokens are CSS variables in `@theme`, custom utilities use `@utility` (not `@layer utilities`), and a `tailwind.config.js` is ignored unless a stylesheet loads it with `@config`. The build runs through `@tailwindcss/vite`, `@tailwindcss/postcss` or `@tailwindcss/cli`.
- v4 syntax: a CSS variable as an arbitrary value is `bg-(--brand)` (v3: `bg-[--brand]`); the important mark goes last (`flex!`); stacked variants read left to right (`*:first:pt-0`); a prefix comes first (`tw:flex`).
- v4 `@apply` inside a Vue or Svelte `<style>` block or a CSS module only sees the theme after `@reference "<path to the main stylesheet>";`. Sass, Less and Stylus are not supported with v4.
- v4 scans every file git does not ignore. Classes used inside a dependency (a shared UI package in `node_modules`) need `@source "<path>"`; classes that never appear literally need `@source inline("…")`. Class names built by concatenation (`bg-${color}-500`) are never generated in any version: map each case to a full class string.
- 4.2 deprecated `start-*` and `end-*` in favour of `inset-s-*` and `inset-e-*`.
- v4 needs Safari 16.4, Chrome 111, Firefox 128 or later. A project that supports older browsers stays on v3.4, and an upgrade is never a side effect of another task.

## Verify

- The project's own scripts from `package.json`: `build`, `lint`, and its Stylelint or Prettier check if present. In v4 an unknown class in `@apply` fails the build, so build after touching stylesheets.
- Upgrades, only when the ticket asks: `npx @tailwindcss/upgrade` (Node.js 20+), on its own branch, with the diff reviewed.

## Security

- User input never goes into class names or `style` values without an allowlist (a user color is validated as a hex value first): CSS injection can overlay or leak content.
- Stylesheets, fonts and images loaded from third-party hosts leak visitors' addresses and need CSP entries: serve them the way the project already serves assets.
- Inline `style` attributes need `style-src 'unsafe-inline'` under a strict CSP: prefer classes.
- CSS hiding (`hidden`, `sr-only`, `display: none`) does not protect data: content the user may not see is not rendered at all.

## References

- Read `references/v3-to-v4.md` when the project is on v3, when a v3 config meets a v4 install, or when the task is an upgrade.
- Tailwind (facts restated, not copied): `https://tailwindcss.com/docs/upgrade-guide`, `/docs/theme`, `/docs/detecting-classes-in-source-files`, `/docs/dark-mode`, `/docs/responsive-design`; release notes `https://github.com/tailwindlabs/tailwindcss/releases`.
- CSS: the MDN pages for `:focus-visible`, `prefers-reduced-motion`, container queries and logical properties.
