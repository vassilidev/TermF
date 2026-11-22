---
description: A CI lane that uploads a Flutter app bundle to Google Play's internal track.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Our Flutter app (Flutter 3.50) already has fastlane with an empty `platform :android do … end` in fastlane/Fastfile, and we use GitHub Actions. On every tag `v*`, we want CI to build the app bundle and send it to our testers on Google Play. Write the lane and the workflow job, and tell us what to set up.
