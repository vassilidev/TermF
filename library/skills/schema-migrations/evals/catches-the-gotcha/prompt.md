---
description: An Alembic migration that builds an index concurrently fails inside the migration's transaction.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Our Alembic 1.20 migration fails on PostgreSQL with "CREATE INDEX CONCURRENTLY cannot run inside a transaction block". Here is its upgrade():

```python
def upgrade():
    op.create_index("ix_orders_customer_id", "orders", ["customer_id"], postgresql_concurrently=True)
```

How do I fix it without dropping CONCURRENTLY? The orders table is large and busy.
