---
type: llm
---

PASS if the answer defines the task with @task from django.tasks, calls .enqueue(), and states that Django ships no worker: production needs a task backend that runs a worker (a third-party backend), because the built-in backends are for development and tests.
FAIL if it says enqueued tasks will run in production with gunicorn alone or with the built-in backends, or if it switches to Celery without mentioning the built-in framework's missing worker.
