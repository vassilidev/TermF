---
description: The config reads the run's dev server port from PORT instead of a hard-coded port, and the test waits with web-first assertions.
max_turns: 12
allowed_tools: [Read, Glob, Grep, Skill]
---
Several copies of our Next.js app now run side by side on the same machine (one per agent worktree), each dev server started by our tooling on its own port. Our Playwright tests keep hitting whichever app is on 3000. Current config:

```ts
// playwright.config.ts
import { defineConfig, devices } from '@playwright/test';
export default defineConfig({
  testDir: './e2e',
  use: { baseURL: 'http://localhost:3000', trace: 'on-first-retry' },
  webServer: { command: 'npm run dev', url: 'http://localhost:3000', reuseExistingServer: !process.env.CI },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
});
```

Fix the config so each copy tests its own server, and add a test for this acceptance criterion: "On /settings, saving a new display name shows the toast 'Profile saved' and the header shows the new name." @playwright/test is 1.65.1. Put the code in your reply.
