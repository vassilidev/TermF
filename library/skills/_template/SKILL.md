---
name: example-tech
description: >-
  Builds and changes <technology> code the way the installed version expects: <the tasks people ask for, in
  their own words>. Use when a task touches <the files, folders or words that show this technology>. Not for
  <the nearest technology, which another skill covers>.
paths: "<glob>, <glob>"
license: MIT
metadata:
  # Tags of TermF's stack detection that switch the skill on (TAGS in crates/termf-library/src/stack.rs), or "*"
  # alone for a skill of every stack.
  termf-stacks: "web"
  # The roles that get the skill; a session without a role gets every skill. The others get its card only.
  termf-roles: "front back qa"
  # The versions of the FIRST tag of termf-stacks the skill is written for, such as ">=15 <17", or "*" for any.
  termf-versions: ">=1"
  termf-verified: "2026-11-22 <package>@<version>"
  termf-sources: "https://example.org/docs/llms.txt https://example.org/blog"
  # Only when a passage is taken word for word, also listed in library/THIRD_PARTY_NOTICES.md:
  # credits: "owner/repo@<commit> (MIT): path/of/the/file.md"
---
# <Technology>

1. Read the installed version in <the lockfile entry or the manifest key that gives it>, and keep it as the reference for every later step. Done when you can name the version and the file it came from.
2. Open the docs of that version before writing code: <the bundled docs folder, the llms.txt or the versioned page>. Done when the page for the task is open.
3. Use the tools the project already has (package manager, linter, test runner, design system), because a second tool splits the project. Done when every command you plan to run comes from the project.
4. <A step of the procedure, in the order the work happens.> Done when <a check anyone can run or see>.
5. Run the commands of `## Verify` and put each one with its result in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- <What the model gets wrong for this version, then what to do instead, and why.>

## Verify

- `<the project's own command, from its package.json scripts, Makefile or justfile>`
- `<the canonical command, when the project has none>`

## Security

- <A pitfall specific to this technology, then the safe default.>

## References

- Read `references/topic.md` when the task touches <the topic>.
