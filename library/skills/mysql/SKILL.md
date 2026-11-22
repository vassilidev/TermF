---
name: mysql
description: >-
  Writes MySQL schema changes and queries that suit the installed series: online ALTER TABLE without copying
  the table, utf8mb4, strict mode, foreign keys, EXPLAIN ANALYZE, upgrades from 8.0 or 8.4 to 9.7 LTS and the
  new calendar-versioned releases. Use when a task touches SQL or migrations for MySQL, mysql:// or
  DB_CONNECTION=mysql settings, my.cnf, or the mysql and mysqldump clients. Not for PostgreSQL (sql-postgres),
  nor for MariaDB-specific features, which diverged from MySQL.
paths: "**/*.sql, **/migrations/**, **/database/migrations/**, **/db/migrate/**, **/my.cnf, **/*.cnf"
license: MIT
metadata:
  termf-stacks: "mysql"
  termf-roles: "back devops"
  termf-versions: "*"
  termf-verified: "2026-11-22 mysql@9.7.4-lts mysql@8.4-lts mysql@26.10-innovation"
  termf-sources: "https://dev.mysql.com/doc/refman/9.7/en/mysql-releases.html https://dev.mysql.com/doc/relnotes/mysql/9.7/en/ https://dev.mysql.com/doc/relnotes/mysql/9.0/en/news-9-0-0.html https://dev.mysql.com/doc/refman/9.7/en/innodb-online-ddl-operations.html https://dev.mysql.com/doc/refman/9.7/en/mysql-nutshell.html"
---
# MySQL

No official agent skill exists for MySQL (Oracle's `oracle/skills` covers Oracle Database, APEX and OCI, not MySQL). The MySQL manual is proprietary: this skill restates facts in its own words and links the versioned pages.

1. Read the server series: the image tag in Compose or CI (`mysql:8.4`), the managed service config, or `SELECT VERSION();`. Check it is MySQL and not MariaDB (`SELECT @@version_comment;`). Done when you can name the series.
2. Open the manual of that series (`https://dev.mysql.com/doc/refman/<series>/en/`) for any statement whose behaviour you are not sure of. Done when the page for the task is open.
3. Write migrations with the project's migration tool and its naming, because the tool records what ran. Done when the new migration appears in the tool's status command.
4. Write every `ALTER TABLE` on a table that takes writes with an explicit `ALGORITHM` and `LOCK` (see `## Gotchas`), so MySQL refuses it instead of silently copying the table. Done when the statement runs on the local database with the clause you chose.
5. Run the migration up, then down if the tool supports it, then up again, on the local or worktree database only. Done when all three succeed.
6. Run the commands of `## Verify` and put each one with its result in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- Release model: 8.4 and 9.7 are the LTS series (9.7.0 on 2026-04-21, 9.7.4 on 2026-10-20). After 9.7, versions are calendar based (`26.7`, `26.10` Innovation releases). LTS upgrades go one series at a time: 8.4 to 9.7, never skipping an LTS; an Innovation downgrade needs a dump and reload.
- 9.0 removed the `mysql_native_password` server plugin (8.4 had it off by default): clients and drivers must support `caching_sha2_password`, which needs TLS or RSA key exchange on the first connection.
- 9.0 enforces inline `REFERENCES` in a column definition, which earlier versions parsed and ignored: an old schema that relied on that silently gains foreign keys on upgrade.
- 9.0 turns a scalar subquery that returns more than one row into an error in `UPDATE`, `DELETE` and `INSERT IGNORE`, where it used to be skipped.
- Online DDL: adding, dropping or renaming a column and changing a default are `ALGORITHM=INSTANT`; each instant add or drop counts toward 255 row versions (then error 4092 until the table is rebuilt). A secondary index is `ALGORITHM=INPLACE, LOCK=NONE`. Changing a column type is `ALGORITHM=COPY` and blocks writes. Growing a `VARCHAR` is in place unless its length prefix grows from 1 to 2 bytes (crossing 255 bytes), which copies.
- Adding a foreign key copies the table unless `foreign_key_checks` is off for the session; prefer adding it before the table fills, or plan the copy.
- Write `utf8mb4` and an explicit collation; `utf8` is the deprecated 3-byte `utf8mb3` and cannot store emoji.
- `EXPLAIN ANALYZE` executes the query; read plans with `EXPLAIN FORMAT=TREE` first and run `ANALYZE` only on the local database.
- `VECTOR` (9.0+) cannot be part of any index and only compares for equality.
- In a TermF worktree the database is a dump-and-restore copy for this worktree, and the worktree's env files point to it. Run migrations there; a remote or production host is never yours to touch.

## Verify

- The project's own commands first: the migration tool's status, up and down, and the tests that use the database.
- `mysql --version` and `mysql -e "SELECT VERSION(), @@version_comment, @@sql_mode;"`
- `mysql -e "EXPLAIN FORMAT=TREE <query>"` for a query you changed, and check it uses the index you added.

## Security

- Pass values as bind parameters; never build SQL from request data, including `ORDER BY` columns (map them through an allowlist).
- Keep `STRICT_TRANS_TABLES` in `sql_mode`; without it MySQL truncates or zeroes bad values instead of failing.
- `LOAD DATA LOCAL INFILE` lets the server read client files: keep `local_infile` off unless the task needs it.
- The application user gets only the privileges it uses on its own schema: no `SUPER`, `FILE` or `GRANT OPTION`, and no `root`.

## References

- Release model and upgrade paths: https://dev.mysql.com/doc/refman/9.7/en/mysql-releases.html
- Online DDL per operation: https://dev.mysql.com/doc/refman/9.7/en/innodb-online-ddl-operations.html
- Release notes per series: https://dev.mysql.com/doc/relnotes/mysql/9.7/en/
