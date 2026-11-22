---
type: llm
---

PASS if the answer is a routing in JSON whose questions ask the user what "better onboarding" means (which part of the onboarding, which complaint, or what outcome they expect), each question with a proposed answer, rather than deciding a scope on its own.
FAIL if it routes a concrete feature it made up (a new tour, a checklist, emails…) with no question, or if it writes code or a plan of tasks.
