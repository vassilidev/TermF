# Widget patterns (ARIA Authoring Practices Guide)

Summaries written for TermF from the W3C ARIA Authoring Practices Guide (APG), read on 2026-11-22. They say what to build and test; the APG page linked under each heading is the authority, with its working examples. Prefer the native element named first when it does the job.

Contents: Dialog (modal) · Alert dialog · Disclosure and navigation · Menu button and menu · Tabs · Accordion · Combobox · Listbox · Radio group · Switch · Tooltip · Checks for every widget

## Dialog (modal)
[APG dialog-modal](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/). Native first: `<dialog>` opened with `showModal()` makes the rest of the page inert and closes on Escape; do not put `tabindex` on the `<dialog>` itself.
- Name: `aria-labelledby` pointing at the visible title (or `aria-label`). `aria-describedby` only for a short plain description.
- Focus on open: the first focusable element, or a static element at the top (`tabindex="-1"`) when the content is long or structured; for an irreversible action, the least destructive button.
- Keys: Tab and Shift+Tab cycle inside the dialog; Escape closes.
- Focus on close: back to the element that opened it; if that element is gone, to the next logical place in the flow.
- A visible close or cancel button is always present.
- Custom (non-native) dialog: `role="dialog"`, `aria-modal="true"` only when the outside is really blocked and visually obscured, all content inside the dialog element, outside made `inert`.
- Test: open with the keyboard, Tab past the last control (focus wraps), Escape, assert focus is back on the opener.

## Alert dialog
[APG alertdialog](https://www.w3.org/WAI/ARIA/apg/patterns/alertdialog/). A modal dialog that interrupts for an urgent confirmation ("Delete 3 files?"). Same rules as the dialog, with `role="alertdialog"` and `aria-describedby` on the message; initial focus on the least destructive choice.

## Disclosure and navigation
[APG disclosure](https://www.w3.org/WAI/ARIA/apg/patterns/disclosure/). Native first: `<details>`/`<summary>`.
- A `button` with `aria-expanded="true|false"`; `aria-controls` on the shown element is optional.
- Keys: Enter and Space toggle.
- Site navigation with sub-lists: links in `<nav>`, each sub-list behind a disclosure button. APG's disclosure navigation example avoids the `menu` role because site navigation does not need the menu keyboard model and screen readers would expect it.

## Menu button and menu
[APG menu-button](https://www.w3.org/WAI/ARIA/apg/patterns/menu-button/), [APG menubar](https://www.w3.org/WAI/ARIA/apg/patterns/menubar/). For application commands (an "Actions" menu, a context menu), not for navigation links.
- Button: `aria-haspopup="menu"` (or `true`), `aria-expanded` in sync; optional `aria-controls` to the menu. The popup has `role="menu"`; items are `menuitem`, `menuitemcheckbox` or `menuitemradio` (with `aria-checked`).
- Keys on the button: Enter or Space open the menu and focus the first item; Down Arrow (optional) does the same; Up Arrow (optional) focuses the last item.
- Keys in the menu: Down and Up move between items (wrapping optional); Home and End jump to first and last; Enter activates and closes; Space toggles a checkbox or radio item and keeps the menu open; a printable character jumps to the next item starting with it (optional); Escape closes and returns focus to the button; Tab leaves the menu and closes it.
- Focus inside: either the items get `tabindex="-1"` and receive focus in turn, or the menu keeps focus and points at the current item with `aria-activedescendant`.

## Tabs
[APG tabs](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/).
- `role="tablist"` named by a visible label (`aria-labelledby`) or `aria-label`; each `role="tab"` with `aria-selected` and `aria-controls` to its `role="tabpanel"`; each panel `aria-labelledby` its tab. Vertical lists set `aria-orientation="vertical"`.
- Keys: Tab enters the list on the active tab and the next Tab goes to the panel (give the panel `tabindex="0"` when its first content is not focusable). Left and Right (Up and Down when vertical) move between tabs with wrapping; Home and End optional; Enter or Space activate when activation is manual.
- Activate on focus only when the panel shows without delay; otherwise activate on Enter or Space.

## Accordion
[APG accordion](https://www.w3.org/WAI/ARIA/apg/patterns/accordion/).
- Each header is a heading (native `<h2>`…`<h6>` or `role="heading"` with `aria-level`) containing only a `button` with `aria-expanded` and `aria-controls` to its panel.
- Keys: Enter and Space toggle; Tab moves through headers and the open panels' content in page order.
- A panel that cannot be collapsed sets `aria-disabled="true"` on its button while open. Panels may be `role="region"` labelled by their button; skip the region role when many panels can be open at once (it floods the landmark list).

## Combobox
[APG combobox](https://www.w3.org/WAI/ARIA/apg/patterns/combobox/). Native first: `<select>`, or `<input list>` with `<datalist>` for simple suggestions.
- The input has `role="combobox"`, an accessible name (a `<label>`), `aria-expanded`, `aria-controls` to the popup, and `aria-autocomplete` (`none`, `list` or `both`) matching the behaviour.
- DOM focus stays in the input; the highlighted option is `aria-activedescendant`. The popup is `role="listbox"` with `role="option"` children; the current value has `aria-selected="true"`.
- Keys: typing edits the text; Down Arrow opens the popup and moves into the options; Up Arrow (optional) goes to the last option; Enter accepts the highlighted option and closes; Escape closes (and may clear when already closed); Alt+Down opens without moving (optional).
- Announce the result count through an existing `role="status"` region when the list updates.

## Listbox
[APG listbox](https://www.w3.org/WAI/ARIA/apg/patterns/listbox/). Native first: `<select>` (with `multiple` for several).
- `role="listbox"` with a name; `role="option"` children (or inside named `group`s); `aria-selected` (or `aria-checked`, not both); `aria-multiselectable="true"` for multi-select.
- Keys: focus lands on the selected option, else the first; Down and Up move; Home and End recommended beyond five options; typeahead recommended beyond seven. Multi-select: Space toggles the focused option; Shift+Down and Shift+Up extend (optional); Ctrl+A selects all (optional).
- Options are short plain text. Options holding buttons, links or checkboxes need the grid pattern instead.

## Radio group
[APG radio](https://www.w3.org/WAI/ARIA/apg/patterns/radio/). Native first: `<fieldset>` + `<legend>` with `<input type="radio">` sharing a `name`.
- Custom: `role="radiogroup"` with a name, `role="radio"` items with `aria-checked`.
- Keys: Tab enters on the checked radio (else the first) and the next Tab leaves the group; arrows move and check, wrapping at both ends; Space checks the focused radio.

## Switch
[APG switch](https://www.w3.org/WAI/ARIA/apg/patterns/switch/). Native first: `<input type="checkbox">` (its `checked` state replaces `aria-checked`).
- Custom: `role="switch"` with `aria-checked="true|false"`, named by its text, `aria-labelledby` or `aria-label`.
- Keys: Space toggles; Enter optional.
- The label stays the same in both states ("Notifications", not "Turn on" / "Turn off").

## Tooltip
[APG tooltip](https://www.w3.org/WAI/ARIA/apg/patterns/tooltip/) (still marked work in progress by the APG).
- Shown on keyboard focus and on hover of the trigger; the trigger points at it with `aria-describedby`; `role="tooltip"`; it never receives focus.
- Escape hides it; it stays open while the pointer is over the trigger or the tooltip (also WCAG 1.4.13 Content on Hover or Focus).
- Anything interactive inside means it is a non-modal dialog, not a tooltip. Never put the only label of an icon button in a tooltip: give the button its own accessible name.

## Checks for every widget
- Every key the pattern lists works, tried by hand and, where the project has browser tests, asserted with real key presses and `toBeFocused()`.
- Roles and states in the accessibility tree match what is on screen after each interaction (`aria-expanded`, `aria-selected`, `aria-checked`).
- Focus is visible on every stop (2.4.7), never hidden under sticky content (2.4.11), and each target is at least 24 by 24 CSS px or spaced (2.5.8).
- APG's read-me-first page applies throughout: ARIA changes only what assistive technology is told, never the behaviour, so every role added is a promise of the keyboard behaviour above.
