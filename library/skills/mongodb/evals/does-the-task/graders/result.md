---
type: llm
---

PASS if the proposed compound index puts the equality fields (accountId, status) first, then the sort field (total), then the range field (createdAt), and the answer checks the result with explain("executionStats"), looking for an IXSCAN and totalDocsExamined close to nReturned.
FAIL if the index puts createdAt before total, or if the answer gives no way to check the plan.
