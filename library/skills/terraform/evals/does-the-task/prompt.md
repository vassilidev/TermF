---
description: Variables with validation and a database password that should stay out of the state.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

In our Terraform module (Terraform 1.17, AWS provider 6.x) I need two inputs: `environment`, which must be one of dev, staging or prod, and the master password of the `aws_db_instance.main` we already have. Security says the password must not end up in the Terraform state. Show me the variables and the change to the resource.
