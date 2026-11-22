# PostgreSQL: locks and safe forms

Read when the project's database is PostgreSQL. Checked on 2026-11-22 against the PostgreSQL 19 manual (current; 18, 17, 16 and 15 still supported), restated in our own words: https://www.postgresql.org/docs/current/sql-altertable.html, https://www.postgresql.org/docs/current/sql-createindex.html, https://www.postgresql.org/docs/current/explicit-locking.html.

Contents: read the schema · session settings · lock modes · statement by statement · indexes · constraints · backfill · plans · row level security · tool options.

## Read the schema

```sql
\d+ orders                                                    -- psql: columns, indexes, constraints, triggers
SELECT reltuples::bigint AS approx_rows,
       pg_size_pretty(pg_total_relation_size(oid)) AS size
FROM pg_class WHERE oid = 'orders'::regclass;
SELECT conname, contype, convalidated, pg_get_constraintdef(oid)
FROM pg_constraint WHERE conrelid = 'orders'::regclass;
SELECT indexrelid::regclass, indisvalid, pg_get_indexdef(indexrelid)
FROM pg_index WHERE indrelid = 'orders'::regclass;
```

## Session settings for a DDL migration

- `SET lock_timeout = '5s';` first. It bounds each wait for a lock separately (a statement that needs several locks can wait several times). While an `ALTER TABLE` waits for its lock, the queries that arrive after it wait behind it, so a short timeout and a retry hurt less than a long wait.
- `statement_timeout` would also stop a long `CREATE INDEX CONCURRENTLY` or a validation scan: leave it off for those, or set it per migration above the expected duration.
- A timeout aborts the migration; the tool records nothing as applied. Run it again later; when it keeps failing, find the transaction holding the table (`pg_stat_activity`, `pg_locks`) and report it instead of killing it.

## Lock modes, from the weakest that matters here

| Lock | Taken by | Blocks |
|---|---|---|
| `SHARE UPDATE EXCLUSIVE` | `CREATE INDEX CONCURRENTLY`, `REINDEX CONCURRENTLY`, `VALIDATE CONSTRAINT`, `ANALYZE`, `VACUUM` | other schema changes and `VACUUM`; reads and writes go on |
| `SHARE` | `CREATE INDEX` | writes, for the whole build |
| `SHARE ROW EXCLUSIVE` | `ADD FOREIGN KEY` (on both tables), `ENABLE`/`DISABLE TRIGGER` | writes |
| `ACCESS EXCLUSIVE` | every `ALTER TABLE` form not listed above, `DROP`, `RENAME` | everything, reads included |

An `ALTER TABLE` with several subcommands takes the strongest lock any of them needs.

## Statement by statement

| Statement | Lock | Cost | Safe form |
|---|---|---|---|
| `ADD COLUMN` without default, or with a non-volatile default (a constant, `now()`) | `ACCESS EXCLUSIVE` | catalog only (since 11) | as is, after `lock_timeout` |
| `ADD COLUMN` with a volatile default (`clock_timestamp()`, `random()`, `gen_random_uuid()`), an identity, a stored generated column, a domain with constraints | `ACCESS EXCLUSIVE` | rewrites the table and its indexes | add it nullable without default, `ALTER COLUMN … SET DEFAULT …` for new rows (it does not touch existing rows), backfill in batches |
| `ADD COLUMN` virtual generated (18+) | `ACCESS EXCLUSIVE` | catalog only | as is |
| `DROP COLUMN` | `ACCESS EXCLUSIVE` | catalog only; the space comes back with a later rewrite | as is, once no code reads the column |
| `ALTER COLUMN TYPE` | `ACCESS EXCLUSIVE` | rewrite, unless the old type is binary coercible to the new one and `USING` keeps the values (`varchar` to `text`) | new column, dual write, backfill, switch, drop |
| `SET NOT NULL` | `ACCESS EXCLUSIVE` | full scan, skipped when a valid `CHECK` proves no nulls | see Constraints |
| `ADD FOREIGN KEY` | `SHARE ROW EXCLUSIVE` on both tables | full scan to validate | `NOT VALID`, then `VALIDATE CONSTRAINT` |
| `ADD CHECK` | `ACCESS EXCLUSIVE` | full scan to validate | `NOT VALID`, then `VALIDATE CONSTRAINT` |
| `ADD UNIQUE` / `PRIMARY KEY` | `ACCESS EXCLUSIVE` | builds the index under the lock | `CREATE UNIQUE INDEX CONCURRENTLY`, then `ADD CONSTRAINT … USING INDEX` |
| `CREATE INDEX` | `SHARE` | blocks writes for the build | `CREATE INDEX CONCURRENTLY` |
| `DROP INDEX` | `ACCESS EXCLUSIVE` | quick, once it has the lock | `DROP INDEX CONCURRENTLY` |
| `RENAME COLUMN` / `RENAME TO` | `ACCESS EXCLUSIVE` | catalog only | breaks the previous code: expand and contract instead |

## Indexes

- `CREATE INDEX CONCURRENTLY` cannot run inside a transaction block. It scans the table twice and waits, before each scan and at the end, for the transactions that could still see the old state: a long transaction elsewhere makes it wait just as long.
- When it fails (a deadlock, a duplicate for a unique index, a timeout), it leaves an `INVALID` index that queries ignore but every write still maintains; an invalid unique index even keeps rejecting duplicates. Find it with `SELECT indexrelid::regclass FROM pg_index WHERE NOT indisvalid;`, then `DROP INDEX CONCURRENTLY` it and build again, or `REINDEX INDEX CONCURRENTLY` it.
- A unique index built concurrently already rejects duplicates from its second scan, before it is usable: a write can fail even if the build then fails.
- Partitioned tables refuse `CONCURRENTLY`: build the index concurrently on each partition, then create the index on the parent, which is then a catalog-only step.
- Put the `CREATE INDEX CONCURRENTLY` alone in its migration, with the tool's wrapping off (see Tool options).

## Constraints

Foreign key or check on a table in use, in two migrations:

```sql
-- migration 1 (brief lock, no scan)
SET lock_timeout = '5s';
ALTER TABLE orders ADD CONSTRAINT orders_customer_fk
  FOREIGN KEY (customer_id) REFERENCES customers (id) NOT VALID;
-- migration 2 (SHARE UPDATE EXCLUSIVE; ROW SHARE on customers): reads and writes go on during the scan
ALTER TABLE orders VALIDATE CONSTRAINT orders_customer_fk;
```

New rows are checked from migration 1; only the old rows wait for migration 2. Fix the rows that would fail before validating.

`NOT NULL` on a big table:

- 18 and later: `ALTER TABLE orders ADD CONSTRAINT orders_customer_nn NOT NULL customer_id NOT VALID;`, then in a later migration `ALTER TABLE orders VALIDATE CONSTRAINT orders_customer_nn;`.
- 17 and earlier: `ADD CONSTRAINT orders_customer_nn CHECK (customer_id IS NOT NULL) NOT VALID`, then `VALIDATE CONSTRAINT`, then `ALTER COLUMN customer_id SET NOT NULL` (the valid check spares it the scan), then drop the check.

Unique or primary key on a table in use:

```sql
CREATE UNIQUE INDEX CONCURRENTLY orders_ref_key ON orders (ref);           -- alone, outside a transaction
ALTER TABLE orders ADD CONSTRAINT orders_ref_key UNIQUE USING INDEX orders_ref_key;
```

The index must be a plain b-tree (no expression, no `WHERE`). For a primary key, the columns must already be `NOT NULL`, or the statement scans the table.

## Backfill

```sql
-- one batch; the job loops over id ranges until the whole range is done
UPDATE orders SET customer_id = legacy.customer_id
FROM legacy_orders AS legacy
WHERE legacy.order_id = orders.id
  AND orders.id >= :lo AND orders.id < :hi
  AND orders.customer_id IS NULL;
```

Each batch commits on its own. After a large backfill, `ANALYZE orders;` so the planner sees the new values.

## Plans

- `EXPLAIN (ANALYZE, BUFFERS) <query>` executes the query; buffer counts come with `ANALYZE` by default on recent versions. Wrap `INSERT`, `UPDATE`, `DELETE` and `MERGE` in `BEGIN; … ROLLBACK;`.
- `EXPLAIN (GENERIC_PLAN) <query with $1>` plans a parameterised query without values (not together with `ANALYZE`).
- Compare the plan before and after the migration on the run's database; an index the planner does not use is a cost on every write.

## Row level security for a tenant column

- `ALTER TABLE invoices ENABLE ROW LEVEL SECURITY;` with no policy denies every row; add the policy the sibling tables use, in the same migration.
- The table owner skips the policies unless `ALTER TABLE invoices FORCE ROW LEVEL SECURITY;`; superusers and `BYPASSRLS` roles always skip them.
- Unique and foreign key checks ignore policies: a unique constraint without the tenant column tells one tenant that another's value exists. Lead unique constraints with the tenant column.
- Test the policies connected as the application's role, on the run's database.

## Tool options for PostgreSQL

- Rails: `disable_ddl_transaction!` with `add_index :orders, :ref, algorithm: :concurrently`; `add_foreign_key …, validate: false` then `validate_foreign_key`; `add_check_constraint …, validate: false` then `validate_check_constraint`.
- Django: `django.contrib.postgres.operations.AddIndexConcurrently` in a migration with `atomic = False`; `AddConstraintNotValid` and `ValidateConstraint` in two separate migrations (together in one atomic migration they behave like a plain `AddConstraint`).
- Alembic: `op.create_index(…, postgresql_concurrently=True)` inside `with op.get_context().autocommit_block():`; with autocommit blocks, set `transaction_per_migration=True` in `env.py`.
- Laravel: `public $withinTransaction = false;` and the statement through `DB::statement(…)`; the `->online()` index modifier (which adds `CONCURRENTLY`) appears only in the docs of the next release: check https://laravel.com/docs/master/migrations before using it.
- Prisma Migrate: `prisma migrate dev --create-only`, then write the statements above in `migration.sql`; Prisma does not generate `CONCURRENTLY`, and migration scripts accept it from 7.4.0.
- Flyway: a `V<n>__<name>.sql.conf` beside the script with `executeInTransaction=false`, and Flyway's PostgreSQL setting `postgresql.transactional.lock` set to false for `CREATE INDEX CONCURRENTLY` (https://documentation.red-gate.com/fd/flyway-postgresql-transactional-lock-setting-277579114.html).
