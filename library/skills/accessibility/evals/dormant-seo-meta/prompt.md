---
description: Social sharing metadata is SEO work, not accessibility.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---
Add Open Graph and Twitter card tags to our Next.js 16 blog post page (`app/blog/[slug]/page.tsx`) so shared links show the post title, excerpt and cover image. The post comes from `getPost(slug)` which returns `{ title, excerpt, coverUrl }`.
