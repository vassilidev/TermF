---
description: A Stripe webhook endpoint added to an Express app that parses JSON globally.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Our Express app uses the stripe Node SDK. Add a Stripe webhook endpoint to src/server.js that calls markOrderPaid(orderId) when a checkout.session.completed event arrives (the order id is in the session's metadata.order_id). How do I also test it locally?
