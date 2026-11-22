---
type: llm
---

PASS if the webhook route reads the raw body (registered before, or apart from, the global express.json()), verifies the Stripe-Signature header with the Stripe library and a signing secret read from the environment, refuses a bad signature with a 4xx, skips an event id it already processed, answers 2xx quickly, and tests locally in a sandbox with the Stripe CLI (stripe listen forwarding to the route, stripe trigger).
FAIL if the route parses JSON before verifying, skips signature verification, hard-codes a key or a whsec_ secret, or would process a redelivered event twice.
