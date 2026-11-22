---
type: llm
focus: { source: file, path: src/auth/login.ts }
---

PASS if each failed sign-in is logged with `logger.warn` and with what lets support find it (an identifier such as the user id when there is one, the IP, the time or the reason), and neither the password nor its hash appears in the log call.
FAIL if the password, its hash or the whole request body is logged, or if nothing is logged on failure.
