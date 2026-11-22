---
type: llm
---

PASS if the lane uses `upload_to_play_store` (or `supply`) with `track` set to `internal` (or another test track, never the default production), points `aab` at `build/app/outputs/bundle/release/app.aab` built by `flutter build appbundle`, reads the Google service account JSON and the upload keystore with its passwords from GitHub Actions secrets written to files at run time, and keeps the keystore, `key.properties` and the JSON out of the repository.
FAIL if it leaves the track at its default, commits or inlines a key or password, or builds an APK instead of an app bundle for Play.
The run starts in an empty folder: judge the code and commands given in the reply; not finding or editing project files is not a failure.
