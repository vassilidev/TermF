#!/bin/sh
set -e
mkdir -p migrations
cat > migrations/0001_create_orders.sql <<'EOF'
CREATE TABLE customers (id bigserial PRIMARY KEY, email text NOT NULL);
CREATE TABLE orders (
  id bigserial PRIMARY KEY,
  total_cents bigint NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);
EOF
