---
description: Feature tests from acceptance criteria in a Laravel 13 app with Pest.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Our Laravel 13 app uses Pest 4 (tests/Pest.php exists). We just added `PATCH /projects/{project}` that renames a project. Acceptance criteria:
- The owner can rename the project; the new name is saved.
- Another user gets 403 and the name does not change.
- An empty name is refused with a validation error on `name`.

`Project` and `User` have factories; `Project` belongs to `User` through `user_id`. Write tests/Feature/ProjectRenameTest.php.
