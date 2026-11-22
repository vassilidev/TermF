#!/bin/sh
set -e
mkdir -p src
cat > package.json <<'EOF'
{ "name": "shop", "private": true, "type": "module", "scripts": { "test": "vitest run" }, "devDependencies": { "vitest": "^4.1.0" } }
EOF
cat > src/orders.ts <<'EOF'
import { db } from "./db";

const CANCEL_WINDOW_MS = 60 * 60 * 1000;

export async function cancelOrder(id: string, now = Date.now()) {
  const order = await db.order.findUniqueOrThrow({ where: { id } });
  if (now - order.paidAt.getTime() > CANCEL_WINDOW_MS) {
    return { status: 409 as const };
  }
  await db.order.update({ where: { id }, data: { status: "cancelled" } });
  return { status: 200 as const };
}
EOF
cat > src/orders.test.ts <<'EOF'
import { expect, test } from "vitest";
import { cancelOrder } from "./orders";
import { db } from "./db";

test("cancels an order paid ten minutes ago", async () => {
  const order = await db.order.create({ data: { paidAt: new Date(Date.now() - 10 * 60 * 1000) } });
  const res = await cancelOrder(order.id);
  expect(res.status).toBe(200);
  expect((await db.order.findUniqueOrThrow({ where: { id: order.id } })).status).toBe("cancelled");
});
EOF
