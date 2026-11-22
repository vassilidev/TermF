---
name: mobile-release
description: >-
  Prepares and checks the release pipeline of an iOS or Android app for TestFlight, the App Store, the Google Play
  testing tracks and production: certificates, provisioning profiles and App Store Connect API keys, Android upload
  keys and Play App Signing, version and build numbers, EAS Submit for Expo, fastlane lanes, and the store rules in
  force (Play target API level, Xcode and SDK minimum for uploads, 16 KB memory pages, privacy manifests, account
  deletion, review notes). Use when a task touches signing, eas.json submit profiles, a Fastfile, a store build or
  a release workflow of a mobile app. Not for the app's own code or its tests.
paths: "**/eas.json, **/fastlane/**, **/Fastfile, **/Appfile, **/Matchfile, **/ExportOptions.plist, **/*.entitlements, **/PrivacyInfo.xcprivacy, **/key.properties"
license: MIT
metadata:
  termf-stacks: "ios android expo react-native flutter"
  termf-roles: "devops"
  termf-versions: "*"
  termf-verified: "2026-11-22 Play target API 36 (since 2026-08-31), Play 16 KB pages (updates blocked from 2027-02-01), App Store uploads Xcode 26 and iOS 26 SDK (since 2026-04-28), TestFlight and Play testing limits, Play App Signing, EAS Submit, fastlane match, pilot and supply"
  termf-sources: "https://developer.apple.com/news/upcoming-requirements/ https://developer.apple.com/help/app-store-connect/test-a-beta-version/testflight-overview https://developer.apple.com/help/app-store-connect/get-started/app-store-connect-api https://developer.apple.com/app-store/review/guidelines/ https://support.google.com/googleplay/android-developer/answer/11926878 https://support.google.com/googleplay/android-developer/answer/9845334 https://support.google.com/googleplay/android-developer/answer/14151465 https://support.google.com/googleplay/android-developer/answer/9842756 https://support.google.com/googleplay/android-developer/answer/13327111 https://developer.android.com/guide/practices/page-sizes https://developer.android.com/build/building-cmdline https://docs.expo.dev/submit/introduction.md https://docs.expo.dev/build/setup.md https://docs.fastlane.tools/actions/match/ https://docs.fastlane.tools/app-store-connect-api/ https://docs.fastlane.tools/actions/upload_to_play_store/ https://docs.flutter.dev/deployment/android https://docs.flutter.dev/deployment/ios"
---
# Mobile release

1. Find the release path the project already has and keep it: EAS (`eas.json` build and submit profiles), fastlane (`fastlane/Fastfile`, `Appfile`, `Matchfile`), a CI workflow that builds and uploads, or none (Xcode archive, Gradle, `flutter build`). A second path splits the team's releases. Done when you can name the tool, its profiles or lanes, and the CI job that runs them.
2. Check the store rules in force today against the project, with `references/store-rules.md` and the official pages it links (they change every year): Android `targetSdk` (the Gradle module or the app config), native libraries and 16 KB pages, the Xcode version the iOS build uses, the privacy manifest, account deletion. Done when each rule is met or listed in your report as a gap with its date.
3. Version and build numbers: every upload needs a build number higher than the last one the store received (`versionCode`; `CFBundleVersion`; `ios.buildNumber` and `android.versionCode` in an Expo config; the `+N` of `version: x.y.z+N` in `pubspec.yaml`). With EAS, use remote versioning with `autoIncrement` on the store profile. Done when the next build number is set and you can say where it comes from.
4. iOS signing: an Apple distribution certificate and an App Store provisioning profile, managed by the project's tool: EAS credentials (created after an Apple sign-in, which the user does), or fastlane `match` in `readonly` mode on CI with `MATCH_PASSWORD` from the CI secret store. Uploads authenticate with an App Store Connect API key: the `.p8` file (downloadable once), its Key ID and the Issuer ID, each a CI secret. Done when the CI configuration reads every credential from the secret store and the repository holds none.
5. Android signing: the upload key (a keystore, RSA 2048 or more) signs the app bundle; Google keeps the app signing key (Play App Signing, the default for apps uploaded as bundles) and re-signs what users install. The keystore and its passwords come from CI secrets at build time (`android/key.properties` for Flutter, kept out of git). Build the bundle with the project's command: `./gradlew :app:bundleRelease`, `flutter build appbundle` (output `build/app/outputs/bundle/release/app.aab`), or `eas build --platform android`. Done when the release bundle is signed by the upload key from the secret store.
6. Target a test track by default: TestFlight on iOS, the internal track on Google Play. Production and wider tracks are a decision of the user. In fastlane, set `track: "internal"` on `upload_to_play_store`, since its default is production; in EAS Submit, set `track` in the submit profile. Done when no pipeline path reaches production without a manual step.
7. Uploading, promoting a build, starting a rollout or submitting for review leaves the repository and reaches real testers or users. Prepare the exact command (`eas submit --platform ios --latest`, `fastlane <lane>`, `xcrun altool --upload-app --type ios -f <ipa> --apiKey <key id> --apiIssuer <issuer id>`) and run it only when the user asked for that upload in this task; otherwise report `blocked` with the command and the question. Done when the command is in your report, with its result or the question.
8. Run the commands of `## Verify` and put each one with its result in your report, with how to undo the change. Done when they pass, or the failure is reported with its output.

## Gotchas

- fastlane's `upload_to_play_store` (alias `supply`) uploads to the production track unless `track` is set; `release_status: "draft"` uploads without rolling out.
- EAS Submit on Android needs the app created in the Play Console first; the first release goes to the internal track and the Console setup must be finished before promotion. Non-interactive CI runs need the Google service account key and the App Store Connect API key configured beforehand. A TestFlight build never goes to App Review by itself.
- Google Play, phones and tablets: new apps and updates target API 36 (Android 16) since 2026-08-31 (extensions ended 2026-11-01); an existing app below API 35 is hidden from new users on newer Android.
- 16 KB memory pages: apps targeting API 35+ that ship native libraries (every React Native, Expo and Flutter app does) cannot release an update without 16 KB support from 2027-02-01. AGP 8.5.1+ and NDK r28+ align by default; prebuilt SDKs need versions that support it. Check with `zipalign -c -P 16 -v 4 <app>.apk`.
- App Store Connect: uploads must be built with Xcode 26 or later and an iOS 26 SDK since 2026-04-28. Required-reason APIs need a privacy manifest, third-party SDKs included.
- New personal Google Play accounts (created after 2023-11-13) need a closed test with at least 12 testers opted in for 14 consecutive days before applying for production.
- Each TestFlight build expires after 90 days; the first build sent to an external group goes through Beta App Review.
- Google Play's internal track takes up to 100 testers and serves a new bundle within minutes, but the first test link after a first publish can take hours.
- An App Store Connect API key's `.p8` downloads once: when it is lost, the key is revoked and a new one created, and every CI secret that held it changes.

## Verify

- The project's own lanes, scripts and CI jobs first, in a dry or local mode when they have one.
- Android: the release bundle builds (`./gradlew :app:bundleRelease` or `flutter build appbundle`); `bundletool dump config --bundle=<app>.aab | grep alignment` shows `PAGE_ALIGNMENT_16K` when the app has native code.
- iOS: the archive builds with the release configuration (`flutter build ipa`, the project's lane, or `xcodebuild archive`), and `xcodebuild -version` on the build machine meets the Xcode minimum.
- Secrets: `git ls-files | grep -iE '\.(p8|p12|jks|keystore|mobileprovision)$|key\.properties|service.?account.*\.json'` prints nothing.

## Security

- Never commit a `.p8`, `.p12`, provisioning profile, keystore, `key.properties`, Google service account JSON or `MATCH_PASSWORD`; the CI writes file secrets to a temporary path at run time and never prints them.
- Give each credential the least access: an App Store Connect key with the smallest role that can upload (a team key reaches every app of the team), a Google service account limited to this app's releases.
- A leaked `.p8` is revoked and replaced in App Store Connect; a leaked upload key is reset through the Play Console; both are the user's actions: report the leak at once, with what it exposes.
- A lost app signing key cannot be recovered when the developer manages it; keep Play App Signing.
- No upload, promotion, rollout or review submission without the user's go-ahead for that exact action (step 7).

## References

- Read `references/store-rules.md` before a release, a target SDK change or a review rejection: each store rule with its date and official page, and the review notes that are not code.
- When EAS is the path, read `https://docs.expo.dev/submit/introduction.md` before editing a submit profile; when fastlane is, read the page of the action you change on `https://docs.fastlane.tools`.
