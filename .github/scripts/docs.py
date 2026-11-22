#!/usr/bin/env python3
# Builds the website's generated parts: the docs (site/docs/*.html and site/docs/search.json) from the Markdown of
# site/docs-src/<section>/<page>.md, the team on the front page (between <!-- gen:name --> markers) from library/roles/
# and site/roles.toml, a small redirect page for every retired address in site/redirects.txt, and site/sitemap.xml.
# Usage: python3 .github/scripts/docs.py (Python 3.11+, standard library only). The Pages workflow runs it.
import glob
import html
import json
import os
import re
import tomllib

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
SITE = os.path.join(ROOT, "site")
SRC = os.path.join(SITE, "docs-src")
OUT = os.path.join(SITE, "docs")
BASE = "https://vassilidev.github.io/TermF/"
REPO = "https://github.com/vassilidev/TermF"
SECTIONS = [("getting-started", "Getting started"), ("concepts", "Concepts"), ("features", "Features"), ("reference", "Reference")]
SHOTS = {os.path.basename(p)[:-len("-light.webp")] for p in glob.glob(os.path.join(SITE, "shots", "*-light.webp"))}

crew = tomllib.load(open(os.path.join(SITE, "roles.toml"), "rb"))


# ------------------------------------------------------------------------------------------------------------- roles

def frontmatter(path):
    """The top-level `key: value` lines of a role's frontmatter; [a, b] lists and comma lists become Python lists."""
    head = re.match(r"---\n(.*?)\n---\n", open(path, encoding="utf-8").read().replace("\r\n", "\n"), re.S)
    assert head, f"{path}: no frontmatter"
    meta = {}
    for line in head.group(1).splitlines():
        key, sep, value = line.partition(":")
        if sep and not line.startswith((" ", "\t", "-")):
            meta[key.strip()] = value.strip()
    for key in ("disallowedTools", "allowedTools", "writes"):
        if key in meta:
            meta[key] = [x.strip().strip("\"'") for x in meta[key].strip("[]").split(",") if x.strip()]
    return meta


def role_card(path):
    """A role as the site shows it: its readable texts from roles.toml, the rest from its file. A role roles.toml does
    not know yet still shows, in the last group, with its own description."""
    meta = frontmatter(path)
    name = meta["name"]
    known = crew["roles"].get(name)
    if known is None:
        print(f"warning: role {name} is not in site/roles.toml; shown with its description")
        desc = meta.get("description", "").strip()
        label, sep, rest = desc.partition(". ")
        if not sep:
            label, rest = name.capitalize(), desc
        never = re.search(r"\bNot for (.*)$", rest)
        does = re.sub(r"\s*\bNot for .*$", "", rest).strip().rstrip(".") or desc.rstrip(".")
        known = {"label": label, "group": crew["groups"][-1]["id"], "short": label, "does": does + "." if does else "",
                 "never": never.group(1)[0].upper() + never.group(1)[1:].rstrip(".") + "." if never else ""}
    denied = meta.get("disallowedTools", [])
    if "writes" in meta:
        access = known.get("access", "Writes only its own paths") if meta["writes"] else "Reads only"
    else:
        access = "Reads only" if {"Edit", "Write"} <= set(denied) else "Edits files"
    limits = [label for tool, label in (("Bash", "No shell"), ("WebFetch", "No web")) if tool in denied]
    if any(t.startswith("mcp__termf__browser") for t in meta.get("allowedTools", [])):
        limits.append("Uses a browser")
    words = re.split(r"[-_\s]+", name)
    mark = known.get("mark") or (words[0][0] + words[1][0] if len(words) > 1 else name[:2]).upper()
    return {"name": name, "mark": mark, "model": meta.get("model", ""), "effort": meta.get("effort", ""),
            "access": access, "limits": limits, **known}


roles = [role_card(p) for p in sorted(glob.glob(os.path.join(ROOT, "library", "roles", "*.md")))]
assert roles, "no role in library/roles/: the team needs at least one"
order = {name: i for i, name in enumerate(crew["roles"])}
roles.sort(key=lambda r: (order.get(r["name"], len(order)), r["name"]))
color = {g["id"]: g["color"] for g in crew["groups"]}
assert all(r["group"] in color for r in roles), "a role in an unknown group"
names = [r["name"] for r in roles]
COUNTS = {"roles": str(len(roles)), "role_names": " and ".join(filter(None, [", ".join(names[:-1]), names[-1]]))}


def role_cards():
    """Every role of the docs' team page, by group, each card reachable at #role-<name>."""
    out = []
    for g in crew["groups"]:
        members = [r for r in roles if r["group"] == g["id"]]
        if not members:
            continue
        cards = []
        for r in members:
            chips = [f"{r['model']} · {r['effort']}" if r["effort"] else r["model"], r["access"], *r["limits"]]
            rows = "".join(f"<div><dt>{k}</dt><dd>{v}</dd></div>" for k, v in (("Called", r.get("when")), ("Never", r.get("never"))) if v)
            cards.append(f'<article class="role c-{g["color"]}" id="role-{r["name"]}"><header><span class="mark" aria-hidden="true">{r["mark"]}</span>'
                         f'<div><h4>{r["label"]}</h4><code>{r["name"]}</code></div></header><p>{r["does"]}</p><dl>{rows}</dl>'
                         f'<ul class="chips" aria-label="Settings">{"".join(f"<li>{c}</li>" for c in chips if c)}</ul></article>')
        out.append(f'<section class="crew" aria-labelledby="crew-{g["id"]}"><h3 id="crew-{g["id"]}">{g["name"]}</h3>'
                   f'<p>{g["intro"]}</p><div class="roles">{"".join(cards)}</div></section>')
    return "\n".join(out)


def roster():
    """The front page's team board: every role as a token linking to its card in the docs. The CTO leads; a script on
    the page moves an example lineup onto the field."""
    def token(r):
        return (f'<a class="tok c-{color[r["group"]]}" href="docs/team.html#role-{r["name"]}" data-role="{r["name"]}">'
                f'<i aria-hidden="true">{r["mark"]}</i><span>{r["label"]}</span></a>')
    return "".join(token(r) for r in roles if r["name"] != "cto")


# ---------------------------------------------------------------------------------------------------------- markdown

def esc(s):
    return html.escape(s, quote=True)


def slugify(s):
    s = re.sub(r"<[^>]+>", "", s).lower()
    return re.sub(r"[^a-z0-9]+", "-", s).strip("-") or "section"


def text_of(s):
    return re.sub(r"\s+", " ", html.unescape(re.sub(r"<[^>]+>", " ", s))).strip()


class Page:
    def __init__(self, section, path):
        self.section = section
        self.slug = os.path.basename(path)[:-3]
        self.src = os.path.relpath(path, ROOT)
        raw = open(path, encoding="utf-8").read().replace("\r\n", "\n")
        head = re.match(r"---\n(.*?)\n---\n", raw, re.S)
        assert head, f"{self.src}: no front matter"
        meta = dict(line.split(":", 1) for line in head.group(1).splitlines() if ":" in line)
        self.title = meta.get("title", "").strip()
        assert self.title, f"{self.src}: no title"
        self.order = int(meta.get("order", "99"))
        self.md = raw[head.end():]
        self.ids, self.toc, self.links, self.chunks = set(), [], [], []


def inline(s, page):
    def span(part):
        part = esc(part).replace("&#x27;", "'")
        part = re.sub(r"&lt;(/?)(kbd|br)\s*(/?)&gt;", r"<\1\2\3>", part)

        def link(m):
            label, url = m.group(1), html.unescape(m.group(2))
            doc = re.fullmatch(r"([a-z0-9-]+)\.md(#[\w-]+)?", url)
            if doc:
                page.links.append((doc.group(1), (doc.group(2) or "")[1:]))
                url = doc.group(1) + ".html" + (doc.group(2) or "")
            return f'<a href="{esc(url)}">{label}</a>'
        part = re.sub(r"\[([^\]]+)\]\(([^)\s]+)\)", link, part)
        part = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", part)
        return re.sub(r"(?<![\w*])\*(?!\s)(.+?)(?<!\s)\*(?![\w*])", r"<em>\1</em>", part)
    s = re.sub(r"\{(roles|role_names)\}", lambda m: COUNTS[m.group(1)], s)
    parts = re.split(r"`([^`]+)`", s)
    return "".join(f"<code>{esc(p)}</code>" if i % 2 else span(p) for i, p in enumerate(parts))


def heading(level, raw, page):
    m = re.match(r"(.*?)\s*\{#([\w-]+)\}$", raw)
    label, hid = (m.group(1), m.group(2)) if m else (raw, None)
    hid = hid or slugify(label)
    base, n = hid, 2
    while hid in page.ids:
        hid, n = f"{base}-{n}", n + 1
    page.ids.add(hid)
    body = inline(label, page)
    page.toc.append((level, hid, body))
    if level == 2:
        page.chunks.append([text_of(body), hid, []])
    return f'<h{level} id="{hid}">{body}<a class="hash" href="#{hid}" aria-label="Link to this section"></a></h{level}>'


def code_block(lines, lang):
    body = []
    for line in lines:
        e = esc(line).replace("&#x27;", "'")
        body.append(f'<span class="c">{e}</span>' if lang in ("sh", "toml", "text", "ps1", "powershell") and line.lstrip().startswith("#") else e)
    return f'<pre class="code" data-lang="{esc(lang)}"><code>{chr(10).join(body)}</code></pre>'


def table(rows, page):
    def cells(row):
        return [c.strip() for c in row.strip().strip("|").split("|")]
    head, body = cells(rows[0]), [cells(r) for r in rows[2:]]
    aligns = ["right" if c.strip().endswith(":") and not c.strip().startswith(":") else "" for c in cells(rows[1])]
    th = "".join(f'<th scope="col"{" class=r" if a else ""}>{inline(c, page)}</th>' for c, a in zip(head, aligns))
    trs = "".join("<tr>" + "".join(f'<td{" class=r" if a else ""}>{inline(c, page)}</td>' for c, a in zip(r, aligns)) + "</tr>" for r in body)
    return f'<div class="table"><table><thead><tr>{th}</tr></thead><tbody>{trs}</tbody></table></div>'


def figure(name, alt, caption):
    assert name in SHOTS, f"unknown shot {name}: site/shots/{name}-light.webp does not exist"
    def src(theme):
        return f"../shots/{name}-{theme}-800.webp 800w, ../shots/{name}-{theme}.webp 1600w"
    sizes = "(max-width: 900px) 100vw, 760px"
    pic = (f'<picture><source media="(prefers-color-scheme: dark)" srcset="{src("dark")}" sizes="{sizes}">'
           f'<img src="../shots/{name}-light.webp" srcset="{src("light")}" sizes="{sizes}" width="1600" height="1000" loading="lazy" decoding="async" alt="{esc(alt)}"></picture>')
    cap = f"<figcaption>{esc(caption)}</figcaption>" if caption else ""
    return f'<figure class="shot">{pic}{cap}</figure>'


def directive(name):
    if name == "roles":
        return role_cards()
    if name == "changelog":
        log = open(os.path.join(ROOT, "CHANGELOG.md"), encoding="utf-8").read()
        return re.sub(r"\A# .*\n", "", log)
    raise AssertionError(f"unknown directive {{{{{name}}}}}")


def render(md, page):
    md = re.sub(r"^\{\{(\w+)\}\}$", lambda m: m.group(0) if m.group(1) == "roles" else directive(m.group(1)), md, flags=re.M)
    lines, out, para, i = md.split("\n"), [], [], 0

    def emit(block):
        out.append(block)
        if page.chunks:
            page.chunks[-1][2].append(text_of(block))

    def flush():
        if para:
            emit("<p>" + inline(" ".join(para), page) + "</p>")
            para.clear()

    while i < len(lines):
        s = lines[i].strip()
        if not s:
            flush()
            i += 1
            continue
        if s.startswith("```"):
            flush()
            j, buf = i + 1, []
            while j < len(lines) and not lines[j].strip().startswith("```"):
                buf.append(lines[j])
                j += 1
            emit(code_block(buf, s[3:].strip()))
            i = j + 1
            continue
        m = re.match(r"(#{2,4})\s+(.+)$", s)
        if m:
            flush()
            out.append(heading(len(m.group(1)), m.group(2), page))
            i += 1
            continue
        if s == "{{roles}}":
            flush()
            out.append(directive("roles"))
            page.ids.update(f"role-{r['name']}" for r in roles)
            page.ids.update(f"crew-{g['id']}" for g in crew["groups"])
            i += 1
            continue
        m = re.fullmatch(r'!\[([^\]]*)\]\(shot:([\w-]+)(?:\s+"([^"]*)")?\)', s)
        if m:
            flush()
            out.append(figure(m.group(2), m.group(1), m.group(3) or ""))
            i += 1
            continue
        if s.startswith("|") and i + 1 < len(lines) and re.fullmatch(r"\|?[\s:|-]*-[\s:|-]*\|?", lines[i + 1].strip()):
            flush()
            rows = []
            while i < len(lines) and lines[i].strip().startswith("|"):
                rows.append(lines[i])
                i += 1
            emit(table(rows, page))
            continue
        if s.startswith(">"):
            flush()
            quote = []
            while i < len(lines) and lines[i].strip().startswith(">"):
                quote.append(re.sub(r"^\s*>\s?", "", lines[i]))
                i += 1
            kind = re.fullmatch(r"\[!(NOTE|WARNING)\]", quote[0].strip())
            inner = render_nested(quote[1:] if kind else quote, page)
            if kind:
                label = {"NOTE": "Note", "WARNING": "Warning"}[kind.group(1)]
                emit(f'<aside class="callout {kind.group(1).lower()}"><p class="callout-t">{label}</p>{inner}</aside>')
            else:
                emit(f"<blockquote>{inner}</blockquote>")
            continue
        m = re.match(r"([-*]|\d+\.)\s+", s)
        if m:
            flush()
            ordered = m.group(1)[0].isdigit()
            pattern = r"\d+\.\s+" if ordered else r"[-*]\s+"
            items = []
            while i < len(lines):
                line = lines[i]
                if re.match(pattern, line.strip()) and not line.startswith("  "):
                    items.append(re.sub(r"^" + pattern, "", line.strip()))
                elif line.strip() and line.startswith("  ") and items:
                    items[-1] += " " + line.strip()
                elif not line.strip() and i + 1 < len(lines) and re.match(pattern, lines[i + 1].strip()) and not lines[i + 1].startswith("  "):
                    pass
                else:
                    break
                i += 1
            tag = "ol" if ordered else "ul"
            emit(f"<{tag}>" + "".join(f"<li>{inline(x, page)}</li>" for x in items) + f"</{tag}>")
            continue
        para.append(s)
        i += 1
    flush()
    return "\n".join(out)


def render_nested(lines, page):
    """The inside of a callout or a quote: paragraphs and lists, without touching the page's outline."""
    toc, chunks = page.toc, page.chunks
    page.chunks = []
    body = render("\n".join(lines), page)
    page.toc, page.chunks = toc, chunks
    return body


# ------------------------------------------------------------------------------------------------------------ layout

ICONS = {
    "search": '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>',
    "menu": '<path d="M4 6h16M4 12h16M4 18h16"/>',
    "sun": '<circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M4.93 4.93l1.41 1.41m11.32 11.32 1.41 1.41M2 12h2m16 0h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"/>',
    "moon": '<path d="M20.99 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.79 9.8Z"/>',
    "github": '<path d="M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4"/><path d="M9 18c-4.51 2-5-2-7-2"/>',
    "arrow": '<path d="M5 12h14m-6-6 6 6-6 6"/>',
}


def icon(name):
    return f'<svg class="i" viewBox="0 0 24 24" aria-hidden="true">{ICONS[name]}</svg>'


THEME_SCRIPT = """<script>try{const t=localStorage.getItem('theme');if(t==='light'||t==='dark')document.documentElement.dataset.theme=t}catch{}</script>"""


def head(title, desc, url, prefix):
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{esc(title)}</title>
<meta name="description" content="{esc(desc)}">
<link rel="canonical" href="{url}">
<meta property="og:type" content="article">
<meta property="og:site_name" content="TermF">
<meta property="og:title" content="{esc(title)}">
<meta property="og:description" content="{esc(desc)}">
<meta property="og:url" content="{url}">
<meta property="og:image" content="{BASE}og.png">
<meta name="twitter:card" content="summary_large_image">
<meta name="theme-color" content="#f6f7f5" media="(prefers-color-scheme: light)">
<meta name="theme-color" content="#0b0f0d" media="(prefers-color-scheme: dark)">
<meta name="color-scheme" content="light dark">
<link rel="icon" type="image/svg+xml" href="{prefix}icon.svg">
<link rel="icon" type="image/png" sizes="32x32" href="{prefix}favicon.png">
<link rel="apple-touch-icon" sizes="180x180" href="{prefix}apple-touch-icon.png">
<link rel="preload" href="{prefix}fonts/inter.woff2" as="font" type="font/woff2" crossorigin>
<link rel="stylesheet" href="{prefix}site.css">
{THEME_SCRIPT}
</head>"""


def topbar(prefix):
    return f"""<header class="top">
  <button class="side-btn" type="button" aria-controls="side" aria-expanded="false">{icon("menu")}<span class="vh">Documentation menu</span></button>
  <a class="brand" href="{prefix}"><img src="{prefix}icon.svg" alt="" width="26" height="26"><span>TermF</span></a>
  <a class="top-docs" href="{prefix}docs/">Docs</a>
  <button class="find" type="button" aria-haspopup="dialog" aria-label="Search the docs">{icon("search")}<span>Search the docs</span><kbd>⌘K</kbd></button>
  <nav class="top-links" aria-label="Site">
    <a href="{REPO}">{icon("github")}<span class="vh">GitHub</span></a>
    <button class="theme" type="button" aria-label="Switch to dark theme">{icon("moon")}{icon("sun")}</button>
    <a class="btn small" href="{prefix}download.html">Download</a>
  </nav>
</header>"""


# The docs' behaviour: drawer, theme, copy buttons, the outline's current section, and the search over search.json
DOCS_SCRIPT = r"""
const root = document.documentElement, side = document.getElementById('side'), sideBtn = document.querySelector('.side-btn');
const setSide = open => { document.body.classList.toggle('side-open', open); sideBtn.setAttribute('aria-expanded', open); if (open) side.querySelector('a[aria-current]')?.focus(); };
sideBtn.addEventListener('click', () => setSide(!document.body.classList.contains('side-open')));
document.querySelector('.scrim').addEventListener('click', () => setSide(false));
const themeBtn = document.querySelector('.theme');
const dark = () => root.dataset.theme ? root.dataset.theme === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches;
const syncTheme = () => {
  themeBtn.setAttribute('aria-label', dark() ? 'Switch to light theme' : 'Switch to dark theme');
  for (const s of document.querySelectorAll('picture source')) s.media = root.dataset.theme ? (dark() ? 'all' : 'not all') : '(prefers-color-scheme: dark)';
};
themeBtn.addEventListener('click', () => { root.dataset.theme = dark() ? 'light' : 'dark'; try { localStorage.setItem('theme', root.dataset.theme); } catch {} syncTheme(); });
syncTheme();
for (const pre of document.querySelectorAll('pre.code')) {
  const b = Object.assign(document.createElement('button'), {type: 'button', className: 'copy', textContent: 'Copy'});
  b.addEventListener('click', async () => {
    const text = [...pre.querySelector('code').childNodes].filter(n => !(n.classList?.contains('c'))).map(n => n.textContent).join('').replace(/^\n+|\n+$/g, '').replace(/\n{2,}/g, '\n');
    try { await navigator.clipboard.writeText(text); } catch { return; }
    b.textContent = 'Copied'; b.classList.add('ok');
    setTimeout(() => { b.textContent = 'Copy'; b.classList.remove('ok'); }, 1600);
  });
  pre.append(b);
}
const toc = [...document.querySelectorAll('.toc a')];
if (toc.length) {
  const heads = toc.map(a => document.getElementById(a.hash.slice(1)));
  const spy = () => {
    let i = heads.findLastIndex(h => h.getBoundingClientRect().top < 120);
    toc.forEach((a, j) => a.classList.toggle('on', j === Math.max(i, 0)));
  };
  addEventListener('scroll', spy, {passive: true}); spy();
}
const dlg = document.getElementById('search'), q = dlg.querySelector('input'), list = dlg.querySelector('ul'), none = dlg.querySelector('.none');
let index, active = 0;
const open = () => { if (!dlg.open) { dlg.showModal(); q.select(); if (!index) fetch(dlg.dataset.index).then(r => r.json()).then(d => { index = d; find(); }); } };
document.querySelector('.find').addEventListener('click', open);
addEventListener('keydown', e => {
  const typing = /INPUT|TEXTAREA|SELECT/.test(document.activeElement.tagName) || document.activeElement.isContentEditable;
  if ((e.key === 'k' && (e.metaKey || e.ctrlKey)) || (e.key === '/' && !typing)) { e.preventDefault(); open(); }
  if (e.key === 'Escape' && document.body.classList.contains('side-open')) { setSide(false); sideBtn.focus(); }
});
dlg.addEventListener('click', e => { if (e.target === dlg) dlg.close(); });
const mark = (s, words) => { let out = s.replace(/[&<>"]/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;'}[c])); for (const w of words) out = out.replace(new RegExp('(' + w.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + ')', 'ig'), '<mark>$1</mark>'); return out; };
function find() {
  const words = q.value.toLowerCase().split(/\s+/).filter(Boolean);
  list.textContent = '';
  if (!index || !words.length) { none.hidden = true; return; }
  const hits = index.map(e => {
    const t = (e.t + ' ' + e.h).toLowerCase(), x = e.x.toLowerCase();
    if (!words.every(w => t.includes(w) || x.includes(w))) return null;
    return [words.reduce((s, w) => s + (e.h.toLowerCase().includes(w) ? 4 : 0) + (e.t.toLowerCase().includes(w) ? 3 : 0) + (x.includes(w) ? 1 : 0), 0) - (e.h ? 0 : .5), e];
  }).filter(Boolean).sort((a, b) => b[0] - a[0]).slice(0, 12);
  none.hidden = hits.length > 0;
  for (const [, e] of hits) {
    const hit = e.x.toLowerCase().indexOf(words[0]), at = hit > 40 ? e.x.lastIndexOf(' ', hit - 30) + 1 : 0, snip = (at ? '…' : '') + e.x.slice(at, at + 140) + '…';
    const li = document.createElement('li');
    li.innerHTML = `<a href="${e.u}"><span class="r-t">${mark(e.h || e.t, words)}</span><span class="r-p">${e.s} · ${mark(e.t, [])}</span><span class="r-x">${mark(snip, words)}</span></a>`;
    list.append(li);
  }
  active = 0; select();
}
const select = () => [...list.children].forEach((li, i) => li.classList.toggle('on', i === active));
q.addEventListener('input', find);
q.addEventListener('keydown', e => {
  const n = list.children.length;
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); if (n) { active = (active + (e.key === 'ArrowDown' ? 1 : n - 1)) % n; select(); list.children[active].scrollIntoView({block: 'nearest'}); } }
  if (e.key === 'Enter' && n) { e.preventDefault(); location.href = list.children[active].querySelector('a').href; dlg.close(); }
});
"""


def sidebar(pages, current, prefix):
    out = []
    for sid, name in SECTIONS:
        items = "".join(f'<li><a href="{prefix}docs/{p.slug}.html"{" aria-current=page" if p is current else ""}>{p.title}</a></li>'
                        for p in pages if p.section == sid)
        is_open = " open" if current is None or current.section == sid else ""
        out.append(f'<details{is_open}><summary>{name}</summary><ul>{items}</ul></details>')
    return f'<nav class="side" id="side" aria-label="Documentation">{"".join(out)}</nav><div class="scrim" aria-hidden="true"></div>'


def search_dialog():
    return f"""<dialog id="search" aria-label="Search the docs" data-index="search.json">
  <form method="dialog" role="search"><label class="vh" for="q">Search the docs</label>{icon("search")}<input id="q" type="search" placeholder="Search the docs" autocomplete="off" spellcheck="false"><kbd>Esc</kbd></form>
  <ul role="list"></ul>
  <p class="none" hidden>Nothing matches. Try another word.</p>
</dialog>"""


def footer(prefix):
    return f"""<footer class="foot">
  <div class="wrap foot-in">
    <a class="brand" href="{prefix}"><img src="{prefix}icon.svg" alt="" width="24" height="24"><span>TermF</span></a>
    <nav aria-label="Footer"><a href="{prefix}docs/">Docs</a><a href="{prefix}download.html">Download</a><a href="{REPO}">GitHub</a><a href="{REPO}/releases">Releases</a><a href="{REPO}/issues">Report an issue</a><a href="{REPO}/blob/main/LICENSE">MIT license</a></nav>
    <p>Free and open source. Runs on your machine.</p>
  </div>
</footer>"""


def doc_page(page, pages, body):
    prefix = "../"
    i = pages.index(page)
    prev, nxt = (pages[i - 1] if i else None), (pages[i + 1] if i + 1 < len(pages) else None)
    pager = "".join(f'<a class="{cls}" href="{p.slug}.html"><span>{label}</span><b>{p.title}</b></a>'
                    for p, cls, label in ((prev, "prev", "Previous"), (nxt, "next", "Next")) if p)
    toc = "".join(f'<li class="l{lvl}"><a href="#{hid}">{re.sub(r"<[^>]+>", "", label)}</a></li>' for lvl, hid, label in page.toc if lvl <= 3)
    toc_html = f'<aside class="toc" aria-labelledby="toc-h"><p id="toc-h">On this page</p><ul>{toc}</ul></aside>' if toc else '<aside class="toc"></aside>'
    first = re.search(r"<p>(.*?)</p>", body, re.S)
    desc = text_of(first.group(1)) if first else page.title
    desc = desc if len(desc) <= 158 else desc[:155].rsplit(" ", 1)[0] + "…"
    section = dict(SECTIONS)[page.section]
    return f"""{head(f"{page.title} · TermF docs", desc, BASE + f"docs/{page.slug}.html", prefix)}
<body class="docs-body">
<a class="skip" href="#content">Skip to content</a>
{topbar(prefix)}
<div class="docs">
{sidebar(pages, page, prefix)}
<main id="content" class="doc">
<article class="prose">
<p class="crumbs"><a href="./">Docs</a><span aria-hidden="true">/</span>{section}</p>
<h1>{page.title}</h1>
{body}
</article>
<nav class="pager" aria-label="Previous and next page">{pager}</nav>
<p class="edit"><a href="{REPO}/edit/main/{page.src}">Edit this page on GitHub</a></p>
</main>
{toc_html}
</div>
{search_dialog()}
<script>{DOCS_SCRIPT}</script>
</body>
</html>
"""


def docs_index(pages):
    cols = []
    for sid, name in SECTIONS:
        items = "".join(f'<li><a href="{p.slug}.html">{p.title}</a></li>' for p in pages if p.section == sid)
        cols.append(f'<section><h2 id="{sid}">{name}</h2><ul>{items}</ul></section>')
    prefix = "../"
    return f"""{head("Documentation · TermF", "How to install TermF, create a project, work with the CTO and its team, and use every feature.", BASE + "docs/", prefix)}
<body class="docs-body">
<a class="skip" href="#content">Skip to content</a>
{topbar(prefix)}
<div class="docs">
{sidebar(pages, None, prefix)}
<main id="content" class="doc">
<article class="prose">
<h1>Documentation</h1>
<p class="lead">How to install TermF, create a project, hand a ticket to the CTO and its team, and use every feature. New here? Start with <a href="installation.html">Installation</a>.</p>
<div class="index">{"".join(cols)}</div>
</article>
</main>
<aside class="toc"></aside>
</div>
{search_dialog()}
<script>{DOCS_SCRIPT}</script>
</body>
</html>
"""


def redirect(old, new):
    depth = old.count("/")
    target, _, anchor = new.partition("#")
    rel = "../" * depth + target + (f"#{anchor}" if anchor else "")
    keep = "" if anchor else "+location.hash"
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Moved · TermF</title>
<meta name="robots" content="noindex">
<link rel="canonical" href="{BASE}{new}">
<meta http-equiv="refresh" content="0; url={esc(rel)}">
<script>location.replace({json.dumps(rel)}{keep})</script>
</head>
<body>
<p>This page moved: <a href="{esc(rel)}">{esc(BASE + new)}</a>.</p>
</body>
</html>
"""


# ------------------------------------------------------------------------------------------------------------- build

pages = []
for sid, _ in SECTIONS:
    found = [Page(sid, p) for p in glob.glob(os.path.join(SRC, sid, "*.md"))]
    pages += sorted(found, key=lambda p: (p.order, p.slug))
assert pages, "no page in site/docs-src/"
slugs = [p.slug for p in pages]
assert len(set(slugs)) == len(slugs), "two doc pages with the same file name"
by_slug = {p.slug: p for p in pages}

bodies = {p.slug: render(p.md, p) for p in pages}
errors = [f"{p.src}: link to {s}.md{'#' + a if a else ''} does not exist" for p in pages for s, a in p.links
          if s not in by_slug or (a and a not in by_slug[s].ids)]
assert not errors, "\n".join(errors)

os.makedirs(OUT, exist_ok=True)
for old in glob.glob(os.path.join(OUT, "*.html")):
    os.remove(old)
for p in pages:
    open(os.path.join(OUT, f"{p.slug}.html"), "w", encoding="utf-8").write(doc_page(p, pages, bodies[p.slug]))
open(os.path.join(OUT, "index.html"), "w", encoding="utf-8").write(docs_index(pages))

index = []
for p in pages:
    intro = text_of(bodies[p.slug].split("<h2", 1)[0])
    index.append({"t": p.title, "h": "", "u": f"{p.slug}.html", "s": dict(SECTIONS)[p.section], "x": intro[:400]})
    index += [{"t": p.title, "h": h, "u": f"{p.slug}.html#{hid}", "s": dict(SECTIONS)[p.section], "x": " ".join(parts)[:400]}
              for h, hid, parts in p.chunks]
open(os.path.join(OUT, "search.json"), "w", encoding="utf-8").write(json.dumps(index, ensure_ascii=False, separators=(",", ":")))

# The front page's team, between its markers
fills = {"count": COUNTS["roles"], "roster": roster()}
path = os.path.join(SITE, "index.html")
source = open(path, encoding="utf-8").read()
filled = source
for key, value in fills.items():
    filled, n = re.subn(rf"(<!-- gen:{key} -->).*?(<!-- /gen:{key} -->)", lambda m: m.group(1) + value + m.group(2), filled, flags=re.S)
    assert n, f"index.html: no <!-- gen:{key} --> … <!-- /gen:{key} --> marker"
# The launch video plays over the screenshot only once site/media/ has it, so the page never asks for a missing file
video = ' data-video' if os.path.exists(os.path.join(SITE, "media", "launch.mp4")) else ""
filled = re.sub(r'id="reel"( data-video)?', f'id="reel"{video}', filled)
if filled != source:
    open(path, "w", encoding="utf-8").write(filled)
    print("index.html: the team refreshed")

urls = ["", "download.html", "docs/"] + [f"docs/{p.slug}.html" for p in pages]
open(os.path.join(SITE, "sitemap.xml"), "w", encoding="utf-8").write(
    '<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
    + "".join(f"  <url>\n    <loc>{BASE}{u}</loc>\n  </url>\n" for u in urls) + "</urlset>\n")
# The retired pages of the old site, each sent to the page of the docs that replaced it
moved = []
for n, line in enumerate(open(os.path.join(SITE, "redirects.txt"), encoding="utf-8"), 1):
    line = line.split("#", 1)[0].strip() if line.lstrip().startswith("#") else line.strip()
    if not line:
        continue
    old, new = line.split()
    target, _, anchor = new.partition("#")
    slug = target[len("docs/"):-len(".html")] if target.endswith(".html") else ""
    assert target == "docs/" or (slug in by_slug and (not anchor or anchor in by_slug[slug].ids)), f"redirects.txt:{n}: {new} does not exist"
    assert not os.path.exists(os.path.join(SITE, old)) or open(os.path.join(SITE, old), encoding="utf-8").read().startswith("<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>Moved"), f"redirects.txt:{n}: {old} is a real page"
    os.makedirs(os.path.dirname(os.path.join(SITE, old)), exist_ok=True)
    open(os.path.join(SITE, old), "w", encoding="utf-8").write(redirect(old, new))
    moved.append(old)

print(f"{len(pages)} doc pages, {len(index)} search entries, {len(moved)} redirects, {len(roles)} roles")
