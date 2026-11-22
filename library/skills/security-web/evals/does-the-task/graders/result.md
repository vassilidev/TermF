---
type: llm
---

PASS if the review reports that any logged-in user can read another account's invoice (an IDOR / broken object-level authorization), classifies it as A01:2025 Broken Access Control, fixes it by putting the user's account in the query (or an equivalent ownership check before responding), and proposes a test where a user of a second account gets 404 or 403.
FAIL if it misses the cross-account access, only suggests adding authentication or input validation, or cites only the 2021 Top 10 numbering.
