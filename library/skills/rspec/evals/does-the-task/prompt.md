---
description: Request specs from acceptance criteria in a Rails app that uses RSpec.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Our Rails 8.1 app uses rspec-rails 8.1 and FactoryBot. We added `PATCH /articles/:id`. Criteria:
- The author can change the title; the new title is saved.
- Another signed-in user gets 403 and the title does not change.

Sign-in in tests is done with the helper `sign_in_as(user)` from spec/support/auth.rb. Write the spec.
