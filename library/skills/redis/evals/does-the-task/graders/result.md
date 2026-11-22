---
type: llm
---

PASS if cached values are written with an expiry in the same command (for example `set(key, value, 'EX', seconds)`), keys use the `shop:` prefix, and the bulk invalidation iterates with SCAN (or scanStream) or uses a version/namespace key, rather than KEYS.
FAIL if any key is written without a TTL, or if invalidation uses KEYS or FLUSHALL/FLUSHDB.
