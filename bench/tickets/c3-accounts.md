# Accounts: each user sees only their own tasks

**Type:** feature, security sensitive (authentication, access rights, input validation)

The app is about to be shared by several people. Each person needs an account, and must only ever see and change their own tasks.

## Acceptance criteria

1. `POST /api/users` with `{"username", "password"}` creates an account and answers `201` with `{"id": <number>, "username": "<name>"}`.
   - A username is lowercased first (stored and compared in lowercase); then it must be 3 to 32 characters among `a-z`, `0-9` and `_`, otherwise `400` `{"error": "invalid username"}`.
   - A username already taken (in any case) answers `409` `{"error": "username taken"}`.
   - A password has at least 10 characters; otherwise `400` `{"error": "password too short"}`.
2. `POST /api/sessions` with `{"username", "password"}` signs in and answers `201` with `{"token": "<opaque string>"}`. A wrong username or password answers `401` `{"error": "invalid credentials"}`, the same answer in both cases.
3. Every `/api/tasks` route needs `Authorization: Bearer <token>`. A missing, malformed or unknown token answers `401` `{"error": "authentication required"}`.
4. A task belongs to the user who created it. A user lists only their own tasks; reading, changing or deleting someone else's task answers `404` `{"error": "task not found"}`, as for a task that does not exist.
5. `DELETE /api/sessions` with the token signs out (`204`); the token stops working.
6. Passwords are stored salted and hashed with scrypt (`node:crypto`) and compared in constant time. Tokens are random (at least 32 bytes from the OS's random source) and only their hash is stored. No response ever contains a password or a hash.
7. Tasks created before accounts existed stay in the database but no user sees them.
8. The page has a sign-up and sign-in form, sends the token with every request, and has a sign-out button.
9. Tests cover the API (the existing ones are updated to sign in); the README documents accounts and authentication.
