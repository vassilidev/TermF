---
name: swift-ios
description: >-
  Builds, changes and tests Swift code for iOS, macOS and Swift packages the way the installed Swift and
  Xcode versions expect: SwiftUI views and view models, Swift concurrency and actor isolation, Swift Testing
  and XCTest, Package.swift, xcodebuild schemes and simulator destinations. Use when a task touches .swift
  files, Package.swift, an .xcodeproj or .xcworkspace, or an iOS app target. Not for Kotlin or Android code
  (kotlin-android), and not for Objective-C only code.
paths: "**/*.swift, **/Package.swift, **/Package.resolved, **/*.xcodeproj/**, **/*.xcworkspace/**, **/*.xctestplan"
license: MIT
metadata:
  termf-stacks: "swift ios"
  termf-roles: "front qa"
  termf-versions: "swift>=5.10"
  termf-verified: "2026-11-22 swift@6.4.3 xcode@27.2"
  termf-sources: "https://www.swift.org/blog/swift-6.4-released/ https://www.swift.org/blog/swift-6.3-released/ https://www.swift.org/blog/swift-6.2-released/ https://docs.swift.org/swiftpm/documentation/packagemanagerdocs/swifttest https://developer.apple.com/documentation/xcode-release-notes https://developer.apple.com/documentation/xcode/giving-external-agents-access-to-xcode https://developer.apple.com/documentation/testing"
---
# Swift and iOS

1. Read the versions: `// swift-tools-version:` on the first line of `Package.swift`, the `SWIFT_VERSION` build setting of the app target, `swift --version` and `xcodebuild -version`. Also read the isolation default: `defaultIsolation(MainActor.self)` in `Package.swift`, or the target's "Default Actor Isolation" build setting. Done when you can name the Swift language mode, the toolchain, Xcode and the default isolation.
2. Open the docs of that version: swift.org release posts and `https://docs.swift.org/latest/documentation` for the language and SwiftPM; Apple developer pages for frameworks (append `.md` to a page URL to get it as Markdown). When the session has the Xcode MCP server (see `## References`), use its documentation search. Done when the page for the task is open.
3. Use what the project has: SwiftPM or the Xcode project, its architecture (MVVM, TCA…), its test framework, `swift format` or SwiftLint configuration. Add a dependency with SwiftPM in the manifest the project already uses; an Xcode project's package references live in the project file, so change them in Xcode or with the Xcode tools rather than by hand when the file is the old `project.pbxproj` format. Done when every tool and dependency comes from the project.
4. Write the change with the isolation the project chose: under MainActor default isolation, unannotated code already runs on the main actor, so add `@MainActor` nowhere new and mark work that must leave it with `@concurrent` (or `nonisolated` for synchronous code). Under nonisolated default, annotate UI-facing types `@MainActor`. Done when the build shows no new concurrency diagnostic.
5. Write the tests of `## Testing`, then run `## Verify` and put each command with its result in your report. Done when they pass, or the failure is reported with its output.

## Gotchas

- Swift 6.2 "approachable concurrency": default isolation can be `MainActor` per module (`-default-isolation MainActor`, SwiftPM `.defaultIsolation(MainActor.self)`), `@concurrent` marks async work that runs off the caller's actor, and with the upcoming feature `NonisolatedNonsendingByDefault` a `nonisolated async` function runs on the caller's actor (`nonisolated(nonsending)`). The same code compiles differently depending on these settings: read them first (step 1), never guess.
- Fixing a data race by sprinkling `@MainActor`, `nonisolated(unsafe)` or `@unchecked Sendable` hides it; make the value `Sendable`, keep it inside one actor, or pass a copy.
- Swift 6.4: Swift Build is SwiftPM's default build system; `await` is allowed in `defer`; `XCTAssert…` works inside Swift Testing and `#expect` inside XCTest, and both report correctly.
- Xcode 27.2 can store the project as JSON (`.xcproj` inside the `.xcodeproj` bundle), readable by Xcode 27 and later only: do not convert an existing project's format unless the task asks, since older Xcode versions on the team stop opening it.
- Swift Testing ships with the toolchain: `import Testing`, `@Test`, `#expect`, `#require` (throws, use `try`), `@Test(arguments:)` for parameterized cases, which run in parallel unless the suite has `.serialized`. Exit tests (6.2) check code that traps.

## Testing

Follow the skill `tests-from-criteria`: each test comes from an acceptance criterion or a real flow, at the highest level the project can run, and fails without the change.

- New tests go in the framework the target already uses; for a new target or file, Swift Testing.
- A user flow on a screen: an XCUITest in the UI test target, run on a simulator, finding elements by accessibility label as a user would.
- View model and domain rules: unit tests against the real types with fakes only for what leaves the app (network, storage, clock), injected through the initializer.
- One test while iterating: `swift test --filter CartTests` for a package; `xcodebuild test -scheme <Scheme> -destination 'platform=iOS Simulator,name=<device>' -only-testing:<Target>/<Suite>/<test>` for an app (pick a device from `xcrun simctl list devices available`).

## Verify

- The project's own commands first (CI workflow, `Makefile`, fastlane lanes).
- Package: `swift build`, `swift test`. App: `xcodebuild build` and `xcodebuild test` with the scheme and a simulator destination.
- Format or lint with what the project configures: `swift format lint -r -s .` when it has a `.swift-format` file, `swiftlint` when it has `.swiftlint.yml`.

## Security

- Secrets and tokens go in the Keychain; `UserDefaults` is stored unencrypted on disk.
- App Transport Security requires HTTPS: fix the server rather than adding an `NSAppTransportSecurity` exception, and never `NSAllowsArbitraryLoads`.
- Keep URLSession's default server trust evaluation; certificate pinning only against keys bundled with the app, with default handling for every other host.
- No API key in source or in `Info.plist` committed to the repository: the project's configuration files kept out of git, or a server that holds the key.

## References

- Xcode MCP server: in Xcode 27, turn on Xcode > Settings > Intelligence > "Allow External Agents to Use Xcode Tools", open the project in Xcode, then `claude mcp add --transport stdio xcode -- xcrun mcpbridge`. It builds, runs tests, renders previews and searches the docs. Suggest it in your report when it is missing; the user turns it on, not you.
- Read the swift.org release post of the project's Swift version before changing concurrency annotations.
