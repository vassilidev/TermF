---
name: rust
description: >-
  Writes, changes and tests Rust code the way the crate's edition and toolchain expect: modules and public APIs,
  error types, ownership and lifetimes, async code on the project's runtime, unsafe code, Cargo features and
  workspaces, unit, integration and doc tests, rustfmt, clippy and supply-chain checks. Use it before answering
  whenever a task or a question is about Rust: .rs files, Cargo.toml, Cargo.lock, rust-toolchain.toml, or the
  cargo, clippy and rustfmt commands that build, lint and test them. Not for Go or C and C++ code, and not for
  JavaScript bindings beyond their Rust side.
paths: "**/*.rs, **/Cargo.toml, **/Cargo.lock, **/rust-toolchain.toml, **/rust-toolchain, **/clippy.toml, **/rustfmt.toml, **/deny.toml"
license: MIT
metadata:
  termf-stacks: "rust"
  termf-roles: "back devops qa"
  termf-versions: ">=1.85"
  termf-verified: "2026-11-22 rust@1.100.0 cargo-nextest@0.9.152"
  termf-sources: "https://blog.rust-lang.org/2026/11/12/Rust-1.100.0/ https://blog.rust-lang.org/2026/07/09/Rust-1.97.0/ https://doc.rust-lang.org/edition-guide/rust-2024/index.html https://doc.rust-lang.org/cargo/reference/rust-version.html https://github.com/rust-lang/rust-clippy https://nexte.st/docs/running/"
---
# Rust

The Rust project ships no agent skill, `llms.txt` or MCP server; `rust-analyzer` is the language server.

1. Read `edition`, `rust-version` and `[workspace] resolver` in `Cargo.toml`, the pinned toolchain in `rust-toolchain.toml` (or `rust-toolchain`), and the active one with `rustup show`. `rust-version` is enforced: a toolchain older than it fails the build. Done when you can name the edition, the minimum Rust version and the toolchain.
2. Open the docs of that version: the release posts on `https://blog.rust-lang.org/releases/`, the edition guide, and the docs of each crate at its locked version on `docs.rs`. Done when the page for the task is open.
3. Use what the workspace has: its async runtime, error crates (`thiserror`, `anyhow` or its own), logging, test helpers, lint settings in `[lints]` and `clippy.toml`. Add a dependency with `cargo add <crate>` in the crate that needs it (or to `[workspace.dependencies]` when the workspace uses it), and touch only the features you need. Done when `Cargo.lock` changed only through cargo.
4. Write the change: libraries return typed errors and the binary decides how to report them; `unwrap`/`expect` only for invariants the code guarantees, with the reason in the `expect` message; each `unsafe` block carries a `// SAFETY:` comment saying why it holds. Use only features of the minimum Rust version of step 1. Done when `cargo build` shows no new warning.
5. Write the tests of `## Testing`, then run `## Verify` and put each command with its result in your report. Done when they pass, or the failure is reported with its output.

## Gotchas

- Edition 2024 (stable since 1.85): `extern` blocks are `unsafe extern` (items inside may be marked `safe`); `#[no_mangle]`, `#[export_name]` and `#[link_section]` become `#[unsafe(...)]`; `std::env::set_var`, `remove_var` and `CommandExt::before_exec` are unsafe; references to `static mut` are denied; `unsafe_op_in_unsafe_fn` warns. `cargo fix --edition` migrates.
- Edition 2024: return-position `impl Trait` captures every lifetime in scope (narrow it with `use<'a, T>`); `if let` temporaries drop before `else`; tail-expression temporaries drop earlier; `gen` is reserved (`r#gen`); let chains (`if let Some(x) = a && x > 0`) work in `if` and `while`.
- Edition 2024 implies `resolver = "3"` (dependency versions chosen to fit `rust-version`), but a virtual workspace sets `resolver` itself.
- Rust 1.100: the never type `!` is stable (`Infallible` is an alias for it), and its fallback changed in older editions, so the compiler may now flag more code as unreachable. The core `Allocator` API is stable. Cargo's `unused_dependencies` lint warns by default (`[lints.cargo]`) when every target is built.
- Since Cargo 1.97, deny warnings with `CARGO_BUILD_WARNINGS=deny` (or `build.warnings = "deny"`), which keeps the build cache; `-- -D warnings` still works but rebuilds. Older toolchains ignore the variable: use `-D warnings` there.
- `cargo fmt` reads the edition from `Cargo.toml`, plain `rustfmt` defaults to style edition 2015: format through `cargo fmt`, or set `style_edition` in `rustfmt.toml`.

## Testing

Follow the skill `tests-from-criteria`: each test comes from an acceptance criterion or a real flow, at the highest level the workspace can run, and fails without the change.

- Behavior seen from outside the crate goes in `tests/` (integration tests against the public API, or the binary run as a process); logic with real rules gets unit tests in a `#[cfg(test)] mod tests` next to it; examples in docs are compiled and run as doc tests.
- Files and processes through temporary directories, never the repository; async tests with the runtime's test attribute (`#[tokio::test]` when the project uses tokio).
- With cargo-nextest in the project: `cargo nextest run`, plus `cargo test --doc`, because nextest does not run doc tests.
- One test while iterating: `cargo test -p <crate> refund_over_limit` (a name filter), or `cargo test --test <file> -- <module>::<name> --exact`.

## Verify

- The project's commands first (its `CLAUDE.md`, `Makefile`, `justfile`, CI steps).
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features` with warnings denied, `cargo test --workspace`; `--locked` in CI-like runs; `cargo deny check` or `cargo audit` when the project has them and a dependency changed.

## Security

- Integer overflow wraps silently in release builds: use `checked_*`, `saturating_*` or `wrapping_*` where the value comes from input or money.
- `std::process::Command` passes arguments without a shell, except on Windows for `.bat` and `.cmd` files, where an argument can run commands: never pass untrusted input to a batch file.
- Parse input into `Result`; a `panic` on malformed input is a denial of service in a server.
- Every new dependency is a supply-chain decision: prefer crates the workspace already uses, keep `Cargo.lock` committed, and run `cargo deny check` or `cargo audit` when configured; `registry.global-min-publish-age` (1.100) delays freshly published versions.

## References

- `https://doc.rust-lang.org/edition-guide/rust-2024/` before an edition migration; the release post of each version between the locked toolchain and the target before a toolchain bump.
