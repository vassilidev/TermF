#!/bin/sh
set -e
mkdir -p src
cat > src/server.js <<'JS'
import express from "express";
import { markOrderPaid } from "./orders.js";

const app = express();
app.use(express.json());

app.get("/health", (_req, res) => res.send("ok"));

app.listen(4242);
JS
cat > src/orders.js <<'JS'
export async function markOrderPaid(orderId) {
  // updates the order row; not idempotent on its own
}
JS
