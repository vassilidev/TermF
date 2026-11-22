---
description: Reviewing a handler that loads an object by id without checking its owner.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Can you do a quick security review of this endpoint before we merge? Invoices belong to accounts; a user belongs to one account.

```ts
app.get("/api/invoices/:id", requireLogin, async (req, res) => {
  const invoice = await prisma.invoice.findUnique({ where: { id: req.params.id } });
  if (!invoice) return res.status(404).end();
  res.json(invoice);
});
```
