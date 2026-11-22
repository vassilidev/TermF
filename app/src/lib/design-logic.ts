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
  guidelines: string | null;
  pdf: boolean;
};
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
};

export type Section = "issues" | "color" | "type" | "spacing" | "radius" | "shadow" | "motion" | "other" | "components";
/** The token sections, in the order of the navigation. */
export const FAMILY_SECTIONS: Section[] = ["color", "type", "spacing", "radius", "shadow", "motion", "other"];

/** Font family, size, weight and line height make one section. */
export function sectionOf(family: Family): Section {
  return family === "font" || family === "text" || family === "weight" || family === "leading" ? "type" : family;
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
