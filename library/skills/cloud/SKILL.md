---
name: cloud
description: >-
  Changes code and infrastructure that run on AWS, Google Cloud or Azure without touching live accounts:
  infrastructure as code (CDK, CloudFormation, SAM, Bicep, Cloud Build and App Engine configs), IAM roles and
  policies at least privilege, credentials without keys (OIDC, workload identity, managed identity), SDK
  clients, storage exposure, and reading the cloud through the vendor's own agent tools. Use when a task
  touches cdk.json, template.yaml, samconfig.toml, *.bicep, azure.yaml, app.yaml, cloudbuild.yaml, an AWS,
  Google Cloud or Azure SDK, or aws, gcloud, az or azd commands. Not for Terraform or OpenTofu code
  (terraform) nor Kubernetes manifests (kubernetes).
paths: "**/cdk.json, **/template.yaml, **/template.yml, **/samconfig.toml, **/*.bicep, **/*.bicepparam, **/azure.yaml, **/app.yaml, **/cloudbuild.yaml, **/serverless.yml"
license: MIT
metadata:
  termf-stacks: "aws gcp azure"
  termf-roles: "devops back"
  termf-versions: "aws-cli>=2 azure-cli>=2"
  termf-verified: "2026-11-22 aws-cli@2.42.1 azure-cli@2.93.0 aws/agent-toolkit-for-aws@main google/skills@main microsoft/azure-skills@main"
  termf-sources: "https://github.com/aws/agent-toolkit-for-aws (official, Apache-2.0) https://docs.aws.amazon.com/llms.txt https://github.com/google/skills (official, Apache-2.0) https://github.com/microsoft/azure-skills (official, MIT) https://github.com/microsoft/mcp (official, MIT)"
---
# Cloud: AWS, Google Cloud, Azure

Each cloud publishes official skills and an MCP server for agents, maintained with its services: AWS `aws/agent-toolkit-for-aws` (Apache-2.0, plugin `aws-core`), Google `google/skills` (Apache-2.0), Microsoft `microsoft/azure-skills` (MIT, plugin `azure`). This skill is a router: it keeps the boundary, the traps that cut across clouds, and the checks, and sends you to one reference per cloud.

1. Find which cloud and which tool describes the infrastructure: CDK (`cdk.json`), CloudFormation or SAM (`template.yaml`, `samconfig.toml`), Bicep (`*.bicep`, `azure.yaml` for azd), Terraform (use the `terraform` skill), or SDK code only. Done when you can name the cloud, the tool and its version from the lockfile or CLI.
2. Read the reference of that cloud: `references/aws.md`, `references/gcp.md` or `references/azure.md`. Done when it is open.
3. If the cloud's official skill for the task is loaded in this session, follow it for service guidance and keep this skill for the boundary and checks. If not, carry on and name the official plugin in your report so the user can install it. Done when you know which guidance you follow.
4. Change the infrastructure as code, never by CLI or console, because a manual change drifts from the code and is lost at the next deploy. Done when every resource you touch is in a template, stack or module.
5. Write IAM at least privilege: named actions on named resources, conditions where the service supports them, one role per workload. Done when no policy you wrote has a `*` action on a `*` resource, or the report explains why one must.
6. Run the commands of `## Verify` (synthesise, lint, diff or what-if) and put each one with its result in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- Deploying is the user's call: `cdk deploy`, `sam deploy`, `az deployment … create`, `azd up`, `gcloud … deploy|create|delete` and any CLI call that creates, changes or deletes a resource act on a live account. Stop at synth, lint, diff and what-if, and write the deploy command in your report.
- Read-only exploration of the account goes through the vendor's MCP server only when the user configured it for this session, and stays read-only; never configure credentials yourself.
- Credentials come from the environment or the CLI's own chain (SSO, workload identity, managed identity). Long-lived access keys, service account key files and client secrets in code, `.env` files committed to git or CI variables are the leak to avoid; CI uses OIDC federation.
- Check which account or subscription a command would hit before running it (`aws sts get-caller-identity`, `gcloud config list`, `az account show`); a default profile often points at production.
- `https://cloud.google.com/llms.txt` answers with a "not found" page; Google's agent docs come through `google/skills` and its developer-knowledge skill instead.

## Verify

- The project's own targets first (a `Makefile`, `package.json` scripts around CDK or SAM, CI jobs).
- AWS: `npx cdk synth` and `npx cdk diff` (diff needs credentials), `cfn-lint template.yaml`, `sam validate --lint`.
- Azure: `az bicep build --file main.bicep`, then `az deployment group what-if …` only when the user has signed in to a non-production subscription.
- Google Cloud: infrastructure is usually Terraform (run the `terraform` skill's checks); `cloudbuild.yaml` and `app.yaml` have no offline validator, so say in your report that they were checked by reading.
- IAM: `aws accessanalyzer validate-policy --policy-type IDENTITY_POLICY --policy-document file://policy.json` for AWS policies you wrote.

## Security

- Storage stays private: no public bucket, blob container or object ACL unless the task is a public website, and then through the CDN's origin access rather than public reads.
- No `*:*` policy, no `Owner`/`Editor`/`AdministratorAccess` for a workload; scope roles to the resources the code uses.
- Secrets live in the cloud's secret manager and are read at run time; never in templates, parameters files committed to git, or environment blocks of a template.
- Trust policies for CI federation check the repository and branch or environment in the token's subject, not only the issuer.

## References

- Read `references/aws.md` when the project runs on AWS (CDK, CloudFormation, SAM, Lambda, the AWS SDKs).
- Read `references/gcp.md` when it runs on Google Cloud (Cloud Run, GKE, Cloud SQL, the Google Cloud SDKs).
- Read `references/azure.md` when it runs on Azure (Bicep, azd, App Service, Functions, the Azure SDKs).
