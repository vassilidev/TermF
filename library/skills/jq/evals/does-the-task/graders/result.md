---
type: llm
---

PASS if (1) uses `-r`, keeps only releases whose `draft` is false, sums asset sizes so that a release without assets gives 0 (for example `[.assets[].size] | add // 0`), and prints the two values with `@tsv` or an equivalent tab join; and (2) passes the tag with `--arg` (read as a `$` variable in a single-quoted filter) and writes through a temporary file then `mv`.
FAIL if `$TAG` is spliced into the filter text, if the edit redirects jq's output onto `releases.json` itself, or if a release without assets prints `null`.
