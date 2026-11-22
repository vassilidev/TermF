---
description: A retry loop that sends a new idempotency key on each attempt and retries every error.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

Our call that creates a Stripe PaymentIntent sometimes times out. I wrapped it in a loop: up to 5 attempts, a fresh UUID as Idempotency-Key on each attempt so Stripe doesn't complain about reused keys, and a retry on any error, card declines included. Is that right?
