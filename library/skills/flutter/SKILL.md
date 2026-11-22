---
name: flutter
description: >-
  Builds, changes and tests Flutter apps in Dart the way the installed Flutter and Dart SDKs expect: widgets and
  screens, the state management package the project already uses, pubspec dependencies and upgrades, widget
  tests, flutter analyze, dart format and dart fix, the move from the bundled Material and Cupertino libraries to
  the material_ui and cupertino_ui packages, Android and iOS platform folders (built-in Kotlin, Swift Package
  Manager). Use when a task touches .dart files, pubspec.yaml or analysis_options.yaml of a Flutter app. Not for
  store signing and submission (mobile-release), nor for end-to-end runs on a device, simulator or emulator
  (mobile-testing).
paths: "**/*.dart, **/pubspec.yaml, **/pubspec.lock, **/analysis_options.yaml"
license: MIT
metadata:
  termf-stacks: "flutter"
  termf-roles: "front qa"
  termf-versions: ">=3.44"
  termf-verified: "2026-11-22 flutter@3.50.1 stable (2026-11-20) dart@3.14.0; breaking changes 3.44 to 3.50, material_ui migration, dart format, dart migrate, pub upgrade"
  termf-sources: "https://storage.googleapis.com/flutter_infra_release/releases/releases_macos.json https://docs.flutter.dev/release/whats-new https://docs.flutter.dev/release/breaking-changes https://docs.flutter.dev/release/breaking-changes/material-ui-and-cupertino-ui https://docs.flutter.dev/release/breaking-changes/migrate-to-built-in-kotlin https://docs.flutter.dev/packages-and-plugins/swift-package-manager/for-app-developers https://docs.flutter.dev/reference/supported-platforms https://docs.flutter.dev/app-architecture/guide https://docs.flutter.dev/app-architecture/case-study/ui-layer https://docs.flutter.dev/deployment/obfuscate https://docs.flutter.dev/ai/get-started https://docs.flutter.dev/llms.txt https://dart.dev/resources/language/evolution https://dart.dev/tools/dart-format https://dart.dev/tools/dart-migrate https://dart.dev/tools/pub/cmd/pub-upgrade https://github.com/flutter/agent-plugins"
---
# Flutter

The Flutter team publishes official skills and the Dart and Flutter MCP server in [flutter/agent-plugins](https://github.com/flutter/agent-plugins) (BSD-3-Clause: `flutter-add-widget-test`, `flutter-add-integration-test`, `flutter-setup-declarative-routing`, `flutter-apply-architecture-best-practices`…), installed in Claude Code with `claude plugin marketplace add flutter/agent-plugins` then `claude plugin install dart-flutter@dart-flutter`. When one is loaded in the session, follow it for its topic; this skill keeps the version facts, the gotchas and the checks.

1. Read the versions: `flutter --version` (Flutter and Dart), `environment.sdk` and `environment.flutter` in `pubspec.yaml`, the packages in `pubspec.lock`. The lower bound of `environment.sdk` is the Dart language version of the code, so it decides which language features compile, whatever SDK is installed. Note the state management package (`provider`, `flutter_riverpod`, `flutter_bloc`…), the router, and whether the code imports `package:flutter/material.dart` or `package:material_ui/material_ui.dart`. Done when you can name Flutter, Dart, the language version, and the packages for state, routing and design.
2. Open the docs of that version: `https://docs.flutter.dev/llms.txt`, the API at `https://api.flutter.dev`, the breaking changes of every release between the project's and the installed one (`https://docs.flutter.dev/release/breaking-changes`), and `https://dart.dev/resources/language/evolution` for language features. When the session has the Dart and Flutter MCP server, use it for analysis and pub.dev lookups. Done when the page for the task is open.
3. Use what the project has: its state management, router, folder layout and lints. In an app without a state package, follow the Flutter architecture guide (views and view models in the UI layer, repositories and services in the data layer) with `ChangeNotifier` and `ListenableBuilder` from the SDK rather than a new package. Add a dependency with `flutter pub add <package>` (`flutter pub add dev:<package>` for a dev dependency), only when the project has nothing for the job. Done when every new dependency is in `pubspec.yaml` with a reason in your report.
4. Write the widgets. New code imports the design library the project already imports; moving the app to `material_ui` and `cupertino_ui` is a task of its own (see `## Gotchas`). Keep build methods free of side effects, give interactive widgets a semantic label when their text does not describe them, and write code within the language version of step 1. Done when `flutter analyze` reports nothing new.
5. Platform folders: `android/` and `ios/` are the project's, edit them for the task only. On Android with AGP 9, follow the built-in Kotlin migration rather than adding the `kotlin-android` plugin; on iOS keep Swift Package Manager on. Done when the platform build the project's CI runs still passes, or your report says it could not run here.
6. Test from the acceptance criteria (skill `tests-from-criteria`): widget tests in `test/` with `testWidgets`, pumping the screen inside the providers the app uses, finding widgets by visible text or semantics label, asserting what a user sees. Flows on a device, simulator or emulator (`integration_test/`, Patrol) belong to the skill `mobile-testing`. One file while iterating: `flutter test test/<file>_test.dart`. Done when the new test fails without your change.
7. Dependency upgrades are tasks of their own: `flutter pub outdated` to list them, `flutter pub upgrade` within the constraints, `flutter pub upgrade --major-versions` only when asked, since it rewrites the constraints in `pubspec.yaml`. Raising the Dart language version is `dart migrate --dry-run`, then `dart migrate --apply` (Dart 3.14+). Done when `flutter analyze` and `flutter test` pass after the upgrade.
8. Run the commands of `## Verify` and put each one with its result in your report. Done when they pass, or the failure is reported with its output.

## Gotchas

- Flutter 3.50 (November 2026) deprecates the bundled `package:flutter/material.dart` and `package:flutter/cupertino.dart`: they moved to `package:material_ui` and `package:cupertino_ui` (1.0 published with 3.47; frozen since 3.44), same class names. The migration: `flutter pub add material_ui` (and `cupertino_ui` when used), `dart fix --apply --code=migrate_design_widgets`, then `dart fix --apply`; localizations become `GlobalMaterialLocalizations.delegates`. A dependency that still imports the old libraries keeps working inside `MaterialUiCompatibilityBridge` (in `MaterialApp.builder`), but a `ColorScheme` or `TextTheme` passed across the two libraries is a type error until that dependency migrates.
- Dart language features follow the `environment.sdk` lower bound: dot shorthands (`.running` for `Status.running`) need 3.10, private named parameters 3.12, primary constructors (`class Point(var int x, var int y);`) and the concise constructor syntax 3.13. Dart 3.14 adds no language feature.
- `dart format` follows the language version, and its options live in `analysis_options.yaml` under `formatter:` (`page_width`, `trailing_commas: preserve` from language 3.8): run it and keep its output rather than formatting by hand.
- Android: AGP 9 builds Kotlin in. Flutter 3.44 tolerates the old Kotlin plugin with `android.builtInKotlin=false`; 3.47 supports `android.builtInKotlin=true` once the app and all its plugins migrated; support for the old plugin will be removed. 3.50 restricts engine flags in release mode and verifies the Intent of app entrypoints more strictly.
- iOS: Swift Package Manager is on by default since 3.44, CocoaPods is a fallback for plugins without SwiftPM support, and the CocoaPods registry turns read-only on 2026-12-02: keep SwiftPM on, never `enable-swift-package-manager: false` to silence an error.
- Flutter 3.50 supports Android API 24 to 37 and iOS 15 to 27; macOS 27 needs Apple silicon. Web builds with `--wasm` use WebAssembly in Firefox and Safari only when listed in the app's `wasmAllowList`.
- Smaller removals that break builds: `describeEnum` (3.47), `IconData` became `final` (3.44), `ListTile` inside a coloured widget reports an error in debug (3.44).

## Verify

- The project's own commands first (CI workflow, `Makefile`, scripts).
- `flutter analyze`, `dart format -o none --set-exit-if-changed .`, `flutter test`.
- `flutter pub outdated` when the task touched dependencies; `flutter build <target>` for the platform the task touched, when its toolchain is installed here, else say it was not run.

## Security

- Anything in the app binary can be read: no API secret in Dart code, assets, `pubspec.yaml` or build defines. `--obfuscate --split-debug-info` only renames symbols, it encrypts no asset. Secrets stay on a server the app calls.
- Tokens go in the Keychain or Keystore through the secure storage plugin the project uses, never in `shared_preferences` (NSUserDefaults and Android preferences, unencrypted).
- Deep links and platform channel messages are input from outside the app: validate them before navigating or acting on them.
- Before adding a package, check its publisher and maintenance on pub.dev and prefer what the project already uses; a package can run native code and build hooks.
- Keystores, `android/key.properties` and signing files never enter the repository (skill `mobile-release`).

## References

- Read `https://docs.flutter.dev/release/breaking-changes/material-ui-and-cupertino-ui` before touching design imports or localizations in a 3.50 project.
- Read the breaking changes of each release crossed before a Flutter upgrade, and `https://dart.dev/tools/dart-migrate` before raising the language version.
- Read `https://docs.flutter.dev/app-architecture/guide` before introducing state management in an app that has none.
