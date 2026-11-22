---
type: regex
pattern: '(^|\n)\s*allow-unsafe-pr-checkout:\s*true|uses:\s*actions/checkout@v[1-6]\b'
match: not_contains
target: last_message
---
