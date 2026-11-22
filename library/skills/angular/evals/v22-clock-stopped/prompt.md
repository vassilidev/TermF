---
description: After the v22 upgrade, a component with no changeDetection is OnPush; the fix moves the state to a signal (or opts into Eager).
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

We upgraded from Angular 21 to 22.3. This component stopped updating: the time shows once and never changes.

```ts
import { Component, OnDestroy } from '@angular/core';
import { DatePipe } from '@angular/common';

@Component({
  selector: 'app-clock',
  imports: [DatePipe],
  template: `<time>{{ now | date: 'HH:mm:ss' }}</time>`,
})
export class Clock implements OnDestroy {
  now = new Date();
  private id = setInterval(() => (this.now = new Date()), 1000);
  ngOnDestroy() { clearInterval(this.id); }
}
```

Why did it break, and how should it be written now?
