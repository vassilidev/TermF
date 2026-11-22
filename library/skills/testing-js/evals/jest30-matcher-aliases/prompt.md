---
description: After the Jest 30 upgrade, removed matcher aliases are replaced by their primary names.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---
Upgraded jest from 29.7 to 30.5.2 and a bunch of tests now crash with `TypeError: expect(...).toBeCalledWith is not a function`. Example:

```js
test("sends the welcome email once", async () => {
  await signup({ email: "alice@example.test" });
  expect(mailer.send).toBeCalledTimes(1);
  expect(mailer.send).toBeCalledWith(expect.objectContaining({ to: "alice@example.test" }));
  await expect(signup({ email: "" })).rejects.toThrowError("email required");
});
```

What's wrong, and what's the corrected test?
