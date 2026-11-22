---
description: In Nuxt 4, useFetch data starts as undefined, not null; the fix handles undefined.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

package.json has `"nuxt": "^4.6.4"`, `"vue": "^3.5.46"`. In `app/pages/products/[id].vue`:

```vue
<script setup lang="ts">
const route = useRoute()
const { data } = await useFetch(`/api/products/${route.params.id}`)
</script>

<template>
  <p v-if="data === null">Product not found</p>
  <ProductCard v-else :product="data" />
</template>
```

When the API returns nothing, "Product not found" never shows and ProductCard crashes. Why, and what is the fixed file?
