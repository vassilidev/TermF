---
description: A first CI workflow for a pnpm project, which should come out least-privilege and pinned.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Write a GitHub Actions workflow for our Node project: it uses pnpm (lockfile is pnpm-lock.yaml, Node 24), and should run `pnpm lint` and `pnpm test` on every pull request and on pushes to main. Reply with the full `.github/workflows/ci.yml`.
