---
type: llm
---

PASS if the answer explains that set_var and remove_var are unsafe in edition 2024 because changing the environment races with other threads, and either removes the need for the environment (passing the level as a parameter or building the logger directly) or wraps the calls in `unsafe` with a SAFETY comment while making sure the tests that use it cannot run concurrently.
FAIL if it only wraps the calls in `unsafe {}` with no reasoning about the parallel tests, or downgrades the edition.
