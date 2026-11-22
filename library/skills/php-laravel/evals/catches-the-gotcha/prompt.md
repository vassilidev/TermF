---
description: The CSRF middleware name after Laravel 13 renamed it.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

In our Laravel 13 feature tests we still have `$this->withoutMiddleware(\Illuminate\Foundation\Http\Middleware\VerifyCsrfToken::class);` and the upgrade notes mention a rename. What should this line be now?
