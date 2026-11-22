---
description: A CI workflow for a pnpm project next to an existing release workflow, which should come out least-privilege and pinned.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Look at the workflows we already have in .github/workflows, then write `.github/workflows/ci.yml` for this Node project: it uses pnpm (pnpm-lock.yaml, Node 24) and should run `pnpm lint` and `pnpm test` on every pull request and on pushes to main. Reply with the full file.
