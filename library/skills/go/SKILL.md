---
name: go
description: >-
  Writes, changes and tests Go code the way the module's Go version expects: packages and APIs, errors and
  context, goroutines and channels, generics, encoding/json, HTTP servers and clients, go.mod and tool
  dependencies, table-driven tests with the race detector, go vet, go fix, golangci-lint and govulncheck.
  Use when a task touches .go files, go.mod, go.sum or go.work. Not for Rust or other compiled languages,
  and not for templates of other tools that merely use Go syntax.
paths: "**/*.go, **/go.mod, **/go.sum, **/go.work, **/.golangci.yml, **/.golangci.yaml"
license: MIT
metadata:
  termf-stacks: "go"
  termf-roles: "back devops qa"
  termf-versions: ">=1.22"
  termf-verified: "2026-11-22 go@1.27.4 gopls@0.24.0 golangci-lint@2.15.0"
  termf-sources: "https://go.dev/doc/go1.27 https://go.dev/doc/go1.26 https://go.dev/doc/go1.25 https://go.dev/doc/go1.24 https://go.dev/doc/toolchain https://go.dev/doc/devel/release https://go.dev/gopls/features/mcp https://go.dev/doc/security/best-practices https://golangci-lint.run/docs/product/migration-guide/"
---
# Go

The Go team ships no agent skill; `gopls` has an official MCP server (`gopls mcp`) whose usage notes print with `gopls mcp -instructions`.

1. Read the `go` line of `go.mod`: it is the minimum Go version and the language version of the module (loop variables are per iteration only from `go 1.22`), and a toolchain older than it refuses to build. Note the `toolchain` line and `go version`. In a workspace, read `go.work` too. Done when you can name the language version and the toolchain.
2. Open the release notes of that version and the ones after it (`https://go.dev/doc/go1.<minor>`) and the package docs on `pkg.go.dev`; with the gopls MCP server in the session, use its tools to find symbols and references. Done when the page for the task is open.
3. Use what the module has: its packages layout, its router, logger (`log/slog` or another), test helpers, `golangci-lint` configuration. Tools are declared with the `tool` directive in `go.mod` (`go get -tool <pkg>`, run with `go tool <name>`) since Go 1.24, instead of a `tools.go` file. Add a dependency with `go get <module>@<version>`, then `go mod tidy`. Done when `go.mod` and `go.sum` changed only through the go command.
4. Write the change: errors wrapped with `%w` and inspected with `errors.Is`, `errors.As` (or `errors.AsType` from 1.26); a `context.Context` as the first parameter of anything that does I/O, passed down and honored; every goroutine with a clear way to stop (context cancel, closed channel, `sync.WaitGroup.Go` from 1.25). Use only language features and packages of the version of step 1. Done when `go build ./...` and `go vet ./...` pass.
5. Write the tests of `## Testing`, then run `## Verify` and put each command with its result in your report. Done when they pass, or the failure is reported with its output.

## Gotchas

- Go 1.27 allows generic methods (a method declares its own type parameters); interface methods still cannot, and a generic method does not satisfy an interface method. Only in modules with `go 1.27` or later.
- Go 1.27 runs `encoding/json` on the v2 implementation: same behavior, different error texts (tests that compare error strings break); `GOEXPERIMENT=nojsonv2` reverts for now. The new `encoding/json/v2` package rejects invalid UTF-8 and duplicate names.
- New standard packages replace common dependencies: `uuid` (1.27), `crypto/mldsa` (1.27), `crypto/hpke` (1.26); `new(expr)` returns a pointer to a value (1.26); `os.Root` (1.24) confines file access to a directory.
- `go test` runs a vet subset first, which in 1.27 includes `stdversion` (a symbol newer than the `go` line is an error). 1.27 also flags `%w` applied to a pointer whose type implements `error`.
- Go 1.27 removed several GODEBUG settings (`asynctimerchan`, `tls10server`, `tlsrsakex`, `tls3des`, `x509keypairleaf`, `gotypesalias`, `tlsunsafeekm`): a `godebug` line or variable still setting them makes the go command fail.
- `go fix ./...` is the home of the modernizers (rewritten in 1.26, more in 1.27): run it only when the task is a modernization, as it touches many files.
- golangci-lint v2: the config starts with `version: "2"`, formatters (gofmt, goimports, gofumpt) live under `formatters:`, `linters.default` replaces `enable-all`/`disable-all`. `golangci-lint migrate` converts a v1 file; do it only when the task is that upgrade.
- Only the two newest Go releases are supported (1.27 and 1.26 today); say so in the report when `go.mod` targets an older one.

## Testing

Follow the skill `tests-from-criteria`: each test comes from an acceptance criterion or a real flow, at the highest level the module can run, and fails without the change.

- An HTTP endpoint is tested through its real handler and router with `net/http/httptest` (a `httptest.NewServer` or `ResponseRecorder`), and storage on a real database when the project has one for tests (a local instance or testcontainers-go), not on a mocked repository.
- Several cases of one rule: a table of cases, each run with `t.Run(tc.name, ...)`; `t.Context()` (1.24) for contexts cancelled at the end of the test.
- Code that waits on timers or goroutines: `testing/synctest` (`synctest.Test`, 1.25) instead of `time.Sleep`.
- Always `-race` for code with goroutines. One test while iterating: `go test -run '^TestRefund$/over_limit' -count=1 ./orders`.

## Verify

- The project's commands first (`Makefile`, `magefile`, `Taskfile`, CI steps).
- `gofmt -l .` (prints nothing), `go vet ./...`, `go build ./...`, `go test -race ./...`; `golangci-lint run` when the project has a config; `govulncheck ./...` when a dependency changed.

## Security

- `os/exec` runs the program directly, without a shell: pass arguments as separate strings, and never build a `sh -c` line from input.
- HTML output goes through `html/template`; `text/template` does not escape.
- A path that comes from input is opened through `os.Root` / `os.OpenInRoot`, which blocks `..` and symlink escapes.
- Tokens and keys come from `crypto/rand` (`rand.Text()` for a random string), never `math/rand`.
- `httputil.ReverseProxy`: `Rewrite`, not the deprecated `Director`, so hop-by-hop headers cannot be smuggled.

## References

- gopls MCP: `claude mcp add gopls -- gopls mcp` (detached mode sees saved files only); suggest it in your report when it is missing on a large module.
