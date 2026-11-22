---
type: llm
---

PASS if the command edits in place in a form both BSD and GNU sed accept (a suffix attached to `-i`, such as `-i.bak`, with the backups removed; or a temporary file and `mv`; or `perl -pi -e`), deals with the slashes of the URLs (another delimiter such as `|`, or escaped slashes), escapes the dots or accepts matching any character there knowingly, and restricts the change to the Markdown files under `docs/`.
FAIL if it uses `sed -i` with no suffix, `sed -i ''`, or a check of `uname` that picks one of the two forms.
