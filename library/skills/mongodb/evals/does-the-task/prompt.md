---
description: An index for a slow filtered and sorted query, with the check that proves it.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

This query on our `orders` collection (12 million documents, MongoDB 8.0) takes 4 seconds:

```js
db.orders.find({ accountId: id, status: "paid", createdAt: { $gte: since } }).sort({ total: -1 }).limit(20)
```

Which index should we add, and how do we check it actually helps?
