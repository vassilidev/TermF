---
type: regex
target: { source: file, path: web/src/invoices/List.svelte }
pattern: '>\s*(Export CSV|Exporter en CSV)\s*<'
match: not_contains
---
