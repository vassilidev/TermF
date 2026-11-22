# AWS

Read with the `cloud` skill when the project runs on AWS. AWS documentation is CC BY-SA 4.0: facts here are restated and linked, not copied.

## Official agent tools

- Agent Toolkit for AWS (`aws/agent-toolkit-for-aws`, Apache-2.0): `/plugin install aws-core@claude-plugins-official` brings skills (`aws-iam`, `aws-cdk`, `aws-cloudformation`, `aws-serverless`, `aws-security`, `aws-secrets-manager`, `aws-storage`…) and the AWS MCP Server configuration. More plugins: `aws-agents`, `aws-data-analytics`, and `awslabs/agent-plugins` (serverless, deploy-on-aws).
- Docs for agents: https://docs.aws.amazon.com/llms.txt links each service's own `llms.txt` and Markdown pages.

## Versions

- AWS CLI v2 (2.42.1 on 2026-11-22). CDK: the library `aws-cdk-lib` (2.278.0) and the CLI `aws-cdk` (2.1149.0) have separate version numbers since the CLI moved to 2.1000+; read both from the lockfile and do not "align" them.
- JavaScript SDK v3 is modular (`@aws-sdk/client-s3`); the monolithic `aws-sdk` v2 package is out of support. Python: `boto3`.

## Credentials and accounts

- Local: IAM Identity Center profiles (`aws configure sso`, then `aws sso login --profile <p>`), selected with `AWS_PROFILE`. Check the target with `aws sts get-caller-identity` before any call.
- CI: GitHub OIDC with `aws-actions/configure-aws-credentials` and `role-to-assume`; the role's trust policy checks `token.actions.githubusercontent.com:aud` = `sts.amazonaws.com` and `:sub` = `repo:<owner>/<repo>:environment:<env>` (or a branch ref), never a wildcard on the repository.
- Code: let the SDK's default credential chain find credentials; never pass keys to a client constructor.

## IAM

- Name actions and resource ARNs; add conditions (`aws:SourceArn`, `aws:PrincipalOrgID`, tags) where the service supports them.
- `iam:PassRole` on `*` lets a principal hand any role to a service it controls: scope it to the role ARNs the workload passes, with `iam:PassedToService`.
- Validate before proposing: `aws accessanalyzer validate-policy --policy-type IDENTITY_POLICY --policy-document file://policy.json`.

## Infrastructure as code

- CDK: `npx cdk synth` works offline unless the app uses context lookups; commit `cdk.context.json` so synth stays deterministic. `cdk diff` needs credentials. Keep `RemovalPolicy.RETAIN` on databases and buckets that hold data.
- CloudFormation and SAM: `cfn-lint template.yaml`, `sam validate --lint`. Changes to a resource's replacement properties recreate it: read the "Update requires" line of the property in the resource reference before changing it.

## Storage and data

- New S3 buckets block public access and disable ACLs by default: keep it. Serve public files through CloudFront with origin access control instead of public reads.
- Encrypt with KMS keys the project already defines; give the workload `kms:Decrypt` on that key only.
- Secrets in Secrets Manager or SSM Parameter Store (SecureString), read at run time; `NoEcho` only masks a CloudFormation parameter in describe calls, and the value still reaches outputs, metadata or logs that reference it.
