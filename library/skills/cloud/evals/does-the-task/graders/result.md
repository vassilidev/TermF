---
type: llm
---

PASS if the answer uses GitHub OIDC: a role trusting `token.actions.githubusercontent.com` with a condition on `aud` = `sts.amazonaws.com` and on `sub` = `repo:acme/storefront:environment:production`, and a job with `permissions: id-token: write` that calls aws-actions/configure-aws-credentials with `role-to-assume`, and says to delete the old access keys.
FAIL if it keeps access keys, or if the `sub` condition is missing or uses a wildcard that admits other repositories or environments.
