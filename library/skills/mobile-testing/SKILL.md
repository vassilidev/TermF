---
name: mobile-testing
description: >-
  Writes and runs end-to-end checks of a mobile app on iOS simulators and Android emulators, from the acceptance
  criteria: boots a device with xcrun simctl or the Android emulator and adb, installs the debug build, drives the
  user flow with the project's tool (Maestro flows, Detox, Flutter integration_test or Patrol, XCUITest,
  Espresso) and keeps screenshots as evidence for the report. Use when a task asks to prove an acceptance
  criterion on a simulator or emulator, write or fix a mobile end-to-end flow, or reproduce a bug on a device.
  Not for unit, component or widget tests of the code, which the skill of the technology covers.
paths: "**/.maestro/**, **/maestro/**, **/.detoxrc.*, **/e2e/**, **/integration_test/**, **/patrol_test/**, **/*UITests/**, **/androidTest/**"
license: MIT
metadata:
  termf-stacks: "ios android expo react-native flutter"
  termf-roles: "qa"
  termf-versions: "*"
  termf-verified: "2026-11-22 xcrun simctl help (Xcode 26.6), xcodebuild destinations, adb 1.0.41 and emulator 36.3 help, Android CLI emulator and screen commands, Maestro CLI reference and commands (Apache-2.0, Java 17+), Detox test CLI, Flutter integration_test, Patrol test CLI"
  termf-sources: "https://docs.maestro.dev/maestro-cli/maestro-cli-commands-and-options.md https://docs.maestro.dev/maestro-cli/how-to-install-maestro-cli.md https://docs.maestro.dev/reference/commands-available.md https://github.com/mobile-dev-inc/Maestro https://developer.android.com/tools/adb https://developer.android.com/studio/run/emulator-commandline https://developer.android.com/studio/run/emulator-console https://developer.android.com/tools/agents/android-cli https://developer.android.com/tools/agents/android-cli/commands/emulator_start https://developer.android.com/tools/agents/android-cli/commands/screen_capture https://wix.github.io/Detox/docs/cli/test https://wix.github.io/Detox/docs/introduction/project-setup https://wix.github.io/Detox/docs/api/matchers https://wix.github.io/Detox/docs/api/expect https://docs.flutter.dev/testing/integration-tests https://patrol.leancode.co/cli-commands/test"
---
# Mobile end-to-end testing

TermF's visual review does not capture a simulator or an emulator yet: the screenshots you take are the evidence, and they reach the user only through the paths listed in your report.

1. Find the project's end-to-end tool and keep it: Maestro (`.maestro/` or YAML flows starting with `appId:`), Detox (`.detoxrc.js`), Flutter `integration_test/` or Patrol (`patrol` in `dev_dependencies`, `patrol_test/`), an XCUITest target (`*UITests`), Espresso or Compose tests in `src/androidTest/`. With none, Maestro when `maestro` is installed; when no tool is installed, report the gap and the official install command, since installing a tool changes the user's machine. Done when you can name the tool and the command the project runs it with.
2. Get a device the user already has, and pass its id to every command (`booted` picks any one when several run). iOS: `xcrun simctl list devices available`, then `xcrun simctl boot <udid>` and `xcrun simctl bootstatus <udid>` (waits until booted). Android: `android emulator list` and `android emulator start --headless <avd>` (returns when booted) when the Android CLI is installed, else `emulator -list-avds` and `emulator -avd <avd> -no-window -no-snapshot-save`, then `adb -s <serial> wait-for-device`. Done when `xcrun simctl list devices booted` or `adb devices` shows the device.
3. Build and install the debug build with the project's command (`npx expo run:ios`, `flutter build`, `xcodebuild build`, `./gradlew :app:assembleDebug`), then `xcrun simctl install <udid> <App>.app` or `adb -s <serial> install -r <app>.apk`. Start from a known state by resetting the app only: `adb -s <serial> shell pm clear <package>`, `xcrun simctl uninstall <udid> <bundle id>` then install again, or the tool's own reset (Maestro `launchApp` with `clearState: true`). Grant permissions up front when the criterion is not about the prompt: `xcrun simctl privacy <udid> grant <service> <bundle id>`, `adb install -g`. Done when the app opens on its first screen.
4. Write one flow per acceptance criterion, steps as a user takes them: find elements by visible text or accessibility label (Maestro `tapOn`, `inputText`, `assertVisible`; Detox `by.text`, `by.label`), test ids only when the screen has no stable text, and wait with the tool's waits (`extendedWaitUntil`, Detox `waitFor`), never a fixed sleep. Test accounts and data come from the project's fixtures, credentials from environment variables (`maestro test -e USER=… flow.yaml`). Done when the flow fails on the build without the feature (or with the bug) and passes with it.
5. Capture the evidence at each criterion's checkpoint: Maestro `takeScreenshot`, Detox `--take-screenshots all`, `xcrun simctl io <udid> screenshot <file>.png`, `adb -s <serial> exec-out screencap -p > <file>.png`, or `android screen capture --device=<serial> --output=<file>.png`. Fix the status bar before iOS shots (`xcrun simctl status_bar <udid> override --time 9:41`). Write files to the tool's output folder (Maestro `--test-output-dir`, Detox `artifacts/`) or another folder git ignores. Done when each criterion has a screenshot path and the command that produced it.
6. Run the commands of `## Verify`, then put in your report, criterion by criterion, PASS or FAIL with the command, its result and the screenshot paths. A flow that fails once and passes on a rerun is reported as flaky with both outputs; no retry is added to hide it. Done when every criterion has its evidence.
7. Leave the machine as you found it: shut down what you booted (`xcrun simctl shutdown <udid>`, `android emulator stop <serial>` or `adb -s <serial> emu kill`) and remove the screenshots from any tracked folder. Done when only the devices that were running before still run.

## Gotchas

- With several simulators booted, `booted` lets simctl choose one: always the UDID. With several Android devices, `adb -s <serial>`; `-e` targets the only emulator.
- The `emulator` command is deprecated in favour of the Android CLI (`android emulator create|start|stop|list`), whose `start` blocks until the device accepts adb commands.
- Maestro needs Java 17 or later. `--device` is a global option, so it goes before the subcommand: `maestro --device <id> test flow.yaml`. Reports: `--format JUNIT` (or `HTML`) with `--output <file>`, artifacts with `--test-output-dir <dir>`. `maestro cloud` uploads the app to Maestro's servers.
- Detox runs the build command of a configuration first: `detox build -c <config>`, then `detox test -c <config>`; `--take-screenshots failing|all`, `--record-videos failing`, `-a <dir>` for artifacts, `-H` for headless runs.
- Flutter: `flutter test integration_test/<file>_test.dart -d <device id>`; the test's `main` calls `IntegrationTestWidgetsFlutterBinding.ensureInitialized()`. Web needs `flutter drive` with chromedriver. Patrol (system dialogs, notifications): `patrol test --target patrol_test/<file>_test.dart --device "<name>"`.
- XCUITest on a simulator: `xcodebuild test -scheme <Scheme> -destination 'platform=iOS Simulator,id=<udid>' -only-testing:<Target>/<Class>/<test>`. Espresso and Compose tests: `./gradlew connectedAndroidTest` on the connected device.
- A criterion that starts from a link: `xcrun simctl openurl <udid> <url>`, `adb -s <serial> shell am start -a android.intent.action.VIEW -d <url>`, Maestro `openLink`.
- Appearance criteria: `xcrun simctl ui <udid> appearance dark`, Maestro `setDarkMode`; location: Maestro `setLocation`.
- `xcrun simctl erase all`, `xcrun simctl delete`, and `-wipe-data` on an existing AVD destroy the user's devices and their data: reset the app, not the device; erase only a device you created for the run.

## Verify

- The project's end-to-end command first (`package.json` script, Makefile, CI job), on the device of step 2.
- Else the tool's own: `maestro --device <id> test --format JUNIT --output <report>.xml --test-output-dir <dir> <flows>`, `detox test -c <config> --take-screenshots all`, `flutter test integration_test -d <device id>`, `patrol test`, `xcodebuild test …`, `./gradlew connectedAndroidTest`.
- `ls` the screenshot folder: one file per criterion checkpoint, each opened once to check it shows the expected screen.

## Security

- Test accounts and fixture data only; never a real user's account, and credentials only through environment variables, never written in a flow.
- Screenshots and recordings can show personal data, tokens or keys: look at each one before listing it, and keep them out of tracked folders.
- Point the app at the local or test backend the project configures for tests: a flow against production places orders and sends emails for real.
- Install only builds made from the project; never an APK or IPA downloaded from elsewhere.
- Cloud runners and device farms (`maestro cloud`, others) send the app binary off the machine: only when the user asked for it.

## References

- Read `https://docs.maestro.dev/reference/commands-available.md` before writing a Maestro step you have not used in this project, and `https://docs.maestro.dev/maestro-cli/maestro-cli-commands-and-options.md` before changing how flows run.
- Read the technology's skill (`swift-ios`, `kotlin-android`, `react-native-expo`, `flutter`) for its build commands and unit tests, and `tests-from-criteria` for what each test must prove.
