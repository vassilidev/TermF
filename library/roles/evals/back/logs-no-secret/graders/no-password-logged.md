---
type: regex
target: { source: file, path: src/auth/login.ts }
pattern: 'logger\.\w+\([^;]*([{,(]\s*password\s*[,})]|:\s*password\b|\$\{password\}|\+\s*password\b)'
match: not_contains
---
