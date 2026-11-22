---
type: llm
---

PASS if the Dockerfile uses at least two stages, copies package.json and the lockfile before the rest of the source so the install layer stays cached, ends with a slim or distroless runtime stage that runs as a non-root USER, and the answer asks for a .dockerignore that excludes node_modules, .git and .env files.
FAIL if it keeps a single stage, still copies the whole source before installing dependencies, or leaves the container running as root.
