---
type: llm
---

PASS if the manifest pins the image to a version (not `latest`), sets CPU and memory requests, adds a readiness probe on /healthz/ready, sets runAsNonRoot, allowPrivilegeEscalation: false, drops ALL capabilities and uses the RuntimeDefault seccomp profile, moves the database password out of the manifest into a Secret reference, and does not make the liveness probe depend on Postgres.
FAIL if the image stays on `latest`, the password stays inline, there is no securityContext, or a liveness probe checks the database.
