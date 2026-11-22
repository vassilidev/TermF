---
description: An edition 2024 migration error on environment variables in a Rust crate.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

We moved our crate to `edition = "2024"`. Its test helper no longer compiles:

```rust
pub fn with_log_level<F: FnOnce()>(level: &str, f: F) {
    std::env::set_var("RUST_LOG", level);
    f();
    std::env::remove_var("RUST_LOG");
}
```

The tests run in parallel with `cargo test`. Fix it properly.
