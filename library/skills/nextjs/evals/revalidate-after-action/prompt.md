---
description: A Server Action on Next 16 uses updateTag, or revalidateTag with its required second argument, after a write.
max_turns: 12
allowed_tools: [Read, Glob, Grep, Skill]
---
Next.js 16.4 app (App Router). The profile page reads the user with `fetch(url, { next: { tags: ['user-' + id] } })`.

Write the Server Action `saveProfile(formData)` in `app/profile/actions.ts`: it calls `db.users.update(id, data)` and the user must see the new values as soon as the form is submitted. Reply with the file.
