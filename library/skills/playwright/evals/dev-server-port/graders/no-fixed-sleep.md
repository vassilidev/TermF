---
type: regex
target: last_message
match: not_contains
pattern: '\.waitForTimeout\(|expect\(\s*await\s[^\n]{0,160}?\.isVisible\(\)'
---
