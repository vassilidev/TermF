---
name: terraform
description: >-
  Writes and changes Terraform or OpenTofu code without touching real infrastructure: modules, variables with
  validation, providers and the lock file, backends, moved and import blocks, ephemeral values and write-only
  arguments for secrets, terraform test, formatting and linting. Use when a task touches *.tf, *.tofu,
  *.tftest.hcl, *.tfquery.hcl, .terraform.lock.hcl or terraform and tofu commands. Not for Kubernetes manifests
  (kubernetes), nor for cloud consoles and SDK code (cloud).
paths: "**/*.tf, **/*.tofu, **/*.tftest.hcl, **/*.tfquery.hcl, **/*.tfvars, **/.terraform.lock.hcl"
license: MIT
metadata:
  termf-stacks: "terraform opentofu"
  termf-roles: "devops"
  termf-versions: ">=1.5"
  termf-verified: "2026-11-22 terraform@1.17.2 opentofu@1.13.9 tflint@0.66.0 hashicorp/agent-skills@main"
  termf-sources: "https://github.com/hashicorp/terraform/releases https://github.com/hashicorp/terraform/blob/v1.17/CHANGELOG.md https://github.com/opentofu/opentofu/releases/tag/v1.13.0 https://developer.hashicorp.com/terraform/language/upgrade-guides https://github.com/hashicorp/agent-skills https://github.com/terraform-linters/tflint"
---
# Terraform and OpenTofu

HashiCorp publishes official skills in `hashicorp/agent-skills` (MPL-2.0): `terraform-style-guide`, `terraform-test`, `refactor-module`, `terraform-search-import`, `terraform-stacks`, `terraform-policy`, and provider-development skills. This skill stays thin: the version traps, the safety boundary and the checks.

1. Read which tool and version the repository targets: `required_version` in the `terraform {}` block, `.terraform-version` or `.opentofu-version`, `.tofu` files, and the binary CI calls (`terraform` or `tofu`). Done when you can name the tool and the version range.
2. If an official HashiCorp skill for the task is loaded (`terraform-style-guide`, `terraform-test`, `refactor-module`…), follow it for conventions and keep this skill for the traps and the boundary below. If not, carry on here and name `hashicorp/agent-skills` in your report so the user can import it. Done when you know which guidance you follow.
3. Keep the module layout, naming, provider versions and backend the repository already has, because a second style splits the code. Done when every file you touch already existed or the task asked for it.
4. Write the change: typed variables with `validation` blocks, `sensitive = true` on secrets, `moved` blocks for renames and `import` blocks for adoption instead of state commands, version constraints on every provider and module source. Done when `terraform fmt -check -recursive` and `terraform validate` pass.
5. Run the commands of `## Verify` and put each one with its result in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- `terraform test` applies for real by default: a `run` block without `command = plan` creates and destroys infrastructure with the configured credentials. Write unit tests with `command = plan` and `mock_provider` blocks, and run a test file with apply runs only when the user says so.
- `terraform plan` reads the backend and refreshes every resource with real credentials. Without them, validate offline: `terraform init -backend=false` then `terraform validate`.
- State stores every attribute in plain JSON, `sensitive` values included. Since 1.10 use `ephemeral` variables and resources for secrets that must not persist, and since 1.11 the provider's write-only arguments (named `*_wo`, with a `*_wo_version` to trigger updates) to pass them to resources.
- The S3 backend locks natively with `use_lockfile = true` (GA in 1.11); the DynamoDB arguments are deprecated. Keep what the repository uses and name the migration in your report.
- 1.14 added `terraform query` with `list` blocks in `*.tfquery.hcl`, and `action` blocks that providers run during apply; an action has side effects outside the resource graph, so treat a new one like an apply.
- 1.15 lets variables and outputs carry `deprecated = "…"`; 1.17 allows variables and locals inside `required_providers`, adds `plan -minimal-refresh`, and makes Terraform Policy (`-policies`) generally available.
- OpenTofu forked from Terraform 1.5 and has diverged since 1.6: check the OpenTofu docs of the installed version before using a Terraform feature newer than 1.5, and keep `.tofu` files (which override the `.tf` file of the same name) if the repository has them. OpenTofu 1.13 drops WinRM provisioners and is the last series with 32-bit builds.

## Verify

- The project's own targets first (`make lint`, pre-commit hooks, a CI job).
- `terraform fmt -check -recursive` (or `tofu fmt …`)
- `terraform init -backend=false && terraform validate`
- `terraform test` only for files whose runs all use `command = plan` or mocked providers.
- `tflint --init && tflint --recursive` when the repository has a `.tflint.hcl`; `checkov -d .` or `trivy config .` when the project uses them.

## Security

- `apply`, `destroy`, `import` on a real backend, `state rm|mv|push`, `force-unlock` and `taint` change real infrastructure or its record: never run them; describe the command in your report for the user to run.
- No secret in `.tf`, `.tfvars` committed to git, or a provider block; read credentials from the environment or the provider's own chain.
- Commit `.terraform.lock.hcl` with hashes for every platform the team uses: `terraform providers lock -platform=linux_amd64 -platform=darwin_arm64`.
- Pin module sources to a version or a commit (`?ref=` with a SHA for Git sources); a branch ref runs whatever lands on it.
- Make outputs that carry secrets `sensitive = true`, and say in your report which values still reach the state.

## References

- Official skills: https://github.com/hashicorp/agent-skills (`claude plugin marketplace add hashicorp/agent-skills` then `claude plugin install terraform@hashicorp`).
- Version notes: https://github.com/hashicorp/terraform/releases and the upgrade guides https://developer.hashicorp.com/terraform/language/upgrade-guides ; OpenTofu: https://opentofu.org/docs/
