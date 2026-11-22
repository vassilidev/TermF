---
name: accessibility
description: >-
  Makes web interfaces work with a keyboard and assistive technologies and checks them against
  WCAG 2.2 level AA by criterion number: native HTML first, ARIA only as an APG pattern prescribes,
  focus management for dialogs, menus and route changes, contrast from the design tokens, axe in the
  tests plus the manual checks to report. Use when a task builds or changes interactive UI (forms,
  dialogs, menus, tabs, comboboxes, drag and drop, custom widgets), fixes an accessibility bug or
  audit finding, or asks for a11y or WCAG conformance. Not for SEO metadata (seo), nor for visual
  design choices with no interaction or content impact (ux).
paths: "**/*.html, **/*.tsx, **/*.jsx, **/*.vue, **/*.svelte, **/*.astro, **/*.css, **/*.scss"
license: MIT
metadata:
  termf-stacks: "web"
  termf-roles: "front ux qa"
  termf-versions: "*"
  termf-verified: "2026-11-22 WCAG 2.2 (W3C Recommendation 2023-10-05), WAI-ARIA 1.2, axe-core@4.14.1, @axe-core/playwright@4.14.0"
  termf-sources: "https://www.w3.org/WAI/standards-guidelines/wcag/new-in-22/ https://www.w3.org/WAI/WCAG22/quickref/ https://www.w3.org/WAI/ARIA/apg/ https://www.w3.org/TR/wai-aria-1.2/ https://playwright.dev/docs/accessibility-testing https://github.com/dequelabs/axe-core/blob/develop/doc/API.md"
---
# Accessibility

The target is WCAG 2.2 level AA, cited by criterion number (for example 2.4.11) in code reviews and reports; read the criterion on the W3C quick reference rather than quoting it from memory. There is no official agent resource from W3C. When the session has the Chrome DevTools MCP server, its `a11y-debugging` skill (ChromeDevTools/chrome-devtools-mcp, Apache-2.0) runs Lighthouse and reads the accessibility tree of the running page.

## Gotchas
- WCAG 2.2 removed 4.1.1 Parsing; do not cite it. Its new A and AA criteria are the ones most often missed: 2.4.11 Focus Not Obscured (Minimum), 2.5.7 Dragging Movements, 2.5.8 Target Size (Minimum), 3.2.6 Consistent Help, 3.3.7 Redundant Entry, 3.3.8 Accessible Authentication (Minimum). 2.4.13 Focus Appearance is AAA, not part of an AA target.
- 2.5.8: pointer targets are at least 24 by 24 CSS px, or spaced so a 24 px circle centred on each does not overlap another; inline links in a sentence are exempt. Icon buttons in toolbars and table rows are the usual failures.
- 2.5.7: every drag (sortable list, kanban card, slider thumb, map pan) also works with single clicks or taps, for example "Move up / Move down" buttons or a "Move to…" menu.
- 2.4.11: a sticky header, footer or cookie banner that fully covers the focused element fails. Set `scroll-padding-top` (or `-bottom`) to the sticky element's height so focused elements scroll into view below it.
- 3.3.8: a login or one-time-code step allows paste and password managers (`autocomplete="username"`, `"current-password"`, `"one-time-code"`) and has no cognitive test (puzzle, transcription) without an alternative.
- axe-core 4.14 ships its `target-size` rule (2.5.8) disabled by default, and automated tools find only part of WCAG. Keyboard order, focus return, 2.4.11, 2.5.7, 2.5.8 and 3.3.7 stay manual checks to list in the report.
- `role="menu"` promises application-menu keyboard behaviour (arrows, typeahead, Tab leaves the menu). Site navigation stays links in `<nav>`, with a disclosure button (`aria-expanded`) for sub-lists, as in APG's disclosure navigation example.
- `aria-label` on a plain `div` or `span` is not reliably announced: ARIA 1.2 prohibits naming the `generic` role (and `paragraph`, `code`, `strong`…). Name a real control or landmark instead.
- 2.5.3 Label in Name: the accessible name contains the visible label, best at its start. A button showing "Cancel" with `aria-label="Close dialog"` fails for voice-control users.
- `aria-hidden="true"` on an element that is or contains something focusable leaves a keyboard stop that screen readers cannot see. Take a region out of both with the `inert` attribute.
- A live region announces changes only if it is already in the DOM when its content changes. Render the empty `role="status"` (or `role="alert"` for errors) container up front and write the message into it.
- `outline: none` without a replacement fails 2.4.7. Style `:focus-visible` with an indicator that keeps 3:1 against its surroundings (1.4.11).

## Procedure
1. **Read what the project already has.** The design system components (they may implement the patterns already: reuse them), the colour tokens and themes, the a11y lint rules (`eslint-plugin-jsx-a11y`, `eslint-plugin-vuejs-accessibility`, angular-eslint template rules, Svelte compiler a11y warnings) and the tests that use axe (`@axe-core/playwright`, `jest-axe`, `vitest-axe`). Done when you know which component to reuse and which checks already run.
2. **Use the native element.** `<button>` for actions, `<a href>` for navigation, `<label for>` on every field, `<fieldset>` + `<legend>` for groups, `<details>`/`<summary>` for show/hide, `<dialog>` opened with `showModal()` for modal dialogs, `<table>` with `<th scope>` for data. Done when every interactive element is native, or step 3 applies.
3. **Follow the APG pattern for a custom widget.** Read `references/apg-patterns.md` for the widget, then the linked APG page. Implement its roles, its states kept in sync (`aria-expanded`, `aria-selected`, `aria-checked`), every key it lists, and its focus rules. Done when each key in the pattern's table does what it says, tried by hand.
4. **Name and structure.** Every control has an accessible name that contains its visible label (4.1.2, 2.5.3). `<html lang>` is set (3.1.1). Headings follow the page outline and landmarks (`header`, `nav`, `main`, `footer`) are present once each where they apply (1.3.1, 2.4.6). Informative images get a short `alt`, decorative ones `alt=""` (1.1.1). Field errors are text next to the field, linked with `aria-describedby`, and `aria-invalid="true"` is set while the error shows (3.3.1, 3.3.3). Done when the accessibility tree (browser devtools or a Playwright aria snapshot) reads like the visual page.
5. **Manage focus.** Tab order follows reading order (2.4.3); no positive `tabindex`. Dialogs: focus moves inside on open, stays inside, Escape closes, focus returns to the opener on close. After a client-side route change, move focus to the new page's `<h1>` (made focusable with `tabindex="-1"`) and update `document.title` (2.4.2). Nothing traps the keyboard (2.1.2). Done when a full pass with Tab, Shift+Tab, Enter, Space, Escape and the arrows completes the flow without a mouse.
6. **Check visual rules against the tokens.** Text contrast 4.5:1, or 3:1 for large text (at least 24 px regular or about 18.5 px bold) (1.4.3); component boundaries, states and focus indicators 3:1 (1.4.11), in every theme the project ships. Content reflows without horizontal scrolling at 320 CSS px wide (1.4.10) and survives 200 % zoom (1.4.4). Information is never carried by colour alone (1.4.1). Done when each new colour pair is a token pair you checked, not a one-off value.
7. **Test.** Add an axe scan of the changed page or state with the tool already installed (with Playwright: `new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa']).analyze()` and `expect(results.violations).toEqual([])`; skill `playwright`). Write the keyboard flow as a test with real key presses and `toBeFocused()` assertions where the project has browser tests (skill `tests-from-criteria` for what to cover). Done when both run green and were seen failing on the broken markup.
8. **Report.** List the criteria you checked by number with how (axe, keyboard test, manual check), the manual checks you did (keyboard pass, 200 % zoom, 320 px width, target sizes), and what you could not check (no screen reader run, untested browser).

## Verify
- The project's lint and test scripts from `package.json` (they carry the a11y lint rules and axe tests when the project has them).
- Browser tests: the project's e2e script, else `PLAYWRIGHT_HTML_OPEN=never npx playwright test` on the specs you touched.
- Manual, every time: keyboard-only pass through the changed flow; 200 % browser zoom; 320 px wide viewport; target sizes of new controls; focus never hidden by sticky elements.

## Security
- Accessible names, `title`, `alt` and live-region text are read by assistive technology and by any script or extension that walks the accessibility tree. Keep secrets, tokens and full card or ID numbers out of them, as you would out of visible text.
- Allowing paste and password managers on login fields (3.3.8) is also the safer choice: users keep long unique passwords. Leave `autocomplete` on for credentials.
- Replacing a cognitive CAPTCHA keeps the bot protection: offer a non-cognitive alternative (passkey, email link, invisible challenge) rather than removing the check.
- Content hidden with CSS or `aria-hidden` is still in the HTML. Data a user may not see is not rendered at all for that user; hiding is not access control.
- Third-party "accessibility overlay" scripts inject remote code into every page and do not make the markup conform. Fix the markup instead of adding one.

## References
- Read `references/apg-patterns.md` when building or fixing a custom widget: dialog, alert dialog, disclosure, menu button and menu, tabs, accordion, combobox, listbox, radio group, switch, tooltip.
- WCAG 2.2 [quick reference](https://www.w3.org/WAI/WCAG22/quickref/) and [what's new in 2.2](https://www.w3.org/WAI/standards-guidelines/wcag/new-in-22/); [ARIA Authoring Practices Guide](https://www.w3.org/WAI/ARIA/apg/) and its [read me first](https://www.w3.org/WAI/ARIA/apg/practices/read-me-first/); [Playwright accessibility testing](https://playwright.dev/docs/accessibility-testing); [axe-core API](https://github.com/dequelabs/axe-core/blob/develop/doc/API.md).
