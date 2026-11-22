---
type: llm
---

PASS if the answer moves the app to the `material_ui` package (and `cupertino_ui` when Cupertino is used) with `flutter pub add` and `dart fix --apply --code=migrate_design_widgets`, and keeps the unmigrated charts package working with `MaterialUiCompatibilityBridge` in `MaterialApp.builder`, warning that theme objects passed to that package may not type-check until it migrates.
FAIL if it silences the deprecation (an `ignore` comment or a disabled lint), pins Flutter to an older release, or rewrites the imports by hand without adding the package.
The run starts in an empty folder: judge the code and commands given in the reply; not finding or editing project files is not a failure.
