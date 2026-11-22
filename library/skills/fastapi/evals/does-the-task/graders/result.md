---
type: llm
---

PASS if the endpoint uses FastAPI's built-in SSE support (EventSourceResponse or ServerSentEvent imported from fastapi.sse, with response_class=EventSourceResponse and an async generator that yields) and adds no third-party SSE package.
FAIL if it adds sse-starlette or another SSE dependency, or builds the event-stream format by hand in a StreamingResponse.
