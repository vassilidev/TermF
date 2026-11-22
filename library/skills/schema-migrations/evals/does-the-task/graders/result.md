---
type: llm
---

PASS if the answer spreads the rename over several deploys: first add `full_name` (nullable or without a table rewrite) and have the code write both columns, then backfill existing rows in batches outside the schema migration, then switch reads to `full_name`, and only in a later deploy drop `name`; it sets a lock timeout or says which lock each step takes, and gives each migration a way back.
FAIL if it uses a single `rename_column :users, :name, :full_name`, drops `name` in the same deploy that adds `full_name`, or backfills all 30 million rows in one UPDATE inside the migration.
