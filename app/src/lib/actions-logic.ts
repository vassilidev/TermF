// No imports, so `pnpm test` runs it in plain Node.

/**
 * An argument of a screen's action. `secret`: TermF's card asks the user for it and it goes to the vault or the
 * keychain; the agent never gives it and never sees it. `object`: a map of texts (a workflow's inputs); `array`: a
 * list of texts.
 */
export type Field = {
  type: "string" | "integer" | "boolean" | "object" | "array";
  description: string;
  enum?: string[];
  secret?: boolean;
};

export type Args = Record<string, unknown>;

/** What an agent knows of an action, in English. */
export type Spec = {
  id: string;
  description: string;
  fields: Record<string, Field>;
  required?: string[];
  /** The secret fields the card asks for with these arguments; all of them by default. */
  asks?: (args: Args) => string[];
};

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);

/** The secret fields the card asks for. */
export function secretsOf(spec: Spec, args: Args): string[] {
  const all = Object.keys(spec.fields).filter((k) => spec.fields[k].secret);
  return spec.asks ? spec.asks(args).filter((k) => all.includes(k)) : all;
}

/** An action as `actions` lists it to an agent: a secret field only by its name, under `askedToUser`. */
export function forAgent(spec: Spec) {
  const properties: Record<string, unknown> = {};
  const asked: { field: string; description: string }[] = [];
  for (const [name, f] of Object.entries(spec.fields)) {
    if (f.secret) asked.push({ field: name, description: f.description });
    else
      properties[name] = {
        type: f.type,
        description: f.description,
        ...(f.enum ? { enum: f.enum } : {}),
        ...(f.type === "object" ? { additionalProperties: { type: "string" } } : {}),
        ...(f.type === "array" ? { items: { type: "string" } } : {}),
      };
  }
  const required = (spec.required ?? []).filter((k) => !spec.fields[k]?.secret);
  return {
    id: spec.id,
    description: spec.description,
    arguments: { type: "object", properties, required, additionalProperties: false },
    ...(asked.length ? { askedToUser: asked } : {}),
  };
}

const kinds = { string: "a text", integer: "an integer", boolean: "true or false", object: "an object of texts", array: "a list of texts" };

function typed(f: Field, value: unknown): boolean {
  switch (f.type) {
    case "boolean":
      return typeof value === "boolean";
    case "integer":
      return Number.isInteger(value);
    case "object":
      return isObject(value) && Object.values(value).every((v) => typeof v === "string");
    case "array":
      return Array.isArray(value) && value.every((v) => typeof v === "string");
    default:
      return typeof value === "string";
  }
}

/** Why an agent's call is refused, in words it reads; null when it may go to the user. */
export function refused(spec: Spec, args: unknown, hints: unknown): string | null {
  if (!isObject(args)) return "args: an object";
  const secret = Object.keys(spec.fields).filter((k) => spec.fields[k].secret);
  for (const [name, value] of Object.entries(args)) {
    const f = spec.fields[name];
    if (!f) return `${spec.id} has no argument ${name}`;
    if (f.secret) return `${name} is asked to the user by TermF, never given by you: say in hints where they find it`;
    if (value === null) continue;
    if (!typed(f, value)) return `${name}: ${kinds[f.type]}`;
    if (f.enum && !f.enum.includes(value as string)) return `${name}: one of ${f.enum.join(", ")}`;
  }
  for (const name of spec.required ?? []) {
    const value = args[name];
    if (!spec.fields[name]?.secret && (value === undefined || value === null || value === "")) return `${name} is required`;
  }
  if (hints === undefined || hints === null) return null;
  if (!isObject(hints)) return "hints: an object";
  for (const [name, text] of Object.entries(hints)) {
    if (!secret.includes(name)) return `hints are for the fields asked to the user (${secret.join(", ") || "none here"}), not ${name}`;
    if (typeof text !== "string" || text.length > 200) return `hints.${name}: a text of 200 characters at most`;
  }
  return null;
}

/**
 * What an action that keeps or reads a secret of the vault waits for, as the agent reads it: the action that settles
 * it, to propose first. Null once the vault is open.
 */
export function vaultNeeds(status: { initialized: boolean; unlocked: boolean } | null): string | null {
  return (
    noVault(status) ??
    (status?.unlocked ? null : "The vault is locked: propose vault.unlock first (the user types its master password in TermF's card), then this action again.")
  );
}

/** What an action that keeps a secret waits for: a vault. A locked one is no prerequisite: TermF's card opens it when the action runs. */
export function noVault(status: { initialized: boolean } | null): string | null {
  return status?.initialized
    ? null
    : "There is no vault yet: propose vault.create first (the user picks its master password in TermF's card), then this action again.";
}

/**
 * Why an action's run stopped at the vault, which TermF's card settles in place before running it again: locked, or
 * prod asking for the master password again (5 minutes). Null for any other error.
 */
export function vaultBlock(error: unknown): "unlock" | "reauth" | null {
  const text = String(error);
  if (/the vault is locked/.test(text)) return "unlock";
  return /needs the master password again/.test(text) ? "reauth" : null;
}

/** What the agent proposes for a secret the vault lacks: vault.set_secret, whose card asks the user for it. */
export const proposeSecret = (name: string, scope: string, then: string) =>
  `The vault has no ${name} (${scope}) for this project: propose vault.set_secret {name: "${name}", scope: "${scope}"}, whose card asks the user for its value (you never see it), then ${then}.`;

/** The next step for an action whose run failed on a secret the vault lacks (`no secret X in the vault (env…`). */
export function secretMissing(error: unknown): string | null {
  const m = /no secret ([A-Za-z_][\w.-]*) in the vault \((dev|staging|prod)\b/.exec(String(error));
  return m ? proposeSecret(m[1], m[2], "this action again") : null;
}

/** The master password a new vault takes, as typed twice in the card; why not, in words the user reads. */
export function newMaster(secrets: Record<string, string>): { password: string } | { why: "short" | "differ" } {
  const password = secrets.password ?? "";
  if (password.length < 8) return { why: "short" };
  if (password !== (secrets.confirm ?? "")) return { why: "differ" };
  return { password };
}

/** The same text whatever the order of an object's keys. */
export function canon(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canon).join(",")}]`;
  if (isObject(value))
    return `{${Object.keys(value)
      .sort()
      .map((k) => `${JSON.stringify(k)}:${canon(value[k])}`)
      .join(",")}}`;
  return JSON.stringify(value) ?? "null";
}

/**
 * What the user approved in a conversation's permission card, with what they typed there, kept until the call they
 * allowed reaches TermF (two minutes at most): one card for both. Taken once.
 */
export class Approved<T> {
  #kept = new Map<string, { at: number; value: T }>();
  #ttl: number;

  constructor(ttl = 120_000) {
    this.#ttl = ttl;
  }

  #key = (session: number, action: string, args: unknown) => `${session} ${action} ${canon(args ?? {})}`;

  keep(session: number, action: string, args: unknown, value: T, now = Date.now()) {
    for (const [key, kept] of this.#kept) if (now - kept.at > this.#ttl) this.#kept.delete(key);
    this.#kept.set(this.#key(session, action, args), { at: now, value });
  }

  take(session: number, action: string, args: unknown, now = Date.now()): T | undefined {
    const key = this.#key(session, action, args);
    const kept = this.#kept.get(key);
    this.#kept.delete(key);
    return kept && now - kept.at <= this.#ttl ? kept.value : undefined;
  }
}
