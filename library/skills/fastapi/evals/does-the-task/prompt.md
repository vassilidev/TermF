---
description: A streaming endpoint with Server-Sent Events in a current FastAPI app.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Our FastAPI app (fastapi 0.165, Python 3.13) runs long imports. Add an endpoint GET /imports/{import_id}/progress that pushes the progress percentage to the browser as Server-Sent Events until it reaches 100. Progress comes from `async def read_progress(import_id: str) -> int` in acme/imports.py. Show the code and any dependency you add.
