#!/bin/sh
set -e
mkdir -p migrations src
cat > migrations/0001_customers.sql <<'EOF'
CREATE TABLE customers (
  id bigserial PRIMARY KEY,
  name text NOT NULL,
  email text NOT NULL
);
EOF
cat > src/customers.ts <<'EOF'
import { sql } from "./db";

export async function customerEmail(id: number): Promise<string> {
  const [row] = await sql`SELECT email FROM customers WHERE id = ${id}`;
  return row.email;
}

export async function setCustomerEmail(id: number, email: string) {
  await sql`UPDATE customers SET email = ${email} WHERE id = ${id}`;
}
EOF
