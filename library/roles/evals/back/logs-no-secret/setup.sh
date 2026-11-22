#!/bin/sh
set -e
mkdir -p src/auth
cat > src/auth/login.ts <<'EOF'
import { verify } from "./hash";
import { logger } from "../logger";
import { db } from "../db";

export async function login(email: string, password: string, ip: string) {
  const user = await db.user.findUnique({ where: { email } });
  if (!user || !(await verify(user.passwordHash, password))) {
    return { ok: false as const };
  }
  logger.info("sign-in", { userId: user.id, ip });
  return { ok: true as const, userId: user.id };
}
EOF
