---
type: llm
---

PASS if the answer reads main's side before editing (`git log --merge`, or the commits of main that touch the file with `git show`), shows the base, ours and theirs of the file (`git checkout --conflict=diff3` or `zdiff3`, `git diff --base` / `--ours` / `--theirs`, or `git show :1:` / `:2:` / `:3:`), keeps both changes in `formatInvoice`, checks that no marker is left (`git diff --check` or `--cached --check`) and runs the build or the tests before concluding the merge.
FAIL if it takes `--ours` or `--theirs` for the whole file without reading the other side, or concludes the merge without checking for markers or running the tests.
