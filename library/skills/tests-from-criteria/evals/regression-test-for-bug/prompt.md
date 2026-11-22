---
description: A bug fix comes with a regression test that is shown failing before the fix.
max_turns: 12
allowed_tools: [Read, Glob, Grep, Skill]
---
Users report that when they apply the coupon "WELCOME10" twice in a row, the cart gets 20% off instead of 10%. Here's the code (Node, tests run with `npm test`, Vitest):

```js
// src/cart.js
export function applyCoupon(cart, code) {
  const coupon = COUPONS[code];
  if (!coupon) throw new Error("unknown coupon");
  cart.discounts.push({ code, percent: coupon.percent });
  return cart;
}
export function total(cart) {
  const gross = cart.lines.reduce((s, l) => s + l.price * l.qty, 0);
  const pct = cart.discounts.reduce((s, d) => s + d.percent, 0);
  return Math.round(gross * (100 - pct)) / 100;
}
const COUPONS = { WELCOME10: { percent: 10 } };
```

Fix it and add the test that should go with the fix. Tell me exactly how you'd check the test is worth keeping. Answer in the reply, no need to create files.
