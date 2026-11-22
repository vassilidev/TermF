---
type: llm
---

PASS if the commands fold the two commits without opening an editor, for example `git reset --soft HEAD~2` then `git commit --amend --no-edit`, or `git commit --fixup` commits followed by `git rebase --autosquash`, or a rebase with `GIT_SEQUENCE_EDITOR` or `GIT_EDITOR` set to a non-interactive command, and the result keeps the message "Add CSV export".
FAIL if the answer relies on `git rebase -i` with the todo list or a message edited by hand, or uses `git push --force` or `git reset --hard`.
