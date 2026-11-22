---
name: sql-postgres
description: >-
  Writes PostgreSQL schema migrations that are safe on a live database, and queries and indexes that hold up:
  adding columns, constraints and indexes without long locks, backfills, EXPLAIN plans, slow queries,
  transactions, row level security, upgrades to PostgreSQL 18 and 19. Use when a task touches SQL for
  Postgres, migration files of an ORM on Postgres, psql, DATABASE_URL with postgres://, or EXPLAIN output.
  Not for MySQL or MariaDB (mysql), MongoDB (mongodb) or Redis (redis).
paths: "**/*.sql, **/migrations/**, **/migrate/**, **/db/**, **/prisma/schema.prisma, **/alembic/**"
license: MIT
metadata:
  termf-stacks: "postgres"
  termf-roles: "back devops"
  termf-versions: ">=14"
  termf-verified: "2026-11-22 postgresql@19.0 postgresql@18.7 supabase/agent-skills@main timescale/pg-aiguide@main"
  termf-sources: "https://www.postgresql.org/docs/19/release-19.html https://www.postgresql.org/docs/18/release-18.html https://www.postgresql.org/support/versioning/ https://www.postgresql.org/docs/current/sql-createindex.html https://www.postgresql.org/docs/current/sql-altertable.html https://github.com/supabase/agent-skills (official Supabase, MIT) https://github.com/timescale/pg-aiguide (official Timescale, Apache-2.0)"
---
# PostgreSQL

Two database vendors publish Postgres skills: Supabase's `supabase-postgres-best-practices` (`supabase/agent-skills`, MIT) and Timescale's `postgres`, `design-postgres-tables` and `postgres-database-migration` (`timescale/pg-aiguide`, Apache-2.0). This skill stays thin: lock safety on a live database, the traps of the current versions, and the checks.

1. Read the server version the project targets: the image tag in Compose or CI (`postgres:18`), the managed service config, or `SELECT version();` on the local database. Done when you can name the major version.
2. If one of the skills above is loaded in this session, follow it for schema design, indexing and query style, and keep this skill for migrations and the traps below. If none is, carry on here and name them in your report so the user can import one. Done when you know which guidance you follow.
3. Write migrations with the project's migration tool and its naming, never raw files beside it, because the tool records what ran. Done when the new migration appears in the tool's status or list command.
4. Make each step of a migration safe on a table that is being written to (see `## Gotchas`): a `lock_timeout` first, no table rewrite under an exclusive lock, constraints in two steps, indexes built concurrently, backfills in batches outside the schema change. Done when no statement takes an `ACCESS EXCLUSIVE` lock for longer than a catalog update.
5. Run the migration up, then down if the tool supports it, then up again, on the local or worktree database only. Done when all three succeed.
6. Run the commands of `## Verify` and put each one with its result in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- Start each DDL migration with `SET lock_timeout = '5s'`: an `ALTER TABLE` waiting behind a long transaction blocks every later query on the table while it waits.
- `CREATE INDEX CONCURRENTLY` cannot run inside a transaction block, and most migration tools wrap each migration in one: use the tool's per-migration switch to turn that off. A failed concurrent build leaves an `INVALID` index: drop it and build again.
- Add foreign keys and checks as `NOT VALID`, then `VALIDATE CONSTRAINT` in a later migration; validation only takes a lock that lets writes continue. For `NOT NULL` on a big table: add `CHECK (col IS NOT NULL) NOT VALID`, validate it, then `SET NOT NULL` (which then skips the full scan) and drop the check; on 18+ `ALTER TABLE` can also mark a `NOT NULL` constraint `NOT VALID`.
- Adding a column with a constant default is a catalog change; a volatile default such as `clock_timestamp()` or `gen_random_uuid()`, or most column type changes, rewrite the whole table under an exclusive lock.
- PostgreSQL 19 (2026-11-19): `standard_conforming_strings` is forced on (backslashes in `'…'` are literal; dumps taken by an older `pg_dump` with it off no longer load), `json_array()` over no rows returns `[]` instead of `NULL`, JIT is off by default, `REPACK (CONCURRENTLY)` replaces `VACUUM FULL` and `CLUSTER` without blocking writes, `max_locks_per_transaction` defaults to 128, and MD5 password logins log a warning.
- PostgreSQL 18: generated columns are virtual unless you write `STORED`, `uuidv7()` exists, `RETURNING` can read `OLD` and `NEW`, `VACUUM` and `ANALYZE` include inheritance children unless you write `ONLY`, and new clusters have data checksums on.
- `EXPLAIN ANALYZE` runs the statement: wrap an `INSERT`, `UPDATE` or `DELETE` in `BEGIN; … ROLLBACK;`.
- In a TermF worktree the database is a copy made for this worktree (`CREATE DATABASE … TEMPLATE …`), and the worktree's `.env` / `DATABASE_URL` points to it. Run migrations there; a remote or production URL is never yours to touch.

## Verify

- The project's own commands first: the migration tool's status, up and down, and the test suite that uses the database.
- `psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f <file.sql>` for plain SQL.
- `EXPLAIN (ANALYZE, BUFFERS) <query>` on realistic data for a query you changed, and check that the plan uses the index you added.
- After a concurrent index build: `SELECT indexrelid::regclass FROM pg_index WHERE NOT indisvalid;` returns nothing.

## Security

- Pass values as bind parameters; in PL/pgSQL build dynamic SQL with `format('%I', …)` for identifiers and `%L` or `USING` for values.
- Row level security does not apply to the table owner or superusers unless `ALTER TABLE … FORCE ROW LEVEL SECURITY`; test policies as the application role.
- A `SECURITY DEFINER` function needs `SET search_path = pg_catalog, pg_temp` (plus the schemas it uses), otherwise a caller can shadow the objects it reads.
- The application role owns nothing it does not need: no superuser, no `CREATEROLE`, migrations under a separate role when the project has one.
- MD5 password authentication is deprecated (18) and warned about at each login (19): use SCRAM for any role you create.

## References

- Release notes: https://www.postgresql.org/docs/19/release-19.html and https://www.postgresql.org/docs/18/release-18.html ; lock levels of each `ALTER TABLE` form: https://www.postgresql.org/docs/current/sql-altertable.html
- Vendor skills: `npx skills add supabase/agent-skills --skill supabase-postgres-best-practices` ; https://github.com/timescale/pg-aiguide
