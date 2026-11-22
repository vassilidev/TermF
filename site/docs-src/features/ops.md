---
title: Servers, databases and logs
order: 7
---

The Ops view gives you, and one approval at a time the agents, access to the project's servers, databases and logs, with secrets masked in everything that comes back.

## Connections and recipes {#ops}

You declare connections (SSH, Postgres, MySQL, Kubernetes, CloudWatch, Google Cloud Logging, Loki, Datadog) and run recipes on them: tail the logs, list the pods, run a read-only command. An agent can ask to run a recipe too, and every request shows you the exact command, the target and the environment before it runs.

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
- The project's own recipes sit next to the shipped ones: logs, Kubernetes pods and logs, a read-only SSH command, a service restart.

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

## The read-only SQL console {#sql-console}

The SQL console runs one query at a time on a Postgres or MySQL connection of the project, and refuses anything that is not a read before it is sent: TermF parses the query in the database's own dialect, then runs it in a read-only transaction with a time limit.

![The SQL console of Ops: a read-only query on a production Postgres, checked before it runs, its result sorted with emails masked, and the recent queries](shot:sql "A read-only query, checked as you type, emails masked. Sample data.")

### Run a query

1. Open **Ops**, then **SQL**, and pick a Postgres or MySQL connection.
2. Write the query. As you type, TermF shows the exact query that will run, or why it is refused, in plain words.
3. Run it with <kbd>⌘↵</kbd>. The table sorts by any column; copy it, or export it to CSV.
4. An agent can ask for the same through the `sql-readonly` recipe: the card shows you the query before it runs, and personal data comes back masked.

### Rules

- One statement: `SELECT`, `WITH`, `EXPLAIN` of a query, `SHOW` and `DESCRIBE` only. A write, `SELECT … INTO`, a lock or a function that acts beyond reading is refused, even inside a subquery.
- A read-only transaction, 15 seconds and 500 rows at most.
- A warning when the connection's user could write: give the console a read-only user.
- Every query is logged in the project's Ops history.
- The project's own recipes can take a `sql` parameter, for a query you run often.

```sh
# the same query from a terminal
termf ops run sql-readonly --connection <name> --query <sql>
```

> [!NOTE]
> Never run against a real database: the tests use fake `psql` and `mysql`. The remaining risk is a query the parser reads differently from the server; the read-only transaction is the next line of defense.
>
> **Tried so far.** Covered by automated tests, including the known tricks to slip a write past a parser; the console was checked with a simulated backend.

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
