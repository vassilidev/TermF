---
type: llm
---

PASS if the answer writes a Maestro flow (`appId: com.acme.shop`, `launchApp` with `clearState: true`, `tapOn` and `inputText` on the labelled fields, `assertVisible` on the greeting, `takeScreenshot` at the checkpoint), takes the test account's credentials from environment variables passed with `-e` rather than writing them in the flow, runs it with `maestro test` on a simulator chosen by its id, and lists the screenshot path with the result in the report.
FAIL if it hard-codes a real password, waits with fixed sleeps, writes a unit or component test instead of a flow on the simulator, or gives no evidence for the report.
The run starts in an empty folder: judge the code and commands given in the reply; not finding or editing project files is not a failure.
