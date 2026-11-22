# MySQL (InnoDB): online DDL

Read when the project's database is MySQL. Checked on 2026-11-22 against the MySQL 9.7 LTS manual (8.4 for its own limits); the manual is proprietary, so this page restates facts in its own words. Per operation: https://dev.mysql.com/doc/refman/9.7/en/innodb-online-ddl-operations.html ; clauses: https://dev.mysql.com/doc/refman/9.7/en/alter-table.html. MariaDB is a different product whose online DDL differs: read its own manual.

Contents: read the schema · DDL is not transactional · ALGORITHM and LOCK · metadata locks · operation by operation · limits of INSTANT · foreign keys · plans · tenants · tool options.

## Read the schema

```sql
SELECT VERSION(), @@version_comment;                 -- MySQL or MariaDB, and the series
SHOW CREATE TABLE events;
SELECT TABLE_ROWS, DATA_LENGTH, INDEX_LENGTH          -- TABLE_ROWS is an estimate
FROM information_schema.TABLES
WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = 'events';
SELECT NAME, TOTAL_ROW_VERSIONS FROM information_schema.INNODB_TABLES
WHERE NAME = CONCAT(DATABASE(), '/events');
```

## DDL is not transactional

- Each DDL statement commits the open transaction before it runs; it cannot sit inside `START TRANSACTION … COMMIT` and is never rolled back with other statements. An InnoDB DDL statement is atomic on its own: it completes or leaves nothing.
- So a tool's transaction does nothing for DDL on MySQL (Django's `atomic` has no effect there; Laravel opens none), and a migration that fails halfway keeps what ran. Put one DDL statement in each migration, and a data change never in the same migration as a DDL statement.

## ALGORITHM and LOCK: always write them

- Without `ALGORITHM`, MySQL takes `INSTANT` when the operation allows it, else `INPLACE`, else `COPY`, silently. With `ALGORITHM` written, an operation that cannot use it fails with an error instead: that error is what you want on a busy table.
- `LOCK=NONE` keeps reads and writes going and fails when the operation cannot allow writes; `LOCK=SHARED` keeps reads only; `COPY` always blocks writes at least.
- `ALGORITHM=INSTANT` accepts only the default lock: write `ALGORITHM=INSTANT` alone. For the rest, write `ALGORITHM=INPLACE, LOCK=NONE`. When neither is accepted, the operation copies the table: report `blocked` with the estimated time.
- An instant change cannot share a statement with a change that is not instant: one `ALTER TABLE` per kind.

```sql
SET SESSION lock_wait_timeout = 5;
ALTER TABLE events ADD COLUMN source VARCHAR(32) NULL, ALGORITHM=INSTANT;
-- next migration
SET SESSION lock_wait_timeout = 5;
ALTER TABLE events ADD INDEX events_account_created (account_id, created_at), ALGORITHM=INPLACE, LOCK=NONE;
```

## Metadata locks

- Even `INSTANT` and `INPLACE` take an exclusive metadata lock for a moment, at the start and at the end. To get it they wait for every open transaction that has touched the table, and while they wait, each new query on the table waits behind them (`SHOW PROCESSLIST` shows "Waiting for table metadata lock").
- `lock_wait_timeout` bounds each of those waits; its default is 31,536,000 seconds (a year). Set it low in the migration's session and run the migration again later when it times out.
- An idle transaction left open by a worker or a console blocks the DDL just as much as a long query: find it before retrying.

## Operation by operation (9.7)

| Operation | Algorithm | Writes during it | Notes |
|---|---|---|---|
| Add a column, any position | `INSTANT` | yes | the limits below; an `AUTO_INCREMENT` column needs `INPLACE, LOCK=SHARED` |
| Drop a column | `INSTANT` | yes | not a column a functional index uses |
| Rename a column | `INSTANT` | yes, if type and nullability stay | a column another table's foreign key references: `INPLACE` only |
| Set or drop a default | `INSTANT` | yes | |
| Add `ENUM` / `SET` members at the end | `INSTANT` | yes | only when the storage size stays (a `SET` going from 8 to 9 members does not); members inserted in the middle copy |
| Lengthen a `VARCHAR` | `INPLACE` | yes | only while its length prefix keeps its size: crossing 255 bytes, or shortening, copies |
| Make a column `NULL` / `NOT NULL` | `INPLACE`, rebuilds | yes | `NOT NULL` needs a strict `sql_mode` and fails on existing nulls |
| Reorder columns | `INPLACE`, rebuilds | yes | |
| Change a column's type | `COPY` only | no | expand and contract with a new column |
| Add a secondary index | `INPLACE` | yes | finishes after the transactions using the table end |
| Drop or rename an index | `INPLACE` | yes | catalog only |
| Add a `FULLTEXT` index | `INPLACE` | no | the first one rebuilds the table unless it has an `FTS_DOC_ID` column |
| Add a `SPATIAL` index | `INPLACE`, `LOCK=SHARED` | no | |
| Add a primary key | `INPLACE`, rebuilds | yes | copies the data; define it at creation when you can |
| Drop the primary key without adding one | `COPY` | no | drop and add in one statement stays `INPLACE` |
| Convert the table's character set | `INPLACE`, rebuilds when the encoding changes | no | |
| Rebuild (`ALTER TABLE … FORCE`) | `INPLACE`, `LOCK=NONE` | yes | not on a table with a `FULLTEXT` index |

`LOCK=NONE` is refused on a table that has `ON … CASCADE` or `ON … SET NULL` foreign keys.

## Limits of INSTANT

- Each instant add or drop counts as a row version: 255 at most on 9.1 and later, 64 on 8.4. At the limit, instant adds and drops fail (error 4092) until a rebuild (`ALTER TABLE … FORCE` or `OPTIMIZE TABLE`) resets the count. Read `TOTAL_ROW_VERSIONS` before a series of instant changes.
- No instant add or drop on a table with a `FULLTEXT` index, with `ROW_FORMAT=COMPRESSED`, on a temporary table, in the data dictionary tablespace, or past the maximum row size or 1,022 internal columns: those fall back to `INPLACE` (with `ALGORITHM` written, they fail and tell you).

## Foreign keys

- Adding one is `INPLACE` only with `foreign_key_checks` off for the session; with checks on, it copies the table. With checks off, MySQL does not check the existing rows: run the query that finds orphans (a `LEFT JOIN … WHERE parent.id IS NULL`) on the run's database first, and fix them in a data migration.
- Dropping one is `INPLACE` and catalog only.

## Other costs of a long online change

- The writes made during an `INPLACE` build are kept in a log bounded by `innodb_online_alter_log_max_size`; past it the DDL fails. The log is applied at the end under a brief lock: the busier the table, the longer that end.
- A replica runs the DDL only after the source finishes it, and applies the writes made meanwhile only after that: replicas lag by the DDL's duration.
- A large rebuild cannot be paused or throttled, and rolling it back after a failure is expensive.

## Plans

- `EXPLAIN FORMAT=TREE <query>` reads the plan without running it.
- `EXPLAIN ANALYZE <query>` runs it and reports, per step, the estimated and actual rows, the time to the first row and in total, and the loops; it covers `SELECT`, `TABLE` and multi-table `UPDATE` and `DELETE`. Its JSON form needs `explain_json_format_version = 2`.
- `EXPLAIN FOR CONNECTION <id>` shows the plan of a statement running in another connection (not with `ANALYZE`).

## Tenants

MySQL has no row level security: a tenant column is enforced by the application's queries (every new query filters on it, every unique key starts with it), or the project uses a database per tenant and runs each migration once per tenant.

## Tool options for MySQL

- Write explicit `ALGORITHM` and `LOCK` through the tool's raw SQL: Rails `execute`, Django `migrations.RunSQL` with its `reverse_sql`, Alembic `op.execute`, Laravel `DB::statement`, and the `.sql` file itself for Prisma and Flyway.
- Laravel 13 has `->instant()` (adds or modifies the column with the instant algorithm; it appends the column at the end, so not with `after` or `first`) and `->lock($mode)` on columns, indexes and foreign keys. Use `->instant()` alone for an instant change, since MySQL accepts no other lock than the default with `INSTANT`.
