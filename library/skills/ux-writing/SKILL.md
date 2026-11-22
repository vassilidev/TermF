---
name: ux-writing
description: >-
  Writes the product's words a ticket asks for, consistent with the rest: interface labels and microcopy
  (buttons, errors, empty states, confirmations, onboarding), pages and README sections, in every language the
  project ships, through its translation files, with the product's glossary and tone. Proposes options for the
  user to choose when the wording is not given, and leaves existing text alone. Use when a task asks for UI
  text, an error or empty-state message, a translation, a glossary, a tone of voice or a README section. Not for
  the layout or the code around the text (front), nor for rewording text no ticket names.
license: MIT
metadata:
  termf-stacks: "*"
  termf-roles: "copy"
  termf-versions: "*"
  termf-verified: "2026-11-22 WCAG 2.2, CLDR plural rules, ICU MessageFormat"
  termf-sources: "https://www.w3.org/TR/WCAG22/ https://cldr.unicode.org/index/cldr-spec/plural-rules https://unicode-org.github.io/icu/userguide/format_parse/messages/"
---
# UX writing

1. **What the ticket asks.** List each text to write: where it shows (screen, state, component), what it must say, and whether the ticket gives the words, asks you to write them, or leaves them open. Anything else stays as it is. Done when every text has its place and its source in the ticket.
2. **Read the product's words.** Open the translation files (`i18n/`, `locales/`, `messages/`, `*.po`, `*.xliff`, `Localizable.strings`, `strings.xml`, the i18n module of the app), the neighbouring screens and the README. Note the terms used for the same things, the form of address (you, tu or vous, du or Sie), capitalisation of labels (sentence or title case), punctuation, how numbers, dates and units are written, and the glossary if there is one. Done when you can name the product's word for each thing your texts mention.
3. **Draft.** Write each text to the rules below and the product's habits from step 2, in the language of the ticket first.
   - A button names its action with a verb and the object when it is not obvious ("Export invoices", not "OK" or "Submit"); the same action keeps the same word everywhere.
   - An error says what happened, why when it helps, and what to do next, without blame and without code words ("The file is over 10 MB. Choose a smaller file." rather than "Error 413"). It names the field it is about.
   - An empty state says what will appear here and how to make the first one appear, with the button's own label.
   - A confirmation of something destructive names what will be lost and repeats the action on its button ("Delete 3 invoices"); the safe choice is the other button.
   - Plain words, short sentences, the user's vocabulary rather than the team's; no jargon, no exclamation marks in errors, no "please" padding.
   Done when every text has one draft.
4. **Options or text.** When the ticket gives the words, use them as given. When it asks for wording without fixing it, report `blocked` with a choice question per text (or one question for a set): two or three options that differ in a way that matters (tone, length, term), your pick as the proposed answer, and why. Write the chosen ones once the user answers. Done when every text written is given by the ticket or chosen by the user.
5. **Every language.** Add each text to every language the project ships, in the same change, with the same key. Keep placeholders (`{count}`, `%s`, `{{name}}`), plural forms (`one`, `other`, and the extra CLDR categories some languages need), gender selects and markup exactly as the file's format writes them. A language you cannot write well is a question, not a machine guess. Done when every locale file has the key and parses.
6. **Fit.** Compare the length with the neighbouring texts and the space on screen; German and French usually run longer than English, short labels most of all: give the layout's longest language a version that fits, or say where it will wrap. Done when each text fits, or the overflow is reported with the screen.
7. **Glossary and tone** when the task asks for them: a table of terms (use, avoid, definition, example) and three to five tone rules with a do and a don't each, in the file the ticket names (`docs/voice.md`, `GLOSSARY.md`…). Done when every term in your new texts is in it.
8. **Report.** Each text in `text_changes` with the criterion that asks for it; the screens in `affected_views`; existing texts you would improve in your out-of-scope findings, never in the files. Done when the report lists every text you wrote.

## Gotchas

- Rewording an existing label "while you are there" is a text change nobody asked for: TermF's rule `no-text-changes` blocks it, and the user loses a decision that was theirs.
- English plural logic (`count === 1 ? … : …`) breaks other languages: Polish, Russian, Arabic and others have more plural categories. Use the plural syntax of the project's i18n library (ICU `{count, plural, one {…} other {…}}` and friends).
- Concatenated fragments ("Delete" + " " + item) cannot be translated: word order changes across languages. Write whole sentences with placeholders.
- A placeholder renamed or dropped in one locale breaks that locale at runtime, often silently: keep every placeholder in every translation.
- An error that only shows in colour or an icon is lost to screen readers and colour-blind users: the text says it (WCAG 2.2: 1.4.1, 3.3.1, 3.3.3).
- Title Case in one screen and sentence case in the next reads as two products: follow the product's habit from step 2.

## Verify

- Read each locale file back after your edit: it is still valid JSON, YAML, PO… (brackets, commas, quotes), and each new key appears once, in every file.
- Placeholders: the same set in every translation of a key.
- Name in your report the project's i18n check (a `lint:i18n`, `i18n:check` or `extract` script, the typecheck of a typed i18n module) for the next step to run: the copy role works without a shell.

## Security

- Error messages say what the user can act on, never stack traces, internal paths, SQL, or whether an account exists on a login or reset form (say the same thing in both cases).
- Text shown from user content goes through the app's escaping: a translation never builds HTML by concatenation with user input.
- No real names, emails or customer data in examples and placeholders: use obviously fictional ones.

## References

- WCAG 2.2, criteria 1.4.1 (use of colour), 3.3.1 (error identification), 3.3.3 (error suggestion): https://www.w3.org/TR/WCAG22/
- CLDR plural rules: https://cldr.unicode.org/index/cldr-spec/plural-rules
- ICU MessageFormat: https://unicode-org.github.io/icu/userguide/format_parse/messages/
