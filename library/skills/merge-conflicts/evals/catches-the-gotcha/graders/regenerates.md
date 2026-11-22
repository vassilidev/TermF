---
type: llm
---

PASS if the answer has pnpm write the lockfile (`pnpm install`, after taking one side with `git checkout --ours` or `--theirs` or letting pnpm merge the two sides), checks that both `zod` and `papaparse` end up in it, and says not to edit the conflict markers of the lockfile by hand.
FAIL if it resolves the markers of `pnpm-lock.yaml` by hand, deletes the lockfile without regenerating it, or keeps one side without running pnpm.
