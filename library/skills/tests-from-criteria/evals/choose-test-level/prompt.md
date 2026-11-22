---
description: An access criterion is tested through the HTTP layer of the app, not by unit-testing the helper.
max_turns: 12
allowed_tools: [Read, Glob, Grep, Skill]
---
Ticket acceptance criterion: "A signed-out visitor who opens /account is redirected to /login."

Our Express app already has integration tests with supertest (`test/http/*.test.js`, run by `npm test`). The redirect is done by this middleware:

```js
// src/auth.js
export const isAuthenticated = (req) => Boolean(req.session?.userId);
export function requireLogin(req, res, next) {
  if (!isAuthenticated(req)) return res.redirect(302, "/login");
  next();
}
// src/app.js: app.get("/account", requireLogin, accountPage);
```

What test(s) should I add for this criterion? Show the code in your reply.
