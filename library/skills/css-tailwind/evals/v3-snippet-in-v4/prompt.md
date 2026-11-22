---
description: Classes copied from a v3 project are rewritten for Tailwind v4 (opacity modifier, outline-hidden, renamed scale).
max_turns: 12
allowed_tools: [Read, Glob, Grep, Skill]
---
Our app uses tailwindcss 4.3.5 (`@import "tailwindcss";` in `src/app.css`, tokens in `@theme`). I copied this card from an older project and it doesn't look the same here:

```html
<div class="shadow rounded border bg-white bg-opacity-90 p-4">
  <button class="outline-none focus:ring px-3 py-1">Open</button>
</div>
```

Give me the markup with the classes that produce the old look in our Tailwind version.
