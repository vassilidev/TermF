---
description: CDK library and CLI versions differ on purpose; the outdated answer says to align them.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

A teammate noticed that our package.json has `"aws-cdk-lib": "2.278.0"` but `"aws-cdk": "2.1149.0"`, and thinks one of them is a typo. Should we set both to the same version before we run `cdk synth`?
