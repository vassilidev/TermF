# Crutches

Parts of the library and of the team's brief that make up for what models did not do well on their own when they were written (2026-11, Claude Opus 5.5 and Sonnet 5). Each one costs tokens or turns on every run, so each is checked again when the default models change, and kept only while it still earns its cost.

In the items:
- "Treat the earlier reports as unverified claims" (`qa`, `secops`, `reviewer`), and "a perfect report from an earlier step is a reason to look closer" (`qa`): agents trusting what another agent says it did.
- Run the stack card's `Verify` commands and report each with its result (`front`, `back`, `devops`, `seo`, `qa`): work declared done without the checks run.
- "Change the code, never the test or a hard-coded value" (`front`, `back`), "leave existing tests as they are" (`qa`): tests bent until they pass.
- Find, then confirm each candidate on its own, with a fresh subagent for a doubtful one (`reviewer`, `secops`): reviews that always find something, findings that do not hold.
- `<example>` blocks (`po`, `lead`, `cto`): vague criteria and task goals.
- "Not for …" in the role descriptions: the planner mixing up `front`, `seo`, `ux` and `qa`.
- The reason given after each rule: rules applied to the letter where they do not fit.

In the engine (`termf-team`, for the owner):
- The brief gives the documents first and the request last: a request lost under long documents.
- The obstacle or boundary sentence: agents stopping at the first timeout, or forcing their way past a refused tool.
- Summaries with the key points first, about 300 words: reports too long, or full of names the reader never saw.
- Turn caps by size (`[team] max_turns`) and one more attempt before the user: sessions going round in circles.
- Efforts by size: thinking too little or too much for the task.
