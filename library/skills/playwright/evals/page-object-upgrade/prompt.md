---
description: After the 1.65 upgrade a page object breaks; the answer names useDefineForClassFields and moves the locators into the constructor.
max_turns: 12
allowed_tools: [Read, Glob, Grep, Skill]
---
We bumped @playwright/test from 1.64.1 to 1.65.1 and now every test using this page object dies with `TypeError: Cannot read properties of undefined (reading 'getByRole')`. Nothing else changed. tsconfig has `"target": "ES2022"`.

```ts
// e2e/pages/login-page.ts
import type { Page } from '@playwright/test';

export class LoginPage {
  email = this.page.getByRole('textbox', { name: 'Email' });
  password = this.page.getByLabel('Password');
  submit = this.page.getByRole('button', { name: 'Sign in' });

  constructor(private readonly page: Page) {}

  async login(email: string, password: string) {
    await this.email.fill(email);
    await this.password.fill(password);
    await this.submit.click();
  }
}
```

Why did the upgrade break it, and what's the fix? Reply with the corrected class.
