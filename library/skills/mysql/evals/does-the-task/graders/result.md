---
type: llm
---

PASS if the column is added with ALGORITHM=INSTANT, the index with ALGORITHM=INPLACE and LOCK=NONE, both clauses are written explicitly, and the two changes are separate statements (an INSTANT operation cannot share a statement with a non-INSTANT one).
FAIL if no ALGORITHM or LOCK clause is given, if ALGORITHM=COPY is used, or if both changes are combined in one ALTER TABLE that claims to be INSTANT.
