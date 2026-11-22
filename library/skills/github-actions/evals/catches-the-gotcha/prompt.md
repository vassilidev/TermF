---
description: A pull_request_target workflow broken by checkout v7; the outdated fix downgrades checkout or opts back into the unsafe checkout.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Since last week this workflow fails on pull requests from forks with an error from actions/checkout saying it refuses to check out fork pull request code. It used to work. Can you fix it? It runs our tests and then posts the coverage summary as a PR comment.

```yaml
name: coverage
on: pull_request_target
permissions:
  contents: read
  pull-requests: write
jobs:
  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
        with:
          ref: ${{ github.event.pull_request.head.sha }}
      - run: npm ci && npm test -- --coverage
      - run: gh pr comment ${{ github.event.pull_request.number }} --body-file coverage/summary.md
        env:
          GH_TOKEN: ${{ github.token }}
```
