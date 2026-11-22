---
description: A Compose UI test for an acceptance criterion on a login screen.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Acceptance criterion: "When the password is wrong, the login screen shows 'Wrong password' under the field." Our app uses Kotlin 2.4, AGP 9.5 and the Compose BOM 2026.11.01. Write the UI test for LoginScreen. The composable is:

```kotlin
@Composable
fun LoginScreen(state: LoginState, onEmail: (String) -> Unit, onPassword: (String) -> Unit, onSubmit: () -> Unit) {
    Column {
        TextField(value = state.email, onValueChange = onEmail, label = { Text("Email") })
        TextField(value = state.password, onValueChange = onPassword, label = { Text("Password") })
        state.error?.let { Text(it) }
        Button(onClick = onSubmit) { Text("Sign in") }
    }
}
```

The view model `LoginViewModel(auth: AuthApi)` sets `error = "Wrong password"` when `auth.signIn` throws `InvalidCredentials`.
