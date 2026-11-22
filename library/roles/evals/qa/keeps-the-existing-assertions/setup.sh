#!/bin/sh
set -e
mkdir -p src
cat > package.json <<'EOF'
{ "name": "shop", "private": true, "type": "module", "scripts": { "test": "vitest run" }, "devDependencies": { "vitest": "^4.1.0" } }
EOF
cat > src/cart.ts <<'EOF'
export type Cart = { subtotalCents: number; discountPercent: number };

export function checkoutTotal(cart: Cart): number {
  const discount = Math.round((cart.subtotalCents * cart.discountPercent) / 100);
  return cart.subtotalCents;
}
EOF
cat > src/cart.test.ts <<'EOF'
import { expect, test } from "vitest";
import { checkoutTotal } from "./cart";

test("charges the discounted total", () => {
  expect(checkoutTotal({ subtotalCents: 10000, discountPercent: 10 })).toBe(9000);
});
EOF
