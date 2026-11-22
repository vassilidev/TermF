---
description: A Terraform question, which the terraform skill covers.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

Our CI runs `terraform validate` but it fails at `terraform init` because the runner has no access to the S3 backend. How do I validate the configuration without backend credentials?
