// No imports, so `pnpm test` runs it in plain Node.

/** A piece of a line, shown as text: nothing in it is ever read as HTML. */
export type Span =
  | { kind: "text" | "code"; text: string }
  | { kind: "strong"; spans: Span[] }
  | { kind: "link"; text: string; url: string };
/** `start` is null for a bulleted list. */
export type Block =
  | { kind: "paragraph"; spans: Span[] }
  | { kind: "list"; start: number | null; items: Span[][] }
  | { kind: "code"; text: string }
  | { kind: "heading"; level: number; spans: Span[] }
  | { kind: "table"; align: Align[]; head: Span[][]; rows: Span[][][] };
export type Align = "left" | "center" | "right" | null;

// `code`, **bold** (no `*` inside), [label](http…), a bare http(s) address without the punctuation that ends a
// sentence. An address may hold one level of balanced parentheses, as Wikipedia's do.
const inline =
  /`([^`\n]+)`|\*\*([^*\s](?:[^*\n]*[^*\s])?)\*\*|\[([^\]\n]+)\]\((https?:\/\/(?:[^\s()]|\([^\s()]*\))+)\)|https?:\/\/(?:[^\s<>"'`()]|\([^\s<>"'`()]*\))*(?:[^\s<>"'`().,;:!?\]»]|\([^\s<>"'`()]*\))/g;
const item = /^\s*(?:[-*+]|(\d{1,9})[.)])\s+(.*)$/;
const fence = /^\s*```/;
const heading = /^\s{0,3}(#{1,6})\s+(.+?)(?:\s+#+)?\s*$/;
const delimiter = /^\s*\|?\s*:?-+:?\s*(?:\|\s*:?-+:?\s*)*\|?\s*$/;

/** A table row's cells; `\|` stays a bar inside a cell. */
function cells(line: string): string[] {
  const s = line.trim();
  const out = [""];
  for (let i = 0; i < s.length; i++) {
    if (s[i] === "\\" && s[i + 1] === "|") out[out.length - 1] += s[++i];
    else if (s[i] === "|") out.push("");
    else out[out.length - 1] += s[i];
  }
  if (s.startsWith("|")) out.shift();
  if (s.endsWith("|") && !s.endsWith("\\|") && out.length > 1) out.pop();
  return out.map((c) => c.trim());
}

function align(cell: string): Align {
  const left = cell.startsWith(":");
  const right = cell.endsWith(":");
  return left && right ? "center" : right ? "right" : left ? "left" : null;
}

export function spans(text: string): Span[] {
  const out: Span[] = [];
  let at = 0;
  for (const m of text.matchAll(inline)) {
    if (m.index > at) out.push({ kind: "text", text: text.slice(at, m.index) });
    if (m[1] !== undefined) out.push({ kind: "code", text: m[1] });
    else if (m[2] !== undefined) out.push({ kind: "strong", spans: spans(m[2]) });
    else if (m[3] !== undefined) out.push({ kind: "link", text: m[3], url: m[4] });
    else out.push({ kind: "link", text: m[0], url: m[0] });
    at = m.index + m[0].length;
  }
  if (at < text.length) out.push({ kind: "text", text: text.slice(at) });
  return out;
}

/**
 * Light Markdown: paragraphs (their line breaks kept), lists, fenced code, and with `rich` headings and tables;
 * anything else stays as it was written.
 */
export function blocks(text: string, rich = false): Block[] {
  const out: Block[] = [];
  let para: string[] | null = null;
  let list: { start: number | null; items: string[] } | null = null;
  const close = () => {
    if (para) out.push({ kind: "paragraph", spans: spans(para.join("\n")) });
    if (list) out.push({ kind: "list", start: list.start, items: list.items.map(spans) });
    para = list = null;
  };
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (fence.test(line)) {
      close();
      const code: string[] = [];
      while (++i < lines.length && !fence.test(lines[i])) code.push(lines[i]);
      out.push({ kind: "code", text: code.join("\n") });
      continue;
    }
    if (!line.trim()) {
      close();
      continue;
    }
    const h = rich ? heading.exec(line) : null;
    if (h) {
      close();
      out.push({ kind: "heading", level: h[1].length, spans: spans(h[2]) });
      continue;
    }
    const head = rich && line.includes("|") && delimiter.test(lines[i + 1] ?? "") ? cells(line) : null;
    if (head && cells(lines[i + 1]).length === head.length) {
      close();
      const aligns = cells(lines[i + 1]).map(align);
      const rows: Span[][][] = [];
      while (i + 2 < lines.length && lines[i + 2].includes("|") && lines[i + 2].trim()) {
        const row = cells(lines[i + 2]);
        rows.push(head.map((_, j) => spans(row[j] ?? "")));
        i++;
      }
      i++;
      out.push({ kind: "table", align: aligns, head: head.map(spans), rows });
      continue;
    }
    const m = item.exec(line);
    if (m) {
      const start = m[1] === undefined ? null : Number(m[1]);
      if (!list || (list.start === null) !== (start === null)) {
        close();
        list = { start, items: [] };
      }
      list.items.push(m[2]);
    } else if (list && /^\s/.test(line)) {
      list.items[list.items.length - 1] += `\n${line.trim()}`;
    } else {
      if (list) close();
      (para ??= []).push(line);
    }
  }
  close();
  return out;
}
