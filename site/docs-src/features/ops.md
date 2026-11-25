---
title: Servers, databases and logs
order: 7
---

The Ops view gives you, and one approval at a time the agents, access to the project's servers, databases and logs, with secrets masked in everything that comes back.

## Connections and recipes {#ops}

You declare connections (SSH, Postgres, MySQL, Redis, Kubernetes, CloudWatch, Google Cloud Logging, Loki, Datadog) and run recipes on them: tail the logs, list the pods, run a read-only command. An agent can ask to run a recipe too, and every request shows you the exact command, the target and the environment before it runs.

![Ops: the project's connections, a production SSH server with its checks, and the recipes that run on it](shot:ops "A connection, tested, with its recipes. Sample data.")

### Add a connection

1. In **Ops**, add a connection: its kind, its environment (dev, staging, prod) and its secrets, by name, from the [vault](vault.md).
2. **Test** it: the tool, the secrets, the tunnel. A database warns you when its user can write.
3. Run a recipe with its form. The exact command shows as you type, and the result comes back masked.
4. When an agent asks, an approval card shows the command, the target and the environment, with a countdown. A prod target asks your master password.

### Defaults

- A connection is invisible to agents until you allow it, masked or in full.
- Your system's own clients run it (`ssh`, `psql`, `mysql`, `kubectl`…). Keys are never read and secrets never go on the command line.
- Databases open read-only when the recipe reads. The output is masked (vault values, tokens, emails, phone numbers) and each run is logged.
- After a prod recipe, the session needs your OK to reach the web or other tools.

### Settings

- Connections live in the project's file, as `[[ops.connections]]`.
- The project's own recipes sit next to the shipped ones: logs, Kubernetes pods and logs, a read-only SSH command, a service restart, a server's health, Nginx, Laravel and Redis.

```sh
# connections
termf ops connections | test

# recipes, from a terminal
termf ops recipes | run <recipe>
```

> [!NOTE]
> Never run against a real server, database, cluster or log service: the tests use fake `ssh`, `psql` and the like, on macOS and Linux. `ssh` on Windows has not been tried.
>
> **Tried so far.** Covered by automated tests, including command injection attempts against a real shell; adding an SSH connection is driven in the real app on macOS by an end-to-end test.

## Servers, Redis and Forge {#servers}

An SSH connection also shows panels that read the server for you, and a few changes that wait for your confirmation:

- [Server health](server-health.md): CPU, memory, disks, services, ports and SSL certificates;
- [Nginx](nginx.md): whether it runs, `nginx -t`, its sites, its logs live, a safe reload;
- [Laravel](laravel.md): `php artisan about`, the scheduler, Horizon, failed jobs, `laravel.log` live.

A [Redis](redis.md) connection lists its keys and reads their values. The **Forge** tab imports a [Laravel Forge](forge.md) account's servers and sites, and redeploys them.

From <kbd>⌘I</kbd>, the DevOps engineer can add, change, test or delete a connection and do these changes too, each through its card. See [Agents that act for you](agent-actions.md).

## The database studio {#sql-console}

The **SQL** tab of Ops is a database studio for the project's Postgres, MySQL, MariaDB, SQLite and ClickHouse databases, always read-only: the schema and its diagram, an editor that completes names, charts of a result, the DBA, saved queries and exports. See [Database studio](database-studio.md).

## Live logs and the production .env {#live-logs}

The **Live** tab follows a log as it is written, masked line by line. The `env-fetch` recipe reads a server's `.env` and shows its keys, values hidden, so you can find the variable that differs between production and your machine.

### Follow a log

1. Open **Ops**, then **Live**. Pick the recipe, the connection and its settings, then **Follow**.
2. Filter by text or by level, pause, clear, stop.

Logs come from `journalctl` or a file over SSH, Kubernetes, CloudWatch and Loki. The last 5,000 lines stay on screen, and the stream keeps going from one tab to another.

### Compare a server's .env

1. In **Ops › Recipes**, run `env-fetch` on an SSH connection: you get the keys of the server's `.env`, values masked.
2. Compare it with a repository of the project: which keys differ, which are only local.
3. Reveal a value for 30 seconds, copy it, or write it into the `.env` of a run's worktree.

A value written into a worktree goes to a file git ignores; it is refused if git tracks it. Every reveal, copy and write is logged. Agents never follow a log, and never get a value of the `.env`: only its keys.

Your own recipes can do the same with `follow = true` or `env = true`.

```sh
# the keys of a server's .env, from a terminal
termf ops run env-fetch --connection <name> --path <file>
```

> [!NOTE]
> Never run against a real server or log service: the tests use fake `ssh` and `journalctl`, on macOS and Linux.
>
> **Tried so far.** Covered by automated tests; the Live tab and the comparison were checked with a simulated backend.
