---
description: Replacing long-lived AWS keys in CI with federation, scoped to one repository.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Our deploy job authenticates to AWS with `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` stored as repository secrets. Security wants them gone. The repository is `acme/storefront` and only the `production` environment of that repository may deploy. Give me the IAM role trust policy and the change to the workflow job.
