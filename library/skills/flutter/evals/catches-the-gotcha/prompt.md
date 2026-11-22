---
description: Deprecated Material imports after the move to Flutter 3.50, with a dependency not migrated yet.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

We upgraded to Flutter 3.50.1 and `flutter analyze` now shows hundreds of deprecation infos on `import 'package:flutter/material.dart';` across the app. We also depend on a charts package that still imports `package:flutter/material.dart` and has no new release. How do we fix this properly?
