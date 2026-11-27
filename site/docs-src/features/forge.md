---
title: Laravel Forge
order: 8
---

Give a project its Laravel Forge account, and TermF imports its servers and sites as Ops connections, databases and deploy targets. Then you follow and redeploy each site from Ops.

![The Forge tab of Ops: the project's Forge account, then its servers and sites, two of them checked to import](shot:forge "Import a Forge account's sites. Sample data.")

## The project's account {#account}

1. In **Ops**, open **Forge**.
2. Fill **Account label** and **API token**: create the token on forge.laravel.com, under API Tokens. When the Forge CLI on this machine has a token, **Reuse this token** takes it instead, after your click only.
3. **Test** checks the token and names its organization and its number of servers; a token that sees several organizations lets you pick yours.
4. **Save** tests the token first and writes nothing if it fails. The token goes to the [vault](vault.md), as a global production secret named after the label, such as `FORGE_TOKEN_ACME_AGENCY`. The project's file keeps only the label, the organization and the secret's name, in `[ops.forge]`.

Each project has its own account. Agents never see the token: an agent that sets up the account asks you to type it on its card.

## Import servers and sites {#import}

1. **Import from Forge**, then **List the servers**: each server with its sites, and each site's repository and branch.
2. Check the sites you want, then **Import**.

For each one, TermF creates:

- the SSH connection `forge@ip` of its server, with the [site folder](laravel.md) when it is the only site imported on that server;
- the site's database, read from its environment through Forge's API (MySQL, MariaDB or Postgres), reached through the server's SSH tunnel. Its password goes to the vault, never to the project's file;
- a Forge deploy target in [CI/CD](ci.md), linked to the project's repository of the same name.

A connection that already has the same name is kept, and TermF says so.

## Deployments {#deployments}

![The deployments of a Forge site: each with its state, commit, author and date, and the log of the last one](shot:deploys "A site's deployments and the log of one. Sample data.")

- **Deployments** of a site: each one's state, commit, author and date.
- A click shows its log, masked by the vault, the token removed.
- **Redeploy** runs the site's deployment script on its server, after a confirmation.

The Forge deploy targets of CI/CD use the project's account when they have no token of their own.

## With an agent {#agents}

From <kbd>⌘I</kbd> on Ops, an agent lists the servers and sites without a card, and sets up the account, imports sites and redeploys one, each through its card. See [Agents that act for you](agent-actions.md).

> [!NOTE]
> Not yet: the deployment script and the site's environment, daemons, Forge's scheduler, SSL, service restarts, and Forge's own databases and backups.
>
> **Tried so far.** Never against the real Forge API: built from its documentation and checked with recorded answers, automated tests and a simulated backend.
