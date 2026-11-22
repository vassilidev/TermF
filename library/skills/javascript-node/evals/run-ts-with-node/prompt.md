---
description: Running a .ts file with plain Node 26 needs erasable syntax, explicit .ts imports and `#` subpath imports instead of tsconfig paths.
max_turns: 12
allowed_tools: [Read, Glob, Grep, Skill]
---
We're on Node 26.13 and I'd like to drop `tsx` and run our migration script with plain `node scripts/migrate.ts`. package.json has `"type": "module"`, tsconfig has `"paths": { "@/lib/*": ["./src/lib/*"] }`. The script:

```ts
import { log } from "@/lib/log";
import { connect, Db } from "./db";

enum Step { Schema = "schema", Seed = "seed" }

export async function migrate(db: Db, step: Step) {
  log(`running ${step}`);
  await db.run(step);
}

await migrate(await connect(), Step.Schema);
```

Right now it crashes on start. What exactly do I need to change in this file and in package.json so `node scripts/migrate.ts` works? Show the changed code.
