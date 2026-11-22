---
description: Moving work off the main actor in a module with MainActor default isolation.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

Our Package.swift sets, for the target ImageKit:

```swift
swiftSettings: [
    .defaultIsolation(MainActor.self),
    .enableUpcomingFeature("NonisolatedNonsendingByDefault")
]
```

This function freezes the UI when it resizes big photos:

```swift
func resized(_ data: Data, to width: Int) async -> Data {
    // CPU-heavy resizing with CoreGraphics
}
```

Change it so the resizing runs off the main actor. Show the new declaration.
