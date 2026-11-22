---
description: A first terraform test; the outdated answer writes run blocks that apply for real.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Write a `terraform test` file for this module that checks the bucket name gets the environment prefix. We run tests in CI on every pull request.

```hcl
variable "environment" { type = string }
variable "name" { type = string }

resource "aws_s3_bucket" "this" {
  bucket = "${var.environment}-${var.name}"
}
```
