---
type: llm
---

PASS if the tests use Swift Testing (import Testing, @Test, #expect), cover the boundary the criterion sets (a value just below 100, exactly 100, and one above), preferably as one parameterized @Test(arguments:), and state expected totals as literal values from the criterion (for example 100 -> 90), not recomputed with the same formula.
FAIL if the tests use XCTest for a new empty target, skip the 100 boundary, or compute the expected value with the production formula.
