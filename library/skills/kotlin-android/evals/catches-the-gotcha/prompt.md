---
description: Room's annotation processor in a module after the move to AGP 9.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

We upgraded to AGP 9.5.2 and Kotlin 2.4.21. The app module now fails at configuration time. Here is app/build.gradle.kts:

```kotlin
plugins {
    alias(libs.plugins.android.application)
    id("kotlin-kapt")
}
dependencies {
    implementation(libs.androidx.room.runtime)
    kapt(libs.androidx.room.compiler)
}
```

Fix the module so Room's code generation works.
