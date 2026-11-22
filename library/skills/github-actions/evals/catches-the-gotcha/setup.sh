#!/bin/sh
set -e
mkdir -p .github/workflows
cat > .github/workflows/coverage.yml <<'EOF'
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
EOF
