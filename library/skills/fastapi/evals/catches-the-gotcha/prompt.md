---
description: Response class choice after FastAPI moved JSON serialization to Pydantic.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

To speed up JSON we set `app = FastAPI(default_response_class=ORJSONResponse)` years ago. We are now on fastapi 0.165. Is this still the fastest setup, and what should the app look like?
