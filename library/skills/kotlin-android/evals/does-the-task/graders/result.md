---
type: llm
---

PASS if the test drives the screen the way a user does (types into the fields found by their label, clicks "Sign in") with a fake AuthApi that rejects the password, asserts that the text "Wrong password" is displayed, and takes its rule or runner from the v2 testing APIs (androidx.compose.ui.test.junit4.v2.createComposeRule or androidx.compose.ui.test.v2.runComposeUiTest).
FAIL if it only checks the view model state without the screen, never asserts the visible message, or imports the deprecated v1 androidx.compose.ui.test.junit4.createComposeRule.
