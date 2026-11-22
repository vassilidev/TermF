---
type: llm
---

PASS if the review is JSON with no important finding (an empty list, or nits only), and its summary says what was checked (the boundary at 5000 cents, both criteria, the tests going through createOrder) and what was not.
FAIL if it reports an important finding, or invents a failure it cannot make concrete (style, naming, the constant's name, a test it would have written differently).
