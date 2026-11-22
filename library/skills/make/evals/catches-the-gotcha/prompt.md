---
description: A Makefile that relies on .ONESHELL, which the GNU Make 3.81 of macOS ignores without a word.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

Our Makefile starts with `.ONESHELL:` so the `build` recipe can `cd build` on one line and run `cmake ..` on the next. It works in CI on Ubuntu, but on my Mac cmake runs in the repository root and fails. Why, and what should I change?
