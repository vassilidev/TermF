---
description: Slim down and harden a Node API image whose build reinstalls packages on every change.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Our API image is 1.3 GB and every code change reinstalls all npm packages during the build. Can you rewrite this Dockerfile for production? Reply with the new Dockerfile and anything else I should add to the repo.

```dockerfile
FROM node:24
WORKDIR /app
COPY . .
RUN npm ci
RUN npm run build
EXPOSE 3000
CMD ["node", "dist/server.js"]
```
