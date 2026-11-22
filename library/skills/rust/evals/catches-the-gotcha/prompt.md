---
description: Denying warnings in a Rust check without invalidating the build cache.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

Our CI runs `cargo build` and then `cargo clippy --all-targets -- -D warnings`, and clippy rebuilds everything every time. We use Rust 1.100. What command should CI run so clippy fails on any warning without the rebuild?
