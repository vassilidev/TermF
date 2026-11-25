---
title: Database studio
order: 8
---

The database studio reads your Postgres, MySQL, MariaDB, SQLite and ClickHouse databases, read-only, with the schema at hand, charts of a result, and a DBA to write queries.

![The database studio: the tables of a production Postgres on the left, and on the right the DBA's answer, a query checked as a read, with buttons to put it in the editor, run it or save it](shot:studio "The DBA writes a query, checked before anything runs. Sample data.")

## Add a database {#connect}

1. In **Ops**, **Add a connection**, paste the database's URL (`postgres://`, `mysql://`, `mariadb://`, `sqlite:`, `clickhouse://`) and **Fill in**. Or **Use** a database declared in the project's repositories: `.env` files, `config/database.php`, `prisma/schema.prisma`, `config/database.yml` or a Compose file.
2. The password goes to the [vault](vault.md), never in the project's file. A database on your machine is `dev`, any other `prod`.
3. A database behind a server is reached through an SSH connection of the project, with several hops if needed. `sslmode` sets the encryption.
4. **Test** it. A failed check says what to do in plain words: a closed port, a refused password, an unknown host.

## Query it {#query}

1. Open **Ops**, then **SQL**, and pick a database.
2. **Schema**, on the left, lists the tables with their columns, types and keys, and the indexes. A click puts a name in the query.
3. The editor completes table and column names. As you type, TermF shows the exact query that will run, or why it is refused, in plain words.
4. **Run** it with <kbd>⌘↵</kbd>. Sort the result by any column, **Copy** it (TSV, CSV, JSON, Markdown) or **Export** it (CSV, TSV, JSON, NDJSON, Markdown, SQL, Excel), up to 100,000 rows.
5. **Explain** asks the DBA what the query does; **Optimize** sends it the query with its plan.

## Chart a result {#chart}

**Table** and **Chart**, above a result, switch between the rows and a chart.

![A result shown as a bar chart: the total spent by the ten best customers this month, with a tooltip on the first bar](shot:chart "The ten best customers of the month, as bars. Sample data.")

- TermF picks the chart for you: a line along a date or a time, bars of numbers by a text, a line along a number. Id columns are left out of the series.
- Change the type (**Bars**, **Line**, **Area**, or **Pie** when it reads well: one series, a few slices, no negative value), the **X axis** and the **Series**.
- Hover a point to read its value. **Export** saves the chart as PNG or SVG.

## The whole schema {#schema}

**Query** and **Schema**, in the database's bar, switch the studio to a diagram of every table, with its columns, types and keys, and a line for each foreign key.

![The schema of a database as a diagram: tables as cards linked by their foreign keys, and the orders table open with its columns, indexes and relations](shot:schema "The whole schema, the orders table open. Sample data.")

- Search a table or a column: the others fade. Zoom with the buttons, a pinch or <kbd>⌘</kbd> and the wheel, drag to move, **Fit** to see everything.
- Click a table: its columns, indexes, **Points to** and **Pointed to by** (a click goes to the other table), **Query this table** and **Ask the DBA**.

## Ask the DBA {#dba}

The **DBA** tab, on the right, takes a request in your own words and answers with one query and a few lines. It is the same conversation as everywhere else in TermF: streamed answers, cards, images and **Topic done** work the same, see [Ask from any screen](ask-panel.md#conversation).

- It sees the schema. It sees rows only if the connection lets agents in: each of its reads is a query you approve in TermF, with personal data masked.
- Each query it writes is checked like yours: **Checked: a read**, or why it is refused. **Put in the editor**, **Run** or **Save** it.
- One conversation per database, kept for the session. Open it in **Conversations** to go on.

## Saved queries {#saved}

**Save** keeps a query with its name, description, folder, tags and database, in the project's file, so it syncs with the project. **Saved** searches them by any of these or by their text, grouped by folder. A saved query can become a recipe: an agent then runs that query and nothing else, after your approval. **Recent** lists the last queries run.

## Rules {#rules}

- One statement: `SELECT`, `WITH`, `EXPLAIN` of a query, `SHOW` and `DESCRIBE` only. A write, `SELECT … INTO`, a lock or a function that acts beyond reading is refused, even inside a subquery.
- A read-only transaction, 15 seconds and 500 rows at most. SQLite opens the file read-only, and ClickHouse runs in its read-only mode.
- A warning when the connection's user could write: give the studio a read-only user.
- Every query, schema read and export is logged in the project's Ops history.

```sh
# the same query from a terminal
termf ops run sql-readonly --connection <name> --query <sql>
```

> [!NOTE]
> Only SQLite has run against a real database. Postgres, MySQL, MariaDB and ClickHouse run through their own clients, checked against fake clients and their documentation. The DBA has only run with a fake Claude Code. The remaining risk is a query the parser reads differently from the server; the read-only transaction is the next line of defense.
>
> **Tried so far.** Covered by automated tests, including the known tricks to slip a write past a parser; SQLite tried for real; the studio checked with a simulated backend.
