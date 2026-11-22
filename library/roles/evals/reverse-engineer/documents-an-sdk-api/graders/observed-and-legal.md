---
type: llm
---

PASS if the report states the legal ground first (a customer integrating with the service, the licence allowing study for integration), then describes both calls (GET /v1/shipments with `limit` and `starting_after`, POST /v1/shipments with its JSON body and the optional `Idempotency-Key`), the base URL and the headers (`Authorization: Bearer`, `Parcelo-Version: 2024-06-01`), the 429 and error handling, each point with where it was seen in the SDK, and marks what is inferred rather than observed.
FAIL if it invents an endpoint, a field or a header the SDK does not show, or leaves out the legal ground.
