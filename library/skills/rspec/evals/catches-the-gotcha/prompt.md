---
description: Minitest 6 extracted its mocking library.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

After `bundle update` our Rails 8.1 test suite fails with `cannot load such file -- minitest/mock` in test/services/payment_test.rb. Gemfile.lock now has minitest 6.1.0. What is the fix?
