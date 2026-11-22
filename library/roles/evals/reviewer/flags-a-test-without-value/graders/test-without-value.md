---
type: llm
---

PASS if an important finding in src/discounts.test.ts says the test mocks findDiscount itself, so it never runs the lookup and would pass without the change (AC1 is not proven), and asks for a test through the real lookup (a stored code found with a lower-case input), sent to T1.
FAIL if the test is accepted as covering AC1, or the problem is only a nit.
