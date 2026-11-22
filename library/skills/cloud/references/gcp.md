# Google Cloud

Read with the `cloud` skill when the project runs on Google Cloud. Google Cloud documentation is CC BY 4.0 (code samples Apache-2.0); facts here are restated and linked.

## Official agent tools

- `google/skills` (Apache-2.0): `npx skills add google/skills`, or in Claude Code `claude plugin marketplace add google/skills` then `claude plugin install <plugin>@google-plugins`. It holds skills per product (`cloud-run-basics`, `cloud-sql-basics`, `gke-*`, `iam-helper-*`, `google-cloud-waf-security`, `gcloud`…) and `retrieving-developer-knowledge`, the way to read current Google docs.
- `https://cloud.google.com/llms.txt` answers with a "not found" page: do not rely on it.

## Credentials and projects

- Local: Application Default Credentials from `gcloud auth application-default login`; code uses the client library's default credentials and never loads a key file path from the repository.
- Check the active project and account with `gcloud config list` before any command; pass `--project` explicitly in scripts.
- CI: Workload Identity Federation with `google-github-actions/auth` (`workload_identity_provider` and `service_account`); the provider's attribute condition checks the repository, not only the GitHub issuer.
- Service account key files are the leak to avoid; many organisations block their creation by policy, which is a reason to propose federation, not a key.

## IAM

- One service account per workload with the narrowest predefined role on the resource itself (a bucket, a secret, a topic), not on the project.
- Do not run workloads as the default compute service account, which often carries broad project roles.
- Basic roles (`roles/owner`, `roles/editor`, `roles/viewer`) are too broad for any workload.

## Services

- Cloud Run: `--allow-unauthenticated` makes the service public on the internet; keep it off for internal services and set ingress to internal where callers are inside the project. Mount secrets with `--set-secrets` from Secret Manager.
- Cloud SQL: connect through the Cloud SQL connectors or Auth Proxy with IAM database authentication; never an authorised network of `0.0.0.0/0` on a public IP.
- Cloud Storage: uniform bucket-level access and public access prevention on; signed URLs for temporary access.
- Infrastructure is usually Terraform with the `google` provider: use the `terraform` skill for it, and `google/skills`' `google-cicd-terraform` when loaded.
