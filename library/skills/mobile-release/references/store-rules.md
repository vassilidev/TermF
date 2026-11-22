# Store rules and release facts

Read before a release, a target SDK change or a review rejection. Checked on the official pages on 2026-11-22; both stores change these every year, so open the page before relying on a date. Apple's and Google's texts are restated here, not quoted.

## Contents

- App Store and TestFlight
- Google Play
- Review notes that are not code
- Commands by toolchain

## App Store and TestFlight

| Rule | Since | Page |
|---|---|---|
| Uploads built with Xcode 26 or later and an SDK for iOS 26, iPadOS 26, tvOS 26, visionOS 26 or watchOS 26 | 2026-04-28 | https://developer.apple.com/news/upcoming-requirements/ |
| iOS and iPadOS apps uploaded target iOS 13 or later | 2026-09-09 | same |
| Approved reasons declared for required-reason APIs, including those used by third-party SDKs (privacy manifest `PrivacyInfo.xcprivacy`) | 2024-05-01 | same |
| Answers to the new age rating questions in App Store Connect, App Information | 2026-01-31 | same |
| DSA trader status to distribute in the EU | 2024-10-16; apps without it removed from the EU store since 2025-02-17 | same |

The upcoming requirements page listed nothing dated after 2026-09-09 when checked.

TestFlight (https://developer.apple.com/help/app-store-connect/test-a-beta-version/testflight-overview):
- Internal testers: up to 100 App Store Connect users of the team. External testers: up to 10,000.
- A build stays testable for 90 days.
- The first build added to an external group goes through Beta App Review; later builds may not need a full review.
- Read the export compliance page when the app uses encryption, to know whether documentation is needed.

App Store Connect API keys (https://developer.apple.com/help/app-store-connect/get-started/app-store-connect-api):
- Team keys: created by the Account Holder or an Admin, one role per key, valid for every app of the team; name and role cannot change after creation.
- Individual keys: one active key per user, with that user's access.
- The private key downloads once; a lost or compromised key is revoked (a revoked key cannot be restored) and replaced.

## Google Play

| Rule | Since | Page |
|---|---|---|
| New apps and updates (phones, tablets) target API 36 (Android 16) | 2026-08-31, extension to 2026-11-01 | https://support.google.com/googleplay/android-developer/answer/11926878 |
| Existing apps target API 35 or higher to stay visible to new users on newer Android | 2026-08-31 | same |
| Wear OS and Android Automotive OS: target API 35; Android TV: API 34 (since 2025-08-31); Android XR: API 34 | 2026-08-31 | same |
| Apps targeting API 35+ support 16 KB memory pages on 64-bit devices; updates without it cannot be released | 2027-02-01 | https://developer.android.com/guide/practices/page-sizes |
| Apps that create accounts: account deletion started inside the app, plus a web link that works without the app, and the data deletion answers of the Data safety form | 2023-12-07 (extensions to 2024-05-31) | https://support.google.com/googleplay/android-developer/answer/13327111 |
| New personal developer accounts (created after 2023-11-13): a closed test with at least 12 testers opted in for 14 consecutive days, then "Apply for production" in the Dashboard | — | https://support.google.com/googleplay/android-developer/answer/14151465 |

16 KB pages, in practice: only apps with native code are concerned, and React Native, Expo and Flutter apps ship native libraries. AGP 8.5.1 or later aligns uncompressed libraries, NDK r28 or later compiles them aligned; AGP 8.3 to 8.5 aligns the APK but bundletool does not, so a Play-built install can fail while a local APK works. Checks: `bundletool dump config --bundle=<app>.aab | grep alignment` (`PAGE_ALIGNMENT_16K`), `zipalign -c -P 16 -v 4 <app>.apk`, and `adb shell getconf PAGE_SIZE` returning `16384` on a 16 KB emulator image.

Testing tracks (https://support.google.com/googleplay/android-developer/answer/9845334):
- Internal: up to 100 testers by email, a new bundle reaches them within minutes, and it may skip the usual policy and security review.
- Closed: email lists (up to 2,000 testers per list, 50 lists per track) or Google Groups; extra named closed tracks are possible.
- Open: anyone can join from Google Play, unlimited or capped at 1,000 or more.
- The first test link after a first publish can take several hours to work.

Play App Signing (https://support.google.com/googleplay/android-developer/answer/9842756):
- The upload key (yours, RSA 2048 or more, in a `.jks` or `.keystore`) signs what you upload; Google keeps the app signing key and signs what users install. New apps uploading bundles are enrolled by default.
- Lost or compromised upload key: create a new key, export its certificate with `keytool -export -rfc`, then "Request upload key reset" on the Play app signing page (the user does it).
- A self-managed app signing key that is lost cannot be reset.

## Review notes that are not code

- App Store guideline 2.1: give review a demo account (or an approved demo mode) and keep the backend running during review.
- 5.1.1(v): an app that creates accounts lets users delete their account from inside the app (Google Play asks the same, plus the web link above).
- 4.8: an app whose main login is a third-party or social service also offers an equivalent login that limits data to name and email, lets the user hide their email, and does not track for ads without consent.
- 3.1.1: unlocking digital content or features goes through in-app purchase; no license keys or other unlock mechanisms of your own.

Guidelines page: https://developer.apple.com/app-store/review/guidelines/

## Commands by toolchain

- EAS (https://docs.expo.dev/build/setup.md, https://docs.expo.dev/submit/introduction.md): `eas build --platform ios|android|all`, `eas submit --platform android --latest --non-interactive`, `eas submit --platform ios --path <app>.ipa`. Each submission needs a new `android.versionCode` or `ios.buildNumber`; `releaseStatus: "draft"` in the submit profile uploads without rollout.
- fastlane: `match` (certificates and profiles in an encrypted repository or bucket; `readonly` on CI; `MATCH_PASSWORD` from the secret store; types `development`, `appstore`, `adhoc`, `enterprise`), `app_store_connect_api_key(key_id:, issuer_id:, key_filepath:)` then `pilot(api_key:)` for TestFlight, `upload_to_play_store` (`supply`) with `track`, `aab`, `json_key` or `json_key_data`, `release_status`, `rollout`.
- Flutter (https://docs.flutter.dev/deployment/android, https://docs.flutter.dev/deployment/ios): `flutter build appbundle` (signing from `android/key.properties`, never committed), `flutter build ipa` (archive in `build/ios/archive/`, IPA in `build/ios/ipa`; `--build-name` and `--build-number` override `version` in `pubspec.yaml`), upload with `xcrun altool --upload-app --type ios -f build/ios/ipa/*.ipa --apiKey <key id> --apiIssuer <issuer id>` or Transporter.
- Gradle (https://developer.android.com/build/building-cmdline): `./gradlew :app:bundleRelease` with a `signingConfigs` block that reads the keystore and passwords from the environment or an untracked file, never written in `build.gradle.kts`.
