---
description: A spy called in beforeAll reads 0 calls after the Vitest 5 upgrade because clearMocks is now on by default.
max_turns: 12
allowed_tools: [Read, Glob, Grep, Skill]
---

We moved from vitest 4.1 to 5.1.1 yesterday and this test started failing with `expected "track" to be called 1 times, but got 0 times`. No config change, `vitest.config.ts` only sets `environment: "node"`.

```ts
import { beforeAll, describe, expect, test, vi } from "vitest";
import { analytics } from "../src/analytics";
import { bootApp } from "../src/app";

const track = vi.spyOn(analytics, "track");

describe("boot", () => {
  beforeAll(async () => {
    await bootApp();
  });

  test("sends one app_started event", () => {
    expect(track).toHaveBeenCalledTimes(1);
    expect(track).toHaveBeenCalledWith("app_started");
  });
});
```

bootApp definitely calls `analytics.track("app_started")` once. Why does it fail now, and how should the test be written?
