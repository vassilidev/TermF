---
description: An Ansible question, configuration management this skill does not cover.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

Our Ansible playbook runs `shell: useradd deploy` and fails on the second run because the user already exists. How do I make that task idempotent?
