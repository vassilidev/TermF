// No imports, so `pnpm test` runs it in plain Node.

export type Health = "connected" | "failed" | "pending" | "needsauth" | "unknown";
export type Access = { roles: string[]; write: boolean };
/** A connector of the project's file; `access` null: a server set up before connectors, given to every session. */
export type Connector = { name: string; account: boolean; target: string; access: Access | null; keys: boolean };
export type Listing = { connectors: Connector[]; roles: { name: string; prod: boolean }[] };
/** The Claude account's connectors, as `claude mcp list` sees them. */
export type Account = { connectors: { name: string; health: Health }[]; error: string | null };

/**
 * One line of the section. `roles` null: every session but the CTO's and the expert's; `write` null: TermF sets
 * no bound; `health`: known for an account connector only, once `claude mcp list` answered.
 */
export type Row = {
  name: string;
  label: string;
  target: string;
  account: boolean;
  roles: string[] | null;
  write: boolean | null;
  health: Health | null;
  /** In the project's file: its roles and writing can be set, and it can be removed. */
  configured: boolean;
  keys: boolean;
};

const PREFIX = "claude.ai ";
const label = (name: string) => (name.startsWith(PREFIX) ? name.slice(PREFIX.length) : name);

/** The project's connectors, then the account's it does not give yet, one click from being given. */
export function rows(listing: Listing, account: Account | null): Row[] {
  const known = account?.connectors ?? [];
  const own: Row[] = listing.connectors.map((c) => ({
    name: c.name,
    label: label(c.name),
    target: c.target,
    account: c.account,
    roles: c.access?.roles ?? null,
    write: c.access?.write ?? null,
    health: c.account ? (known.find((k) => k.name === c.name)?.health ?? null) : null,
    configured: true,
    keys: c.keys,
  }));
  const offered: Row[] = known
    .filter((k) => !listing.connectors.some((c) => c.name === k.name))
    .map((k) => ({
      name: k.name,
      label: label(k.name),
      target: "",
      account: true,
      roles: null,
      write: null,
      health: k.health,
      configured: false,
      keys: false,
    }));
  return [...own, ...offered];
}

/** Why the CTO's and the expert's conversations, which load only what TermF passes them, do not get it. */
export function strictNote(row: Row): "connStrictAccount" | "connStrictKeys" | null {
  if (!row.roles?.some((r) => r === "cto" || r === "expert")) return null;
  if (row.account) return "connStrictAccount";
  return row.keys ? "connStrictKeys" : null;
}
