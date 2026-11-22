---
name: schema-migrations
description: >-
  Plans, writes and reviews schema changes that ship without downtime, whatever the ORM: the expand, backfill,
  switch and contract sequence across releases, the lock each DDL statement takes and the form that avoids a
  long one, migration tools that wrap DDL in a transaction (Rails, Django, Alembic, Prisma, Laravel, Flyway),
  the down step or the written way back, batched backfills, plans measured on the worktree's own database copy,
  tenant isolation, and the checklist for reviewing a run's migrations. Use when a migration adds, renames,
  drops or retypes a column, table, index or constraint, or when asked to review one. Not for everyday
  application queries, nor database servers, backups or hosting (devops); engine specifics stay with
  sql-postgres, mysql, mongodb and redis.
paths: "**/migrations/**, **/Migrations/**, **/db/migrate/**, **/alembic/**, **/db/migration/**, **/*.prisma, **/db/schema.rb, **/db/structure.sql"
license: MIT
metadata:
  termf-stacks: "sql postgres mysql prisma sqlalchemy django rails laravel"
  termf-roles: "dba back"
  termf-versions: "*"
  termf-verified: "2026-11-22 postgresql@19.0 docs (18 for NOT NULL NOT VALID), mysql@9.7 and 8.4 refman, rails@8.1.4.1 API, django@6.1, alembic@1.20.1, prisma@7.10.1 (7.4.0 notes), laravel@13.x docs and framework master, Flyway docs, TermF worktree database copy"
  termf-sources: "https://www.postgresql.org/docs/current/sql-altertable.html https://www.postgresql.org/docs/current/sql-createindex.html https://www.postgresql.org/docs/current/explicit-locking.html https://www.postgresql.org/docs/current/runtime-config-client.html https://www.postgresql.org/docs/current/sql-explain.html https://www.postgresql.org/docs/current/ddl-rowsecurity.html https://www.postgresql.org/docs/release/11.0/ https://wiki.postgresql.org/wiki/Operations_cheat_sheet https://dev.mysql.com/doc/refman/9.7/en/innodb-online-ddl-operations.html https://dev.mysql.com/doc/refman/9.7/en/alter-table.html https://dev.mysql.com/doc/refman/9.7/en/implicit-commit.html https://dev.mysql.com/doc/refman/9.7/en/explain.html https://api.rubyonrails.org/classes/ActiveRecord/Migration.html https://docs.djangoproject.com/en/stable/howto/writing-migrations/ https://docs.djangoproject.com/en/stable/ref/contrib/postgres/operations/ https://alembic.sqlalchemy.org/en/latest/api/runtime.html https://www.prisma.io/docs/orm/prisma-migrate/workflows/customizing-migrations https://github.com/prisma/prisma/releases/tag/7.4.0 https://laravel.com/docs/13.x/migrations https://documentation.red-gate.com/fd/reference/script-configuration"
---
# Schema migrations

The procedure for changing a schema that running code depends on. The lock of each statement, engine by engine, is in `references/`; queries, indexes and engine upgrades stay with the skills `sql-postgres`, `mysql`, `mongodb` and `redis`.

## Gotchas

- During a deploy, the code of the previous release and of the new one run against the same schema for a while (rolling restarts, workers, a rollback). A migration that renames, drops, retypes or tightens something the previous code uses breaks it: split it as in step 4.
- Most tools wrap each migration in a transaction where the engine allows DDL inside one (PostgreSQL, SQLite). A statement that refuses a transaction (`CREATE INDEX CONCURRENTLY`) then fails, and a backfill inside it keeps the DDL's lock until the end. Turn the wrapping off for that one migration (step 6).
- MySQL commits before each DDL statement and cannot roll DDL back: a migration of three `ALTER TABLE` that fails on the second keeps the first. Put one DDL statement in each MySQL migration.
- A DDL statement waiting for its lock makes every later query on the table wait behind it, so the wait hurts as much as the hold. Bound it (`SET lock_timeout = '5s'` on PostgreSQL, `SET SESSION lock_wait_timeout = 5` on MySQL, whose default is one year) and run the migration again later instead of waiting.
- A down step that adds a dropped column back restores the column, not its data. A contract migration loses data for good: say so in the report and in the migration.
- `EXPLAIN ANALYZE` executes the statement, writes included, on both engines.
- In a TermF worktree the database is a copy made for this worktree (step 2). A URL from a ticket, a log or a message is never yours to migrate.

## Procedure

1. **Engine, version, tool.** Read the engine and its version (the image tag in Compose or CI, the managed service config, `SELECT version();`) and the migration tool with its version from the lockfile (Rails, Django, Alembic, Prisma, Laravel, Flyway…). Done when you can name all three and the file each came from.
2. **The run's database.** TermF gives each worktree its own copy of the project's local database before the session starts: PostgreSQL through `CREATE DATABASE … TEMPLATE` (a dump and restore when the original has open connections), MySQL and MariaDB through a dump, SQLite as a copy of its file; the copy is named after the original with `_termf_<id>` appended. The worktree's git-ignored env files that named the original (`.env.development.local`, `.env.local`, `.env.development`, `.env`; keys `DATABASE_URL`, `DB_URL`, Laravel's `DB_DATABASE`) now name the copy; with Rails' `config/database.yml`, which TermF leaves as it is, the session's `DATABASE_URL` points to the copy. Only a server on this machine (localhost, a loopback address, a local socket) or a service of the repository's Compose file is copied: a remote database is refused, and the run then has no database of its own. Go through the project's commands, which read those files; the session is not allowed to read `.env` files or print its environment, and does not need to. Done when `SELECT current_database();` (PostgreSQL) or `SELECT DATABASE();` (MySQL), run through the project's console, names the copy, or when you report `blocked` because the only database at hand is shared or remote.
3. **The schema as it is.** Read the tool's status, the schema file it keeps (`db/schema.rb` or `db/structure.sql`, `schema.prisma`, the models), then the database itself: each touched table's definition, indexes, constraints, triggers and approximate row count (queries in `references/postgres.md` and `references/mysql.md`). Find who writes to the table: the code, jobs, other services. Done when each touched table has its size, its indexes and constraints, and its writers in your notes.
4. **Split the change into releases** (expand, migrate, contract), so each migration works with the code before it and after it:

   | Change | Expand (this release) | Migrate | Contract (a later release) |
   |---|---|---|---|
   | New required column | add it nullable, or with a constant default | code writes it; backfill old rows | `NOT NULL` in its validated form |
   | Rename a column or table | add the new one; code writes both | backfill; code reads the new one | drop the old one |
   | Change a column's type | add a column of the new type; code writes both | backfill; code reads the new one | drop the old one |
   | Drop a column or table | code stops reading and writing it, the ORM's model included | — | drop it |
   | New foreign key, check or unique | add it without checking existing rows, or build its unique index online | fix the rows that break it | validate it, or attach the index as the constraint |

   Done when each migration is listed with the release it belongs to, and the contract steps are left for a later run unless the task says the previous code is gone.
5. **The lock of each statement.** Read `references/postgres.md` or `references/mysql.md` and pick, for each statement, the form that touches only the catalog or builds online. Write down its lock and how long it holds it on the table's size. When only a blocking form exists and step 4 cannot route around it (dropping a MySQL primary key, converting a MySQL table's character set), stop and report `blocked` with the estimated time: downtime is the user's call. Done when every statement has its lock and its duration in your summary.
6. **The tool's transaction.** Turn it off only for the migration that needs it, and keep that migration to the statement that needs it:

   | Tool | Wraps each migration | For one migration |
   |---|---|---|
   | Rails | yes when the adapter supports DDL transactions | `disable_ddl_transaction!` in the class |
   | Django | yes on PostgreSQL and SQLite | `atomic = False` on the `Migration` class |
   | Alembic | as `env.py` configures it (`transaction_per_migration`) | `with op.get_context().autocommit_block():` around the statement; it commits what ran before it |
   | Laravel | yes when the schema grammar supports it (PostgreSQL, not MySQL) | `public $withinTransaction = false;` on the migration |
   | Prisma Migrate | runs `migration.sql` as written | create it with `prisma migrate dev --create-only`, edit the SQL, then apply; `CREATE INDEX CONCURRENTLY` works in migrations from 7.4.0 |
   | Flyway | `executeInTransaction` is true by default | `V<n>__<name>.sql.conf` beside the script with `executeInTransaction=false` |

   Done when the migration runs on the run's database without "cannot run inside a transaction block".
7. **Backfill in batches.** Change data apart from the schema change (a data migration or a job the project runs), by primary key ranges, each batch in its own short transaction: `UPDATE … SET new_col = … WHERE id >= :lo AND id < :hi AND new_col IS NULL`, sized so one batch takes well under a second (often 1,000 to 10,000 rows), with a pause between batches when replicas lag. The `IS NULL` filter makes it idempotent and resumable. Ship the code that writes new rows before backfilling the old ones, so no row is missed. Done when the backfill runs twice on the run's database with the same result and `SELECT count(*) … WHERE new_col IS NULL` returns 0.
8. **The way back.** Give each migration the tool's down form (`down`, `downgrade`, a reversible `change`, `down()`). When the tool keeps no down step, write the reverse SQL in your summary. Done when up, down and up again succeed on the run's database, and each step that loses data on the way down is named.
9. **Measure.** For each query the change serves or touches, read its plan before and after on the run's database: PostgreSQL `EXPLAIN (ANALYZE, BUFFERS)`, MySQL `EXPLAIN FORMAT=TREE` then `EXPLAIN ANALYZE`; writes go inside `BEGIN; … ROLLBACK;`. The copy holds the original's data; when it is much smaller than production, say so beside the plan. Done when each new index comes with the query it serves and its plan before and after.
10. **Tenants.** Name the isolation the project uses: a tenant column (with row level security on PostgreSQL), a schema per tenant, or a database per tenant. With a tenant column, each new table holding tenant data gets the column `NOT NULL` with its foreign key, leads its indexes and unique constraints with it, and gets the same policy as its sibling tables. With a schema or a database per tenant, the migration runs once per tenant through the project's command: plan its total time and what happens when one tenant fails halfway. Done when every new table, index, unique constraint and query keeps the isolation.
11. Run the commands of `## Verify` and put each one with its result in your summary, with each migration's locks, duration and way back. Done when they pass, or when the failure is reported with its output.

## Reviewing a run's migrations

As the review step, change nothing. Read each migration the run changed and the code that calls the tables it touches, and report each finding with the scenario that shows it and the task that must fix it:

- A lock held longer than a catalog change on a table that takes writes: an index built without `CONCURRENTLY` or `ALGORITHM=INPLACE, LOCK=NONE`, a foreign key or check validated in the statement that adds it, a type change, a volatile default, a MySQL operation that copies the table, or no lock timeout.
- A statement the tool's transaction will refuse, or a backfill inside the schema change's transaction.
- A rename, drop, type change or new `NOT NULL` that the previous release's code still needs.
- A migration without a down step or a written way back, or a down step that loses data without saying so.
- A backfill in a single statement over a large table, or one that is not idempotent.
- An index without the query it serves, one that duplicates an existing index, or a plan not measured.
- A new tenant table without the tenant column, its index or its policy.
- Several DDL statements in one MySQL migration, or a MySQL `ALTER TABLE` without explicit `ALGORITHM`.

## Verify

- The project's own commands first: the migration tool's status, then up, down and up again, and the tests that use the database.
- The SQL the tool will run, to check each lock: Django `python manage.py sqlmigrate <app> <migration>` (and `--backwards`), Alembic `alembic upgrade <rev> --sql`, Laravel `php artisan migrate --pretend`, Prisma and Flyway the `.sql` file itself.
- PostgreSQL, after a concurrent build: `SELECT indexrelid::regclass FROM pg_index WHERE NOT indisvalid;` returns nothing.
- MySQL, after instant column changes: `SELECT NAME, TOTAL_ROW_VERSIONS FROM INFORMATION_SCHEMA.INNODB_TABLES WHERE NAME = '<schema>/<table>';` stays below the series' limit.

## Security

- A table owner, a superuser and a `BYPASSRLS` role skip row level security, and foreign key and unique checks always do: give each new tenant table `ENABLE` and `FORCE ROW LEVEL SECURITY` like its siblings, and test it as the application's role.
- A data migration never carries values copied from production (emails, tokens, keys): it selects the rows it changes by a query.
- A column that copies personal or secret data keeps the source's protections (encryption, masking, retention, grants).
- Identifiers built from data (a schema or a table per tenant) are quoted as identifiers (`format('%I', …)` on PostgreSQL, the driver's identifier quoting on MySQL), never concatenated.
- Migrations run with the role the project gives them, never a superuser; when the project manages grants in migrations, the application's role gets exactly what the new table needs.

## References

- Read `references/postgres.md` when the project's database is PostgreSQL: the lock of each statement, the safe forms, the schema queries, row level security, and each tool's PostgreSQL options.
- Read `references/mysql.md` when it is MySQL (or MariaDB, checking its own manual): online DDL per operation, the limits of `INSTANT`, metadata locks, and each tool's MySQL options.
- The engine skills `sql-postgres`, `mysql`, `mongodb` and `redis` for everything that is not a schema change.
