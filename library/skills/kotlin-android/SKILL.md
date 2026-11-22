---
name: kotlin-android
description: >-
  Builds, changes and tests Kotlin and Android code the way the project's Kotlin, Android Gradle Plugin and
  Compose versions expect: screens in Jetpack Compose, view models and coroutines, Gradle build scripts and
  version catalogs, local and instrumented tests, AGP and Kotlin upgrades. Use when a task touches .kt or
  .kts files, AndroidManifest.xml, gradle/libs.versions.toml or an Android module. Not for Spring Boot
  server code (the java-spring skill covers it), and it leaves a topic to the official android/skills and
  Kotlin/kotlin-agent-skills when they are installed.
paths: "**/*.kt, **/AndroidManifest.xml, **/libs.versions.toml, **/proguard-rules.pro"
license: MIT
metadata:
  termf-stacks: "android"
  termf-roles: "front back qa"
  termf-versions: ">=8.5"
  termf-verified: "2026-11-22 kotlin@2.4.21 agp@9.5.2 compose-bom@2026.11.01"
  termf-sources: "https://github.com/android/skills https://github.com/Kotlin/kotlin-agent-skills https://kotlinlang.org/llms.txt https://kotlinlang.org/docs/whatsnew24.html https://developer.android.com/build/releases/agp-9-0-0-release-notes https://developer.android.com/build/migrate-to-built-in-kotlin https://developer.android.com/develop/ui/compose/testing/migrate-v2 https://developer.android.com/tools/agents/android-cli"
---
# Kotlin and Android

Google and JetBrains publish official skills: [android/skills](https://github.com/android/skills) (Apache-2.0: `agp-9-upgrade`, `navigation-3`, `edge-to-edge`, `migrate-xml-views-to-jetpack-compose`, `testing-setup`, `android-intent-security`…) and [Kotlin/kotlin-agent-skills](https://github.com/Kotlin/kotlin-agent-skills) (Apache-2.0: `kotlin-tooling-agp9-migration`, `kotlin-tooling-java-to-kotlin`…). They install as Claude Code plugins; nothing ships inside the Gradle artifacts. When one is loaded in the session, follow it for its topic; this skill keeps the version facts, the gotchas and the checks.

1. Read the versions in `gradle/libs.versions.toml` (`kotlin`, `agp`, `compose-bom`), else in the `plugins {}` blocks, and the Gradle version in `gradle/wrapper/gradle-wrapper.properties`. Since AGP 9 a module may declare no Kotlin plugin at all (built-in Kotlin). Done when you can name Kotlin, AGP, Gradle and the Compose BOM.
2. Open the docs of those versions: `https://kotlinlang.org/llms.txt` for Kotlin, the AGP release notes of that major for the build, and `android docs search <topic>` when the Android CLI is installed. Done when the page for the task is open.
3. Use what the project has: the version catalog for every new dependency (an alias in `libs.versions.toml`, then `libs.<alias>` in the module), the Compose BOM without versions on Compose artifacts, its DI, navigation and test libraries. Run Gradle only through `./gradlew`. Done when no new dependency carries a hard-coded version in a module script.
4. Write the change in Compose and coroutines the way the surrounding screens do: state hoisted to the view model, collected with lifecycle awareness, work on the dispatcher the project injects. Done when `./gradlew assembleDebug` builds.
5. Write the tests of `## Testing`, then run `## Verify` and put each command with its result in your report. Done when they pass, or the failure is reported with its output.

## Gotchas

- Kotlin 2.4 removed the K1 compiler: `-language-version=1.9` and older no longer compile. Context parameters (`context(logger: Logger)`) are stable, context receivers are not; explicit backing fields are stable.
- The Compose compiler is the Kotlin plugin `org.jetbrains.kotlin.plugin.compose`, versioned with Kotlin (no `kotlinCompilerExtensionVersion` since Kotlin 2.0). In its options, `StrongSkipping` and `IntrinsicRemember` are errors in 2.4 (always on): delete them from `composeCompiler {}`.
- AGP 9 has built-in Kotlin: modules no longer apply `org.jetbrains.kotlin.android` (`kotlin-android`); `kotlin-kapt` is incompatible with it, so annotation processors move to KSP (or `com.android.legacy-kapt` as a stopgap). The new DSL removes the legacy variant API (`applicationVariants`), and `targetSdk` defaults to `compileSdk`.
- AGP 9.5 needs Gradle 9.6 or later and JDK 17. An AGP or Kotlin bump is an upgrade task of its own: follow `agp-9-upgrade` or `kotlin-tooling-agp9-migration` when installed.
- Compose UI tests have v2 APIs (`androidx.compose.ui.test.junit4.v2.createComposeRule`, `androidx.compose.ui.test.v2.runComposeUiTest`); v1 is deprecated. v2 runs on `StandardTestDispatcher`, so coroutines launched by the screen wait until the test advances or waits for idle.

## Testing

Follow the skill `tests-from-criteria`: each test comes from an acceptance criterion or a real flow, at the highest level the project can run, and fails without the change.

- A user flow on a screen: a Compose UI test (`createComposeRule` v2, nodes found by text or content description, as a user would) in `src/androidTest`, run by `./gradlew connectedAndroidTest` on an emulator or device; with Robolectric in the project, the same test can run as a local test.
- View model and domain rules: local tests in `src/test` with `kotlinx-coroutines-test` (`runTest`, one per test) and the project's fakes; mock with MockK only what leaves the app (network, system services).
- One test while iterating: `./gradlew :app:testDebugUnitTest --tests '*.CartViewModelTest'`.

## Verify

- The project's own tasks first (CI workflow steps, a `check` task with ktlint or detekt when configured).
- `./gradlew assembleDebug`, `./gradlew testDebugUnitTest` (or `./gradlew test`), `./gradlew lint`; `./gradlew connectedAndroidTest` when a device or emulator is available, else say it was not run.

## Security

- Every component in `AndroidManifest.xml` declares `android:exported`; `false` unless another app must start it, and an exported one checks its caller and validates its intent extras.
- Cleartext HTTP is off by default since API 28: keep `usesCleartextTraffic` off and use a network security config for a local debug host only.
- WebView: JavaScript off unless the page needs it; `addJavascriptInterface` only for pages the app ships or fully trusts.
- No API key or secret in the repository or in `BuildConfig` from a committed file: the project's secrets plugin or local properties, and the Android Keystore for keys at runtime.

## References

- Read the matching official skill when it is installed: `agp-9-upgrade` for an AGP 9 migration, `migrate-xml-views-to-jetpack-compose` for a view migration, `testing-setup` to add a test stack, `android-intent-security` for exported components. When it is not installed, name it in your report as a suggestion (`/plugin marketplace add android/skills`, `claude plugin marketplace add Kotlin/kotlin-agent-skills`).
