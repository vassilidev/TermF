# Migrations on a live database

A migration runs once against a database that already holds data and serves requests. Write it so that the old code and the new code both work while it runs, and so that it can be reversed.

## Before writing

- Read the generated migration file, every time: Django guesses renames as a remove plus an add, which drops the column's data. Answer the rename prompt of `makemigrations`, or write `RenameField` yourself.
- `python manage.py sqlmigrate <app> <number>` shows the SQL that will run: check it for table rewrites and long locks.
- Keep one concern per migration: schema in one, data in another, so a failure leaves a known state.

## Adding a column

1. Add it nullable, or with a `db_default`, so existing rows and the old code keep working.
2. Backfill in a separate data migration (below), in batches for large tables.
3. Tighten it (`null=False`, a constraint) in a later migration, once the backfill is done everywhere.

## Indexes and constraints on large PostgreSQL tables

- `AddIndexConcurrently` and `RemoveIndexConcurrently` (`django.contrib.postgres.operations`) build the index without blocking writes; the migration needs `atomic = False`.
- A new `CheckConstraint` or foreign key validates every row while holding a lock. On a large table, add it in a form the database checks later (on PostgreSQL, `NOT VALID` then `VALIDATE CONSTRAINT` through `RunSQL` with its reverse), and say so in the report.

## Data migrations

- `migrations.RunPython(forward, reverse)`: always give the reverse (`migrations.RunPython.noop` when undoing is a no-op).
- Inside, get models with `apps.get_model("app", "Model")`, never by importing them: the import gives today's model, not the one at this point of the history.
- Update in batches (`iterator(chunk_size=...)`, `bulk_update(..., batch_size=...)`) and avoid model methods and signals, which the historical model does not have.

## Removing a field or a model

1. Ship code that no longer reads or writes it.
2. Remove it in a later migration. To drop it from Django's state while the column stays for a while, use `SeparateDatabaseAndState`.

## Checks

- `python manage.py makemigrations --check` (no missing migration), `python manage.py migrate --check` (nothing unapplied), and apply then reverse the new migration on the test database: `python manage.py migrate <app> <previous>` then `python manage.py migrate <app>`.
- `python manage.py optimizemigration <app> <number> --check` when a migration grew from several edits.
- Squash only when the task asks for it: squashing rewrites history other branches depend on.
