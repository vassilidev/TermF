---
type: regex
target: last_message
pattern: '```diff|^[-+]Subject|^\+{3} |^-{3} a/'
flags: m
match: not_contains
---
