---
description: An N+1 query in a Django view, which is application query work, not a schema change.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

In our Django app, the order list view loops over `Order.objects.all()` and reads `order.customer.email` for each row, which fires one query per order. How do I fetch the customers in the same query?
