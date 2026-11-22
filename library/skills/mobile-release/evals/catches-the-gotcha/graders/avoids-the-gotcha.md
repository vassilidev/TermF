---
type: llm
---

PASS if the answer says Google Play requires updates to target API 36 (Android 16) since 31 August 2026, so `targetSdkVersion` must go to 36, and flags 16 KB memory page support for the app's native libraries (AGP 8.5.1 or later, updated native dependencies; updates without it blocked from 1 February 2027).
FAIL if it says API 34 or 35 is enough for an update, or never mentions the target API requirement.
The run starts in an empty folder: judge the code and commands given in the reply; not finding or editing project files is not a failure.
