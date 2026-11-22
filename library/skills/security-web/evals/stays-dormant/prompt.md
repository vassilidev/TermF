---
description: A plain refactoring request with no security angle.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

Rewrite this function with async/await instead of promise chains, keeping the behaviour the same:

```js
function loadUser(id) {
  return fetchUser(id)
    .then((user) => fetchTeam(user.teamId).then((team) => ({ ...user, team })))
    .catch(() => null);
}
```
