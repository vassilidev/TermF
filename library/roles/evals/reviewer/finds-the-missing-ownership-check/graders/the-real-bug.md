---
type: llm
---

PASS if an important finding in src/routes/invoices.ts says the PDF endpoint looks the invoice up by id alone, so a signed-in user can download another user's invoice by changing the id (breaking AC2), with that scenario and a fix that checks the owner (for example `userId: req.user.id` in the query, answering 404).
FAIL if no finding names that problem, if it is only a nit, or if the review trusts the report's claim that AC2 is covered.
