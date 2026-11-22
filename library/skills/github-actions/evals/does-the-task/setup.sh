#!/bin/sh
set -e
mkdir -p .github/workflows
cat > package.json <<'EOF'
{ "name": "web", "private": true, "packageManager": "pnpm@10.20.0", "scripts": { "lint": "eslint .", "test": "vitest run" } }
EOF
printf "lockfileVersion: '9.0'\n" > pnpm-lock.yaml
cat > .github/workflows/release.yml <<'EOF'
name: release
on:
  push:
    tags: ["v*"]
jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - run: echo "build and publish"
EOF
