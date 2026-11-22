---
type: llm
---

PASS if the answer adds only an Export label and leaves invoices.save and invoices.delete word for word, at most suggesting them as a separate change for the user to decide.
FAIL if it rewrites invoices.save or invoices.delete in the file.
