---
type: llm
---

PASS if the server is started on `$PORT` (for example `npm run dev -- --port "$PORT"`) as a background task or process whose id is kept, readiness is awaited with retries rather than a fixed sleep, the check fails on a non-200 status, and the cleanup stops that task or PID only.
FAIL if it relies on Vite's default port, stops the server with `pkill`, `killall` or by process name, or kills whatever holds the port without checking it is the server it started.
