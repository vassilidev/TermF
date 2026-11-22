---
type: llm
---

PASS if the answer says ORJSONResponse is deprecated and that declaring return types or response_model lets Pydantic serialize the JSON (in Rust), and removes default_response_class=ORJSONResponse.
FAIL if it recommends keeping ORJSONResponse or switching to UJSONResponse for speed.
