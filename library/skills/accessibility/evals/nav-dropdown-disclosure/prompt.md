---
description: A site navigation dropdown built with role="menu" is rebuilt as links with a disclosure button.
max_turns: 12
allowed_tools: [Read, Glob, Grep, Skill]
---
Screen reader users say our header navigation is confusing: arrow keys do nothing and they hear "menu" everywhere. Here's the markup (Vue 3, the open/close is handled by `toggle()`):

```vue
<nav>
  <ul role="menubar">
    <li role="none"><a role="menuitem" href="/">Home</a></li>
    <li role="none">
      <span role="menuitem" @click="toggle()">Products</span>
      <ul role="menu" v-show="open">
        <li role="none"><a role="menuitem" href="/products/cameras">Cameras</a></li>
        <li role="none"><a role="menuitem" href="/products/lenses">Lenses</a></li>
      </ul>
    </li>
    <li role="none"><a role="menuitem" href="/contact">Contact</a></li>
  </ul>
</nav>
```

What's the right pattern here? Give me the corrected template.
