---
description: Background work with Django's built-in tasks framework.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

We are on Django 6.1, deployed with gunicorn only (no Celery, no RQ). I want the welcome email to be sent in the background after signup using Django's new built-in tasks. Write the task, the call in the signup view, and tell me what else production needs.
