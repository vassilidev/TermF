import { createHash, randomBytes, scrypt, timingSafeEqual } from "node:crypto";
import { promisify } from "node:util";
import type { Db } from "./db.ts";
import { HttpError, badRequest } from "./http.ts";

const scryptAsync = promisify(scrypt);

export interface User {
  id: number;
  username: string;
  email: string | null;
}

const USERNAME = /^[a-z0-9_]{3,32}$/;
const EMAIL = /^[^@\s]+@[^@\s]+\.[^@\s]+$/;
const MIN_PASSWORD = 10;
const KEY_LENGTH = 64;

// Checked against when the username is unknown, so that a wrong username and a wrong password take as long.
const DUMMY_HASH = `${"00".repeat(16)}:${"00".repeat(KEY_LENGTH)}`;

/** Stored as `<salt hex>:<scrypt hex>`. */
async function hashPassword(password: string): Promise<string> {
  const salt = randomBytes(16);
  const hash = await scryptAsync(password, salt, KEY_LENGTH);
  return `${salt.toString("hex")}:${hash.toString("hex")}`;
}

async function checkPassword(password: string, stored: string): Promise<boolean> {
  const [salt, hash] = stored.split(":");
  const actual = await scryptAsync(password, Buffer.from(salt, "hex"), KEY_LENGTH);
  return timingSafeEqual(actual, Buffer.from(hash, "hex"));
}

const hashToken = (token: string): string => createHash("sha256").update(token).digest("hex");

function cleanUsername(value: unknown): string {
  return typeof value === "string" ? value.trim().toLowerCase() : "";
}

export async function signUp(db: Db, body: any): Promise<User> {
  const username = cleanUsername(body.username);
  if (!USERNAME.test(username)) throw badRequest("invalid username");
  if (typeof body.password !== "string" || body.password.length < MIN_PASSWORD) throw badRequest("password too short");
  const email = body.email ?? null;
  if (email !== null && (typeof email !== "string" || !EMAIL.test(email))) throw badRequest("invalid email");
  const passwordHash = await hashPassword(body.password);
  try {
    const { lastInsertRowid } = db
      .prepare("INSERT INTO users (username, password_hash, email) VALUES (?, ?, ?)")
      .run(username, passwordHash, email);
    return { id: Number(lastInsertRowid), username, email };
  } catch (err: any) {
    if (String(err?.message).includes("UNIQUE")) throw new HttpError(409, "username taken");
    throw err;
  }
}

export async function signIn(db: Db, body: any): Promise<{ token: string; user: User }> {
  const row = db
    .prepare("SELECT id, username, email, password_hash FROM users WHERE username = ?")
    .get(cleanUsername(body.username)) as (User & { password_hash: string }) | undefined;
  const password = typeof body.password === "string" ? body.password : "";
  const valid = await checkPassword(password, row?.password_hash ?? DUMMY_HASH);
  if (!row || !valid) throw new HttpError(401, "invalid credentials");
  const token = randomBytes(32).toString("base64url");
  db.prepare("INSERT INTO sessions (token_hash, user_id) VALUES (?, ?)").run(hashToken(token), row.id);
  return { token, user: { id: row.id, username: row.username, email: row.email } };
}

export function signOut(db: Db, req: any): void {
  requireUser(db, req);
  db.prepare("DELETE FROM sessions WHERE token_hash = ?").run(hashToken(bearer(req)!));
}

/** The signed-in user of the request, from its `Authorization: Bearer <token>` header. */
export function requireUser(db: Db, req: any): User {
  const token = bearer(req);
  const user = token
    ? (db
        .prepare(
          `SELECT users.id, users.username, users.email FROM sessions
           JOIN users ON users.id = sessions.user_id WHERE sessions.token_hash = ?`,
        )
        .get(hashToken(token)) as User | undefined)
    : undefined;
  if (!user) throw new HttpError(401, "authentication required");
  return { id: user.id, username: user.username, email: user.email };
}

function bearer(req: any): string | undefined {
  return /^Bearer ([A-Za-z0-9_-]{16,})$/.exec(req.headers.authorization ?? "")?.[1];
}
