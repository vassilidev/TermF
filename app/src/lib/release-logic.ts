export type Os = "mac" | "windows" | "linux";
export type ToolId = "git" | "claude" | "codex" | "gemini" | "gh" | "chrome";

export type Tool = { name: string; version: string | null; path: string | null };
export type Install = { path: string; version: string | null; origin: "setting" | "native" | "path" };
export type Chrome = { path: string; testing: boolean; version: string | null };
export type Agent = { install: Install | null; others: Install[]; minimum: string; tooOld: boolean };
export type Tools = { git: Tool; gh: Tool; claude: Agent; codex: Agent; gemini: Agent; chrome: Chrome | null };

const everywhere = (command: string) => ({ mac: command, windows: command, linux: command });

/** Each tool's official page, and its install command where the page gives one for this system. */
export const installs: Record<ToolId, { url: string; command: Partial<Record<Os, string>> }> = {
  git: {
    url: "https://git-scm.com/downloads",
    command: { mac: "xcode-select --install", windows: "winget install --id Git.Git -e --source winget", linux: "sudo apt install git" },
  },
  claude: {
    url: "https://code.claude.com/docs/en/setup",
    command: {
      mac: "curl -fsSL https://claude.ai/install.sh | bash",
      linux: "curl -fsSL https://claude.ai/install.sh | bash",
      windows: "irm https://claude.ai/install.ps1 | iex",
    },
  },
  codex: { url: "https://github.com/openai/codex", command: everywhere("npm install -g @openai/codex") },
  gemini: { url: "https://github.com/google-gemini/gemini-cli", command: everywhere("npm install -g @google/gemini-cli") },
  gh: { url: "https://cli.github.com", command: { mac: "brew install gh", windows: "winget install --id GitHub.cli" } },
  chrome: { url: "https://www.google.com/chrome/", command: {} },
};

export function osOf(platform: string): Os {
  return platform.startsWith("Mac") ? "mac" : platform.startsWith("Win") ? "windows" : "linux";
}

export const required: ToolId[] = ["git", "claude"];

export function found(tools: Tools, id: ToolId): boolean {
  if (id === "claude" || id === "codex" || id === "gemini") return tools[id].install !== null && !tools[id].tooOld;
  if (id === "chrome") return tools.chrome !== null;
  return tools[id].path !== null;
}

/** What TermF cannot work without and does not have: git, a recent enough Claude Code. */
export function missingRequired(tools: Tools): ToolId[] {
  return required.filter((id) => !found(tools, id));
}

/** Agents at work, which an update's restart would stop: tiles that work, headless sessions that work. */
export function working(tiles: { running: boolean; status: string | null }[], headless: { state: string }[]): number {
  return tiles.filter((s) => s.running && s.status === "working").length + headless.filter((a) => a.state === "working").length;
}

/** "42 %" of a download, or nothing while its size is unknown. */
export function percent(done: number, total: number | null): string {
  return total ? `${Math.min(100, Math.floor((done / total) * 100))} %` : "";
}
