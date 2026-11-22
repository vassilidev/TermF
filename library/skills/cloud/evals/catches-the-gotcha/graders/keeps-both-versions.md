---
type: regex
pattern: '"aws-cdk"\s*:\s*"[\^~]?2\.2[0-9]{2}\.|"aws-cdk-lib"\s*:\s*"[\^~]?2\.1[0-9]{3}\.'
match: not_contains
target: last_message
---
