---
description: Slim down and harden a Node API image whose build reinstalls packages on every change.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Our API image is 1.3 GB and every code change reinstalls all npm packages during the build. Can you rewrite the Dockerfile in this repository for production? Reply with the new Dockerfile and anything else I should add to the repo.
