---
type: llm
---

PASS if the tests follow the three criteria with literal expected values (4.90 below 50, 0 at exactly 50 and above, ValueError for a negative total via pytest.raises), cover the 50 boundary from both sides, and pass any `ids` or `indirect` to parametrize as keyword arguments.
FAIL if the expected values are computed with the same condition as the code, the 50 boundary is missing, or the test only checks that the function returns a Decimal.
