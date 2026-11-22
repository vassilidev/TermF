---
description: On Tailwind v4, @apply inside a Vue SFC style block is fixed with @reference to the main stylesheet.
max_turns: 12
allowed_tools: [Read, Glob, Grep, Skill]
---
Vue 3 + Vite app with tailwindcss 4.3 and `@tailwindcss/vite`. `src/assets/main.css` has `@import "tailwindcss";` and a `@theme` block defining `--color-brand`.

This component fails to build with "Cannot apply unknown utility class `text-brand`":

```vue
<template><h2 class="title">Orders</h2></template>
<style scoped>
.title { @apply text-brand font-semibold; }
</style>
```

How do I fix it without moving the styles out of the component? Reply with the corrected file.
