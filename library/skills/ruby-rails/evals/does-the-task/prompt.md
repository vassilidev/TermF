---
description: A controller action with strong parameters and a sortable list in Rails 8.1.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Our Rails 8.1 app lists articles. Add to ArticlesController:
- `index`, sortable by `params[:sort]` (title or published_at) and showing each article's author name and comment count;
- `update`, which lets an author change title and body only.

Show the controller code.
