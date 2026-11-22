---
description: A Docker build question, which the docker skill covers.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

`docker build .` fails with `"/config/app.toml": not found` on the line `COPY config/app.toml /etc/app/`, but the file is right there in the repo. Our .dockerignore has `*.toml` and `!pyproject.toml`. What is going on and how do I fix it?
