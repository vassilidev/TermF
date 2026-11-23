// No imports, so `pnpm test` runs it in plain Node.

export type Family = "color" | "font" | "text" | "weight" | "leading" | "spacing" | "radius" | "shadow" | "motion" | "other";
export type Level = "error" | "warn" | "info";
export type Theme = "light" | "dark";
export type Kind =
  | "hardcoded"
  | "offScale"
  | "near"
  | "duplicate"
  | "unused"
  | "undefined"
  | "dark"
  | "contrast"
  | "font"
  | "component";
export type Def = { value: string; file: string; line: number };
export type Token = {
  name: string;
  path: string | null;
  family: Family;
  format: "css" | "tailwind" | "scss" | "dtcg" | "config";
  value: string;
  file: string;
  line: number;
  light: boolean;
  dark: Def | null;
  color: string | null;
  darkColor: string | null;
  px: number | null;
  alias: boolean;
  uses: number;
  generated: boolean;
};
export type Loc = { file: string; line: number; text: string };
export type Finding = {
  id: string;
  kind: Kind;
  level: Level;
  family: Family | null;
  subject: string;
  other: string[];
  measure: number | null;
  theme: Theme | null;
  at: Loc[];
  count: number;
};
export type Pair = { fg: string; bg: string; theme: Theme; fgColor: string; bgColor: string; ratio: number; at: Loc | null };
export type Component = {
  name: string;
  file: string;
  framework: string;
  variants: { prop: string; values: string[] }[];
  uses: number;
  usedIn: string[];
  stories: string[];
};
export type Note = { kind: "unread" | "truncated" | "large" | "packages" | "published"; file: string | null; detail: string | null };
export type Brand = {
  summary: string | null;
  logos: { file: string; svg: string }[];
  exports: { file: string; size: number; width: number | null; height: number | null }[];
  missing: ("favicon" | "appleTouch" | "og" | "appIcon")[];
  /** `brand/guidelines.html` exists: it can be exported. */
  printable: boolean;
  /** Its preview, null over 4 MB. */
  guidelines: string | null;
  pdf: boolean;
};
export type FontFace = { weight: string; style: string; src: string; file: string };
export type Font = {
  family: string;
  origin: "repo" | "package" | "service" | "missing";
  faces: FontFace[];
  file: string;
  line: number;
  /** The web font service's stylesheet, fetched only when the user asks. */
  url: string | null;
};
export type Role = "body" | "heading" | "code";
export type FontUse = { role: Role; value: string; file: string; line: number };
export type Design = {
  tokens: Token[];
  pairs: Pair[];
  components: Component[];
  findings: Finding[];
  sources: string[];
  notes: Note[];
  dark: boolean;
  storybook: boolean;
  designMd: boolean;
  brand: Brand | null;
  files: number;
  fonts: Font[];
  fontUses: FontUse[];
};

export type Section = "color" | "type" | "spacing" | "depth" | "other" | "components" | "issues" | "brand";

/** Font family, size, weight and line height make one section; spacing and radii another; shadows and motion a third. */
export function sectionOf(family: Family): Section {
  if (family === "font" || family === "text" || family === "weight" || family === "leading") return "type";
  if (family === "radius") return "spacing";
  if (family === "shadow" || family === "motion") return "depth";
  return family;
}

export function tokensOf(tokens: Token[], section: Section): Token[] {
  return tokens.filter((t) => sectionOf(t.family) === section);
}

export function tally(findings: Finding[]): Record<Level, number> {
  const out = { error: 0, warn: 0, info: 0 };
  for (const f of findings) out[f.level]++;
  return out;
}

/** A value rather than a token: `13px`, `#e11d48`. */
export function literal(subject: string): boolean {
  return /^[#\d.]/.test(subject);
}

/** The colors to show next to a finding: its literal or token colors, in the theme it is about. */
export function swatches(f: Finding, tokens: Token[]): string[] {
  if (f.family !== "color") return [];
  const of = (name: string) => {
    if (name.startsWith("#")) return name;
    const t = tokens.find((t) => t.name === name);
    return (f.theme === "dark" ? (t?.darkColor ?? t?.color) : t?.color) ?? null;
  };
  const names = f.kind === "duplicate" ? [f.subject] : f.kind === "unused" ? [] : [f.subject, ...f.other.slice(0, 1)];
  return names.map(of).filter((c): c is string => c !== null);
}

type T = (key: string, vars?: Record<string, string | number>) => string;

/** The finding as one sentence. */
export function describe(f: Finding, t: T, number: (n: number) => string): string {
  // `13px` could be a padding or a font size: say which.
  const sized = literal(f.subject) && (f.family === "spacing" || f.family === "text" || f.family === "radius");
  const vars = {
    subject: sized ? `${f.subject} (${t(`designOf${f.family![0].toUpperCase()}${f.family!.slice(1)}`)})` : f.subject,
    other: f.other.join(", "),
    first: f.other[0] ?? "",
    measure: f.measure === null ? "" : number(f.measure),
  };
  let text: string;
  switch (f.kind) {
    case "hardcoded":
      text = t(f.other.length === 0 ? "designKindHardcodedLone" : f.measure === null ? "designKindHardcoded" : "designKindHardcodedNear", vars);
      break;
    case "offScale":
      text = literal(f.subject)
        ? t("designKindOffScale", vars)
        : t("designKindOffScaleToken", { ...vars, value: f.other[0] ?? "", base: f.other[1] ?? "" });
      break;
    case "dark":
      text = t(f.other[0] === "dark-only" ? "designKindDarkOnly" : "designKindDark", vars);
      break;
    case "component":
      text = t(f.level === "warn" ? "designKindComponentName" : "designKindComponentShape", vars);
      break;
    case "unused":
      text = t("designKindUnused", { ...vars, count: f.other.length });
      break;
    default:
      text = t(`designKind${f.kind[0].toUpperCase()}${f.kind.slice(1)}`, vars);
  }
  return f.theme ? `${text} (${t(f.theme === "dark" ? "designThemeDark" : "designThemeLight")})` : text;
}

/** The message to the CTO: what to fix and where, for the user to read and send. */
export function request(findings: Finding[], repo: string, t: T, number: (n: number) => string): string {
  const lines = findings.map((f) => {
    const where = f.at
      .slice(0, 3)
      .map((a) => `${a.file}:${a.line}`)
      .join(", ");
    const more = f.count > 3 ? `, ${t("designMore", { n: f.count - 3 })}` : "";
    return `- ${describe(f, t, number)}${where ? ` (${where}${more})` : ""}`;
  });
  return [t("designFixRequest", { repo }), ...lines].join("\n");
}

/** WCAG AA for normal text. */
export function passes(ratio: number): boolean {
  return ratio >= 4.5;
}

/** Reads that overlap (the repository changed meanwhile): only the last one started may show its answer. */
export function latest() {
  let last = 0;
  return { start: () => ++last, isLast: (read: number) => read === last };
}

/** Keeps only the chosen findings that are still there after a new scan. */
export function stillThere(chosen: Set<string>, findings: Finding[]): Set<string> {
  const ids = new Set(findings.map((f) => f.id));
  return new Set([...chosen].filter((id) => ids.has(id)));
}

/** A file size the way people of the language read it: `12 ko`, `12 kB`. */
export function size(bytes: number, lang: string): string {
  const [value, unit] = bytes < 1024 ? [bytes, "byte"] : bytes < 1024 * 1024 ? [bytes / 1024, "kilobyte"] : [bytes / 1024 / 1024, "megabyte"];
  return new Intl.NumberFormat(lang, { style: "unit", unit: unit as string, maximumFractionDigits: 1 }).format(value as number);
}

/** The families of a `font-family` stack, unquoted: `"Inter Variable", system-ui` → `["Inter Variable", "system-ui"]`. */
export function families(stack: string): string[] {
  return (stack.match(/"[^"]*"|'[^']*'|[^,]+/g) ?? []).map((f) => f.trim().replace(/^["']|["']$/g, "").trim()).filter(Boolean);
}

const GENERIC = new Set(["serif", "sans-serif", "monospace", "cursive", "fantasy", "math", "emoji", "fangsong", "system-ui", "ui-serif", "ui-sans-serif", "ui-monospace", "ui-rounded", "-apple-system", "blinkmacsystemfont"]);

/** A family the browser maps to the system's own fonts rather than a typeface of the project. */
export function generic(family: string): boolean {
  return GENERIC.has(family.toLowerCase());
}

/** The stack a value gives, `var(--x)` resolved through the tokens. */
export function stackOf(value: string, tokens: Token[], depth = 0): string {
  const m = /^var\(\s*(--[\w-]+)\s*(?:,\s*(.*))?\)$/.exec(value.trim());
  if (!m || depth > 8) return value.trim();
  const t = tokens.find((t) => t.name === m[1]);
  return t ? stackOf(t.value, tokens, depth + 1) : m[2] ? stackOf(m[2], tokens, depth + 1) : value.trim();
}

/** A stack with each family the view loaded under another name (`aliases`, lowercase keys) given that name. */
export function aliased(stack: string, aliases: Map<string, string>): string {
  return families(stack)
    .map((f) => {
      const alias = aliases.get(f.toLowerCase());
      return alias ? `"${alias}"` : generic(f) ? f : `"${f.replace(/"/g, "")}"`;
    })
    .join(", ");
}

export type FontState = "loaded" | "service" | "installed" | "system" | "missing";

/**
 * How the first family of a stack is drawn: from the project's files (`loaded`), not fetched yet from its web font
 * service (`service`), only because this machine has it (`installed`), the system's font on purpose (`system`), or
 * not at all (`missing`); `shown` is the family the reader sees instead, empty for the browser's default.
 */
export function fontState(
  stack: string,
  fonts: Font[],
  loaded: (family: string) => boolean,
  installed: (family: string) => boolean,
): { family: string; state: FontState; shown: string; font: Font | null } {
  const [first = "", ...rest] = families(stack);
  const font = fonts.find((f) => f.family.toLowerCase() === first.toLowerCase()) ?? null;
  const fallback = rest.find((f) => generic(f) || loaded(f) || installed(f)) ?? "";
  if (generic(first)) return { family: first, state: "system", shown: first, font };
  if (loaded(first)) return { family: first, state: "loaded", shown: first, font };
  if (font?.origin === "service" && font.url) return { family: first, state: "service", shown: fallback, font };
  if (installed(first)) return { family: first, state: "installed", shown: first, font };
  return { family: first, state: "missing", shown: fallback, font };
}

/** The font of the body, the headings and code: as the stylesheets give it, else as the tokens are named. */
export function roles(uses: FontUse[], tokens: Token[]): Record<Role, { stack: string; from: string } | null> {
  const fonts = tokens.filter((t) => t.family === "font");
  const named = (words: string[]) => {
    for (const w of words) {
      const t = fonts.find((t) => t.name.toLowerCase().replace(/^(--|\$)/, "").split(/[-._]/).includes(w));
      if (t) return { stack: stackOf(t.value, tokens), from: t.name };
    }
    return null;
  };
  const used = (role: Role) => {
    const u = uses.find((u) => u.role === role);
    return u ? { stack: stackOf(u.value, tokens), from: `${u.file}:${u.line}` } : null;
  };
  const first = fonts[0] ? { stack: stackOf(fonts[0].value, tokens), from: fonts[0].name } : null;
  const body = used("body") ?? named(["body", "text", "sans", "base", "default", "ui", "copy"]) ?? first;
  const heading = used("heading") ?? named(["heading", "headings", "display", "title", "headline", "serif"]) ?? body;
  return { body, heading, code: used("code") ?? named(["mono", "code", "monospace"]) };
}

/** The weights to show a family in: the weight tokens, else those its files cover (a variable range, in steps). */
export function weights(font: Font | null, tokens: Token[]): number[] {
  const own = tokens.map((t) => (t.family === "weight" ? Number(t.value) : 0)).filter((n) => n >= 100 && n <= 900);
  if (own.length) return [...new Set(own)].sort((a, b) => a - b);
  const out = new Set<number>();
  for (const face of font?.faces ?? []) {
    const [lo, hi = lo] = face.weight.split(/\s+/).map((w) => (w === "normal" ? 400 : w === "bold" ? 700 : Number(w)));
    if (lo === hi) out.add(lo);
    else for (const w of [300, 400, 500, 600, 700, 800]) if (w >= lo && w <= hi) out.add(w);
  }
  return out.size ? [...out].filter(Boolean).sort((a, b) => a - b) : [400, 700];
}

/** Colors of one scale (`--brand-100` … `--brand-900`) together, the others one by one, in the order they came. */
export function ramps(tokens: Token[]): { name: string; tokens: Token[] }[] {
  const base = (t: Token) => t.name.replace(/[-.]?\d+$/, "");
  const count = new Map<string, number>();
  for (const t of tokens) if (/\d$/.test(t.name)) count.set(base(t), (count.get(base(t)) ?? 0) + 1);
  const out: { name: string; tokens: Token[] }[] = [];
  for (const t of tokens) {
    const key = /\d$/.test(t.name) && (count.get(base(t)) ?? 0) >= 3 ? base(t) : t.name;
    const group = out.find((g) => g.name === key);
    if (group) group.tokens.push(t);
    else out.push({ name: key, tokens: [t] });
  }
  return out;
}

export type Primitive = "button" | "field" | "card" | "badge" | "alert";

/** The kind of base component a name says, which the view can draw from the tokens alone. */
export function primitive(name: string): Primitive | null {
  const words = name.replace(/([a-z0-9])([A-Z])/g, "$1 $2").toLowerCase().split(/[\s_-]+/);
  const has = (re: RegExp) => words.some((w) => re.test(w));
  if (has(/^(button|btn)s?$/)) return "button";
  if (has(/^(input|field|textarea|select|combobox)s?$/)) return "field";
  if (has(/^(alert|banner|callout|notice|toast)s?$/)) return "alert";
  if (has(/^(badge|tag|chip|pill)s?$/)) return "badge";
  if (has(/^(card|panel|tile)s?$/)) return "card";
  return null;
}

export type Tone = "accent" | "soft" | "outline" | "ghost" | "danger" | "success" | "warning" | "info";

/** How a variant looks, from its name; an unknown name is the filled look first, then the soft one. */
export function tone(value: string, first: boolean): Tone {
  const looks: [RegExp, Tone][] = [
    [/^(primary|default|solid|filled|brand|accent|main|cta)$/, "accent"],
    [/^(destructive|danger|error|critical|negative|red)$/, "danger"],
    [/^(success|positive|green|ok|done)$/, "success"],
    [/^(warning|warn|caution|amber|yellow)$/, "warning"],
    [/^(info|information|blue|note)$/, "info"],
    [/^(outline|outlined|bordered|stroke)$/, "outline"],
    [/^(ghost|text|link|plain|quiet|transparent|bare|minimal)$/, "ghost"],
    [/^(secondary|soft|subtle|muted|tonal|neutral|light|gray|grey)$/, "soft"],
  ];
  return looks.find(([re]) => re.test(value.toLowerCase()))?.[1] ?? (first ? "accent" : "soft");
}

export type Kit = {
  bg: string;
  surface: string;
  text: string;
  muted: string;
  border: string;
  accent: string;
  onAccent: string;
  /** A quiet filled ground (a secondary button), and the text on it. */
  soft: string;
  onSoft: string;
  danger: string;
  success: string;
  warning: string;
  info: string;
  radius: string;
  radiusLarge: string;
  shadow: string;
  /** What comes from the project's tokens; the rest are neutral stand-ins. */
  own: string[];
};

/** The project's look for drawing its base components, read from its token names, in a theme. */
export function kit(tokens: Token[], dark: boolean): Kit {
  const own: string[] = [];
  const colors = tokens.filter((t) => t.family === "color" && t.color);
  const parts = (t: Token) => t.name.toLowerCase().replace(/^(--|\$)(color-|colors?\.)?/, "").split(/[-._]/);
  const step = (t: Token) => Number(parts(t).at(-1)) || 0;
  const color = (t: Token) => (dark ? (t.darkColor ?? t.color) : t.color) as string;
  const find = (key: keyof Kit, words: string[], avoid: string[], ok: (c: string) => boolean = () => true) => {
    for (const w of words) {
      const hits = colors.filter((t) => parts(t).includes(w) && !parts(t).some((p) => avoid.includes(p)) && ok(color(t)));
      // Of a scale, the step a filled control uses.
      hits.sort((a, b) => Math.abs(step(a) - 600) - Math.abs(step(b) - 600) || a.name.length - b.name.length);
      if (hits[0]) {
        own.push(key);
        return color(hits[0]);
      }
    }
    return null;
  };
  const states = ["hover", "active", "pressed", "focus", "disabled", "soft", "subtle", "muted", "light", "dark", "ring"];
  const ink = ["foreground", "fg", "text", "ink", "on", "contrast", "content"];
  const fills = [...ink, ...states, "border"];
  const bg = find("bg", ["background", "bg", "canvas", "page", "paper", "base", "body"], fills) ?? (dark ? "#121316" : "#ffffff");
  const text =
    find("text", ["foreground", "fg", "text", "ink", "body", "content"], ["muted", "secondary", "subtle", "primary", "accent", "on", "inverse", "placeholder", "disabled", "link"]) ??
    (dark ? "#ececee" : "#17181c");
  const accent = find("accent", ["primary", "accent", "brand", "action", "interactive"], [...fills, "bg", "background"]) ?? text;
  const onAccent =
    colors.filter((t) => /(primary|accent|brand)[-_.]?(foreground|fg|contrast|text|ink)$|on[-_.]?(primary|accent|brand)$/i.test(t.name)).map(color)[0] ??
    (luminance(accent) > 0.45 ? "#111111" : "#ffffff");
  const radii = tokens.filter((t) => t.family === "radius" && t.px !== null && t.px > 0 && t.px < 999).sort((a, b) => a.px! - b.px!);
  const radius = radii.find((t) => /(^|-)(md|base|default)$|^(--|\$)radius$/i.test(t.name)) ?? radii[Math.floor((radii.length - 1) / 2)];
  const large = radii.at(-1);
  const shadows = tokens.filter((t) => t.family === "shadow" && !t.alias);
  const shadow = shadows.find((t) => /sm|md|card|1|2|base/.test(t.name)) ?? shadows[0];
  if (radius) own.push("radius");
  if (shadow) own.push("shadow");
  return {
    bg,
    surface: find("surface", ["card", "surface", "panel", "raised", "popover", "elevated"], fills) ?? bg,
    text,
    // A `--muted` may be a quiet ground rather than a quiet text: only one that reads on the ground counts.
    muted: find("muted", ["muted", "subtle", "secondary", "placeholder"], ["bg", "background", "surface", "border"], (c) => contrast(c, bg) >= 3) ?? `color-mix(in srgb, ${text} 62%, ${bg})`,
    soft: find("soft", ["secondary", "muted", "subtle"], [...ink, "border"], (c) => contrast(c, bg) < 1.6) ?? `color-mix(in srgb, ${text} 8%, ${bg})`,
    onSoft: colors.filter((t) => /secondary[-_.]?(foreground|fg|text|ink)$/i.test(t.name)).map(color)[0] ?? text,
    border: find("border", ["border", "line", "stroke", "divider", "outline", "input"], states) ?? `color-mix(in srgb, ${text} 16%, ${bg})`,
    accent,
    onAccent,
    danger: find("danger", ["danger", "destructive", "error", "critical", "red"], fills) ?? "#c42b1c",
    success: find("success", ["success", "positive", "green"], fills) ?? "#1a6e3a",
    warning: find("warning", ["warning", "caution", "amber", "yellow"], fills) ?? "#9a6200",
    info: find("info", ["info", "blue"], fills) ?? accent,
    radius: radius ? `${radius.px}px` : "6px",
    radiusLarge: large && large !== radius ? `${Math.min(large.px!, 24)}px` : radius ? `${radius.px! * 1.5}px` : "10px",
    shadow: shadow?.value ?? "0 1px 2px rgb(0 0 0 / 0.08)",
    own,
  };
}

/** WCAG contrast ratio of two `#rrggbb` colors. */
export function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/** Relative luminance of a `#rrggbb` color, 0 (black) to 1 (white); anything else counts as mid-gray. */
export function luminance(hex: string): number {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})/i.exec(hex);
  if (!m) return 0.5;
  const [r, g, b] = m.slice(1).map((h) => {
    const c = parseInt(h, 16) / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
