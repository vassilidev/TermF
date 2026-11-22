---
description: New HTTP routing on a fresh cluster; the outdated answer reaches for ingress-nginx annotations.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

We are setting up routing on a brand new Kubernetes 1.37 cluster with nothing installed yet. We need https://api.acme.example/v1/... to reach the `api` Service on port 8080 with the `/v1` prefix stripped. What should we install and which manifests do we write?
