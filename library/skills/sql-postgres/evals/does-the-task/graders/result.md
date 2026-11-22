---
type: llm
---

PASS if the answer sets a lock_timeout, adds the column without a volatile default, creates the index with CREATE INDEX CONCURRENTLY outside a transaction block, adds the foreign key as NOT VALID and validates it in a separate step, and says which statements cannot run inside the migration tool's transaction.
FAIL if it adds the foreign key in one validating statement, builds the index without CONCURRENTLY, or wraps CREATE INDEX CONCURRENTLY in BEGIN/COMMIT.
