---
name: csharp-dotnet
description: >-
  Builds, changes and tests C# and .NET code the way the project's target framework expects: projects and
  solutions, ASP.NET Core endpoints, EF Core queries, xUnit, MSTest or NUnit tests, NuGet packages,
  global.json and test runner settings. Use when a task touches .cs, .csproj, .sln or .slnx files,
  global.json, Directory.Build.props or Directory.Packages.props. Not for Java or Kotlin projects, and it
  leaves a topic to Microsoft's dotnet/skills plugins when they are installed.
paths: "**/*.cs, **/*.csproj, **/*.sln, **/*.slnx, **/global.json, **/Directory.Build.props, **/Directory.Packages.props"
license: MIT
metadata:
  termf-stacks: "dotnet aspnetcore"
  termf-roles: "back qa"
  termf-versions: ">=10"
  termf-verified: "2026-11-22 dotnet@11.0.0 dotnet@10.0.15 csharp@15 xunit.v3@4.0.3"
  termf-sources: "https://github.com/dotnet/skills https://learn.microsoft.com/en-us/training/support/mcp https://learn.microsoft.com/en-us/dotnet/core/whats-new/dotnet-11/overview https://learn.microsoft.com/en-us/dotnet/csharp/whats-new/csharp-15 https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-test-mtp https://dotnet.microsoft.com/en-us/platform/support/policy/dotnet-core"
---
# C# and .NET

Microsoft publishes evaluated .NET skills in [dotnet/skills](https://github.com/dotnet/skills) (MIT, Claude Code marketplace `dotnet-agent-skills`: plugins `dotnet`, `dotnet-test`, `dotnet-data`, `dotnet-aspnetcore`, `dotnet-upgrade`, `dotnet-msbuild`…). Nothing ships inside the SDK or the NuGet packages. When one of those plugins is loaded in the session, follow it for its topic; this skill keeps the version facts, the gotchas and the checks.

1. Read the target framework of each project you touch (`<TargetFramework>` or `<TargetFrameworks>` in its `.csproj`, for example `net10.0`) and the SDK pin in `global.json` (`sdk.version`, `rollForward`, and `test.runner`). Done when you can name the TFM, the SDK and the test runner mode.
2. Open the docs of that version: Microsoft Learn pages have a version selector; when the session has the Microsoft Learn MCP server (`https://learn.microsoft.com/api/mcp`, plugin `microsoft-docs@claude-plugins-official`), ask it. Done when the page for the task is open.
3. Use what the solution already has: its test framework, analyzers, `Directory.Build.props`, central package management (with `Directory.Packages.props`, a package version goes there and the `PackageReference` has no version). Done when every package and command you plan comes from the solution.
4. Write the change with nullable on: fix the null path instead of adding `!`, and keep async all the way (`await`, a `CancellationToken` passed down, no `.Result` or `.Wait()`), because a blocked thread pool hangs ASP.NET Core under load. Done when the build shows no warning your change introduced.
5. Write the tests of `## Testing`, then run `## Verify` and put each command with its result in your report. Done when they pass, or the failure is reported with its output.

## Gotchas

- .NET 8 and .NET 9 went out of support on 2026-11-10. Supported: .NET 10 (LTS) and .NET 11 (STS), both until 2028-11-14. Keep the project's TFM unless the task is an upgrade; say in the report when it is out of support.
- C# 15 is the default only for `net11.0`: `union` types with exhaustive `switch`, `closed` class hierarchies, indexers in `extension` blocks, collection expression arguments (`[with(capacity: n), ..]`), labeled `break`/`continue`. On `net10.0` the language is C# 14: write none of these there.
- `dotnet test` still defaults to VSTest. Microsoft.Testing.Platform (MTP) is opted in by `global.json` (`"test": { "runner": "Microsoft.Testing.Platform" }`, SDK 10+) or by `DOTNET_TEST_RUNNER` (.NET 11). In MTP mode the options come from the test framework's extensions: xUnit v3 filters with `--filter-class`, `--filter-method`, `--filter-namespace`, `--filter-trait`; the VSTest `--filter "FullyQualifiedName~X"` is not an xUnit v3 option, and an option a test project does not know fails the run with exit code 5.
- xUnit v3 is the package `xunit.v3` (4.x today, on MTP), not `xunit` 2.x; the .NET 11 test templates use it. Keep the version the solution has.
- File-based apps (`dotnet run app.cs`, `#:package`, since the .NET 10 SDK) suit one-off scripts; a script that the solution builds belongs in a project.
- `BinaryFormatter` throws since .NET 9 and is gone for good: serialize with System.Text.Json.

## Testing

Follow the skill `tests-from-criteria`: each test comes from an acceptance criterion or a real flow, at the highest level the solution can run, and fails without the change.

- HTTP behavior: an integration test through `WebApplicationFactory<Program>` (package `Microsoft.AspNetCore.Mvc.Testing`), calling the endpoint with its `HttpClient`.
- Data: the database engine production uses (a local instance or Testcontainers); Microsoft discourages the EF Core in-memory provider (no transactions, no raw SQL, different query behavior), and mocking `DbContext` proves nothing about queries.
- One test while iterating: under MTP with xUnit v3, `dotnet test --project <test.csproj> --filter-method "<Namespace.Class.Method>"`; under VSTest, `dotnet test <test.csproj> --filter "FullyQualifiedName~<Class>"`.

## Verify

- The solution's own commands first (build script, `Makefile`, CI workflow steps).
- `dotnet build` (no new warnings), `dotnet test`, `dotnet format --verify-no-changes`.

## Security

- `FromSqlRaw` and `ExecuteSqlRaw` with a concatenated or formatted string are SQL injection; use `FromSql` / `ExecuteSql` with an interpolated string, which EF Core turns into parameters.
- Antiforgery: minimal API forms need `AddAntiforgery()` and `UseAntiforgery()`; `DisableAntiforgery()` only on endpoints a browser cannot reach; MVC uses `[AutoValidateAntiforgeryToken]`. ASP.NET Core 11 also rejects cross-site form posts (`Sec-Fetch-Site`, `Origin`) with 400, so a test that posts cross-origin fails for that reason.
- Secrets stay out of `appsettings*.json`: `dotnet user-secrets` in development (stored unencrypted, development only), environment or a vault elsewhere.
- Deserialize untrusted input into concrete types with System.Text.Json; never revive a type name taken from the payload.

## References

- Read the matching `dotnet/skills` skill when it is installed and the task is an upgrade (`dotnet-upgrade`), EF Core query performance (`dotnet-data`), MSBuild or NuGet trouble (`dotnet-msbuild`, `dotnet-nuget`); when it is not installed, name it in your report as a suggestion (`/plugin marketplace add dotnet/skills`).
- Read `https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-test-mtp` before composing a `dotnet test` command in MTP mode.
