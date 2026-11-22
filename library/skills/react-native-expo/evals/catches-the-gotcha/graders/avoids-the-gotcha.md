---
type: llm
---

PASS if the answer says an EAS update cannot deliver a new native module, so this change needs a new build (and a store release) with a new runtimeVersion (bumped by hand, or a `fingerprint` or `appVersion` policy), and does not offer a plain `eas update --channel production` as the way to ship it tonight.
FAIL if it gives `eas update --channel production …` as the solution for this change, or keeps runtimeVersion "1.4.0" for the build that contains the new module.
The run starts in an empty folder: judge the code and commands given in the reply; not finding or editing project files is not a failure.
