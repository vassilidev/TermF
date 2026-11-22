---
description: Bundler 4 removed the old bundle install flags.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

Our Dockerfile for a Rails 8.1 app on Ruby 4.0 does `RUN bundle install --deployment --without development test` and the build now fails with Bundler 4.1. Rewrite those lines.
