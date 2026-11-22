# Azure

Read with the `cloud` skill when the project runs on Azure. Microsoft Learn documentation is CC BY 4.0 (code MIT); facts here are restated and linked.

## Official agent tools

- `microsoft/azure-skills` (MIT): `/plugin marketplace add microsoft/azure-skills` then `/plugin install azure@azure-skills`. It brings skills (`azure-prepare`, `azure-deploy`, `azure-compute`, `azure-kubernetes`, `azure-compliance`, cost and observability plugins…) and wires the Azure MCP Server. It needs Node.js 18+, `az login`, and `azd auth login` for deployment workflows.
- The Azure MCP Server now lives in `microsoft/mcp`; the older `Azure/azure-mcp` repository is archived.
- Microsoft Learn MCP (`https://learn.microsoft.com/api/mcp`) answers documentation questions from current Learn pages.

## Credentials and subscriptions

- Code uses `DefaultAzureCredential` (Azure Identity), which picks a managed identity in Azure and the developer's `az login` locally; no client secret in configuration.
- Check the target with `az account show` before any command; scripts pass `--subscription` explicitly.
- CI: `azure/login` with OpenID Connect (federated credential on an app registration or user-assigned identity, `client-id`, `tenant-id`, `subscription-id`, no secret); the federated credential's subject names the repository and the environment or branch.

## Bicep and azd

- `az bicep build --file main.bicep` compiles and runs the linter configured in `bicepconfig.json`.
- Preview before anyone deploys: `az deployment group what-if --resource-group <rg> --template-file main.bicep`, or `azd provision --preview` in an azd project. Both need a signed-in, non-production subscription; otherwise stop at the build.
- Mark secret parameters `@secure()` and never output them: deployment outputs are readable by anyone with read access to the deployment.

## RBAC and data

- Assign built-in data-plane roles scoped to the resource (`Storage Blob Data Reader` on one account, `Key Vault Secrets User` on one vault), not `Contributor` or `Owner` on the resource group.
- Key Vault with RBAC authorization; read secrets at run time or through App Service and Functions Key Vault references.
- Storage accounts with `allowBlobPublicAccess: false` and shared key access disabled where the code uses Entra ID.
