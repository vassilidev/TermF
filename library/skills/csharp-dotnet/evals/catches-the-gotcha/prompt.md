---
description: Running one test class when global.json opts into Microsoft.Testing.Platform with xUnit v3.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

Our global.json is:

```json
{ "sdk": { "version": "10.0.100", "rollForward": "latestFeature" }, "test": { "runner": "Microsoft.Testing.Platform" } }
```

The tests are in tests/Acme.Orders.Tests (xUnit v3, package xunit.v3). Give me the exact command to run only the tests of the class Acme.Orders.Tests.OrderTotalsTests.
