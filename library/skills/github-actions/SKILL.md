---
name: github-actions
description: >-
  Writes, fixes and hardens GitHub Actions workflows: CI for pull requests, release and deploy jobs, caching,
  matrices, reusable workflows, cloud credentials through OIDC, pinning actions, the workflow lockfile, token
  permissions, untrusted input from issues and pull requests. Use when a task touches .github/workflows/*.yml,
  an action.yml, dependabot.yml for actions, or asks why a workflow fails or is unsafe. Not for Dockerfiles
  (docker), Kubernetes manifests (kubernetes) or other CI systems.
paths: "**/.github/workflows/*.y*ml, **/.github/workflows/actions.lock, **/action.y*ml, **/.github/dependabot.y*ml"
license: MIT
metadata:
  termf-stacks: "github-actions"
  termf-roles: "devops secops"
  termf-versions: "*"
  termf-verified: "2026-11-22 actions/checkout@v7.0.1 zizmor@1.32.0 actionlint@1.7.12 gh-actions-lock@main"
  termf-sources: "https://docs.github.com/llms.txt https://docs.github.com/en/actions/reference/security/secure-use https://docs.github.com/en/actions/concepts/security/workflow-dependency-lockfiles https://github.blog/changelog/2026-10-28-github-actions-workflow-lockfiles-are-generally-available https://github.blog/changelog/2026-11-02-pull_request_target-blocked-by-default-december-2 https://github.com/actions/checkout https://docs.zizmor.sh/audits/ https://github.com/rhysd/actionlint"
---
# GitHub Actions

GitHub has no official agent skill for Actions; `github/awesome-copilot` has a community `github-actions-hardening` skill (MIT). This skill follows GitHub's "Security hardening" reference and the tools that check it.

1. Read what the repository already has: the workflows, `.github/workflows/actions.lock` if present, `dependabot.yml`, and whether actions are pinned by SHA, by tag or through the lockfile. Done when you can say which pinning scheme the repository uses.
2. Look up the current major of every action you add or bump on its releases page (`gh release list -R actions/checkout`), because remembered versions are stale: `actions/checkout` is at v7. Done when each `uses:` you write names a version you saw today.
3. Write the change with these defaults, each for the reason given:
   - `permissions: {}` or `contents: read` at the top, widened per job only where a step needs it, because the token otherwise gets whatever the repository default is.
   - Untrusted values (`github.event.*` titles, bodies, branch names, `github.head_ref`, inputs) reach `run:` only through `env:` and a quoted shell variable, because `${{ }}` is pasted into the script before the shell runs.
   - `actions/checkout` with `persist-credentials: false` unless a later step pushes, so the token does not stay in the workspace for artifacts or other steps.
   - Cloud access through OIDC: `id-token: write` on the deploy job only, and a cloud role whose trust policy checks the repository and the branch or environment.
   - `concurrency:` with `cancel-in-progress: true` for pull request workflows; never cancel a deploy halfway.
   - Release and deploy jobs build from scratch instead of restoring caches written by pull request jobs, which anyone can fill.
   Done when `actionlint` reports nothing on the files you touched.
4. Pin what you added the way the repository does. With a lockfile: run `gh actions-lock` and commit `actions.lock` with the workflow. Without one: a full 40-character commit SHA taken from the action's own repository, followed by `# vX.Y.Z`. Done when `zizmor` reports no `unpinned-uses` on your lines.
5. Run the commands of `## Verify` and put each one with its result in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- Workflow lockfiles are generally available since 2026-10-28 on Free, Team and Enterprise Cloud: `.github/workflows/actions.lock`, written by the `gh actions-lock` extension (`gh extension install github/gh-actions-lock`), records the commit of every action including transitive ones, and the runner executes those commits even if a tag moves. Never edit it by hand: rerun `gh actions-lock` after any `uses:` change, `gh actions-lock --relock` to advance moved refs. It replaces SHA pinning in workflow files, except for reusable workflows (`jobs.<id>.uses`), which still need a full SHA.
- `actions/checkout` v7 refuses to check out fork pull request code in `pull_request_target` and `workflow_run` workflows unless `allow-unsafe-pr-checkout: true`. Do not set that input to make a workflow pass: split the work into an unprivileged `pull_request` workflow and a privileged `workflow_run` one that only reads its artifacts.
- From 2026-12-02, `pull_request_target` is blocked by default in public repositories unless an organization or repository Actions event policy allows it (brownouts ran on November 9 and 16). Private and internal repositories are not affected.
- Since checkout v6, `persist-credentials` stores the token in a file under `$RUNNER_TEMP` instead of `.git/config`; the token is still on disk for later steps, so `false` remains the safe default.
- A SHA pin only helps if the commit belongs to the named repository: a SHA from a fork network passes the syntax check (zizmor `impostor-commit`), and a stale `# v…` comment misleads reviewers (`ref-version-mismatch`).
- Dependabot alerts do not cover SHA-pinned actions; Dependabot version updates do, and they regenerate `actions.lock` entries in the same pull request.
- `secrets: inherit` hands every secret to a reusable workflow; pass the named secrets it needs.

## Verify

- The project's own lint target if it has one (`make lint`, a pre-commit hook).
- `actionlint` on the workflow files.
- `zizmor .` (add `--offline` without network or token; audits such as `impostor-commit` and `known-vulnerable-actions` need the network). `zizmor --fix=safe .` applies safe fixes only.
- `gh actions-lock --verify` when the repository has `actions.lock` (`--verify-local` checks coverage without the network).

## Security

- A workflow triggered by `pull_request_target`, `workflow_run` or `issue_comment` runs with the base repository's token and secrets: never build or run code from the pull request in it.
- Self-hosted runners on a public repository run code from anyone who opens a pull request; keep them to private repositories, or ephemeral just-in-time runners.
- A secret printed to a log must be rotated, not just deleted from the log; do not wrap secrets in JSON or YAML, which defeats masking.
- Writing untrusted text to `$GITHUB_ENV` or `$GITHUB_PATH` lets it change later steps (zizmor `github-env`).
- Protect `.github/workflows/` with `CODEOWNERS` when the repository has one, and say in your report which workflows gained permissions or secrets.

## References

- Security hardening reference: https://docs.github.com/en/actions/reference/security/secure-use
- Workflow lockfiles: https://docs.github.com/en/actions/concepts/security/workflow-dependency-lockfiles and https://github.com/github/gh-actions-lock
- zizmor audits (what each finding means and how to fix it): https://docs.zizmor.sh/audits/
