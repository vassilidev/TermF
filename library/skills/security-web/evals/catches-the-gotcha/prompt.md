---
description: Classifying an SSRF finding; the outdated answer uses the 2021 category A10.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

For our pentest report: our thumbnail service downloads whatever URL the user submits, and we confirmed it can reach http://169.254.169.254/ from inside the cluster. Which OWASP Top 10 category (current edition, with its identifier) should we file this under, and what is the fix?
