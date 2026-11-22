---
type: llm
---

PASS if the answer installs the camera package with `npx expo install` (no hand-picked version), sets the iOS permission text through the app config (the package's entry in `plugins` with its option, or `ios.infoPlist`) rather than by editing a file under `ios/`, and says a new development build is needed (or `npx expo prebuild --clean` before a local run) because the change is native.
FAIL if it uses `npm install` with a pinned version, edits `ios/<App>/Info.plist` or another generated file by hand, or says the change can ship with EAS Update or Expo Go alone.
The run starts in an empty folder: judge the code and commands given in the reply; not finding or editing project files is not a failure.
