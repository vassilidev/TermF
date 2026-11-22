---
description: A dynamic App Router page on Next 16 awaits params and searchParams instead of reading them synchronously.
max_turns: 12
allowed_tools: [Read, Glob, Grep, Skill]
---
My package.json has "next": "16.4.6", "react": "19.3.1" and I use the App Router.

Write `app/blog/[slug]/page.tsx`: it shows the post title from `getPost(slug)` (already in `lib/posts.ts`) and highlights the comment whose id is in the `?comment=` query string. Give me the whole file in your reply.
