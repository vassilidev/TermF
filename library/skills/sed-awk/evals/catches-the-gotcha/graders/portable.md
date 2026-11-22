---
type: llm
---

PASS if the fixed command runs unchanged on BSD and GNU sed: the in-place edit uses a suffix attached to `-i` (with the backups removed), a temporary file, or `perl -pi -e`; and with sed, the whitespace is matched with a POSIX class such as `[[:space:]]` or `[[:blank:]]` and `+` under `-E` (or `*` in a basic expression), never `\s` or `\+`.
FAIL if the answer keeps `\s` or `\+` in a sed expression, uses `sed -i ''` or a bare `sed -i`, or branches on the operating system.
