---
title: Stack
order: 6
---

The **Stack** view maps what a project is made of: its repositories and the links between them, each dependency with its latest version, its support and its known vulnerabilities. A safe update runs from the map, after your OK.

## The map {#map}

1. Open the project's **Stack**, between CI/CD and Design system, or from the palette (<kbd>⌘K</kbd>).
2. **Project map** shows each repository with its languages, frameworks and versions, and a line where one **calls** another (a URL of an env file or of Compose names it), **depends on the package** another publishes, or **uses the database** they share.
3. A click on a repository shows it alone; a second click shows them all again. Each one lists its dependencies, its Compose services and ports, its databases, the variables its `.env.example` expects and its CI workflows. **Read the schema** reads a database through its Ops connection, on your click only.
4. **Around the repositories**: the CI workflows, the deployments, the Ops connections by environment, and the agents' roles and tools ([MCP and connectors](mcp-connectors.md)).

Each item leads to its view: a repository to Git, workflows and deployments to CI/CD, a connection to Ops, a database to the SQL studio or Redis, a manifest to Files.

Nothing in the repositories runs: TermF reads the manifests and lockfiles of npm, pnpm, yarn, bun, Composer, Cargo, uv, poetry, `requirements.txt`, Go modules and Bundler.

## Dependencies {#deps}

For each direct dependency: the version used, the **Latest**, the kind of update (**Patch**, **Minor** or **Major**), and its vulnerabilities with their severity and the version that fixes them. Runtimes and frameworks also show their LTS and when their support ends.

- Filter by **All**, **To update**, **Vulnerable** or **End of support**, or **Search a package**.
- The versions come from each ecosystem's registry, the vulnerabilities from OSV.dev, the support from endoflife.date. Only package names and versions leave your machine.
- Answers are kept 24 hours; **Check again** reads them anew. Offline, the map keeps what it read last, and says so.

## Update from the map {#update}

- **A safe update**, within the same major version, opens a card with the command and the vulnerabilities it fixes. **Run in a terminal** types it in a TermF terminal of the repository, such as `pnpm add vite@5.4.11`, `composer require` or `cargo update --precise`. Run the repository's tests before you commit.
- **A major update**, a runtime at the end of its support, or a `requirements.txt` goes to devops: **Hand to devops** opens its conversation with the line attached.

> [!NOTE]
> Not yet tried in the real app or against the real registries: the shape of the answers of the registries, OSV.dev and endoflife.date was checked by hand. Maven, Gradle, .NET, Swift, pub and a binary `bun.lockb` are not read yet. The severity is GitHub's level, with no CVSS score, and a major version of a Go module, which is another module, is never proposed.
>
> **Tried so far.** Covered by automated tests with no network call, and checked with a simulated backend.
