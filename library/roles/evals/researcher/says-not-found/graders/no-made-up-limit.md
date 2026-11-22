---
type: regex
target: last_message
pattern: '\d[\d,.]*\s*(requests?|calls?|req)\s*(per|/|an?)\s*(second|minute|hour|day|sec|min|s|m|h|d)\b'
match: not_contains
---
