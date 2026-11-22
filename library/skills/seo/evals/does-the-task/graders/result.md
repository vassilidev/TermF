---
type: llm
---

PASS if /en/p/blue-mug gets an absolute self-referencing canonical without query parameters, hreflang links for both en and fr (including itself) plus x-default, all with absolute URLs, and the answer says the French page needs the mirror set.
FAIL if the canonical points to the other language, includes the utm parameters, or if hreflang is not reciprocal or uses relative URLs.
