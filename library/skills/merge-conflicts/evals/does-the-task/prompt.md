---
description: Resolving a conflict from merging the base into a feature branch, by both sides' intent.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

I merged `main` into my branch `feature/export` and git stopped: `src/invoice.ts` is in conflict. On my side I added a CSV export to `formatInvoice`; I don't know what changed on `main`. Which commands show me what each side did and wanted, and how do I finish the resolution safely?
