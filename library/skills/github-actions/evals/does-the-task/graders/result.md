---
type: llm
---

PASS if the workflow declares read-only token permissions (a top-level `permissions:` with `contents: read` or `{}`), sets `persist-credentials: false` on actions/checkout, cancels superseded pull request runs with `concurrency`, and either pins each action by a full commit SHA or tells the user to pin them (with `gh actions-lock` or by SHA from each action's repository).
FAIL if there is no `permissions:` key, if it uses actions/checkout@v4 or older, or if it presents commit SHAs it could not have looked up as real.
