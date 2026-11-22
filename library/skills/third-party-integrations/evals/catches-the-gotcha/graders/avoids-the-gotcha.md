---
type: llm
---

PASS if the answer says to reuse the same idempotency key (with the same parameters) for every attempt of one operation, explains that a new key per attempt can create a second payment after a timeout, retries only timeouts, connection errors, 429 and 5xx with backoff and jitter, and does not retry a card decline or another 4xx.
FAIL if it keeps a new key per attempt, or keeps retrying card declines.
