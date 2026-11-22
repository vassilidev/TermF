#!/usr/bin/env python3
# Builds the website's feature catalogue (site/features.html), one page per feature (site/features/<slug>.html) and
# site/sitemap.xml, all from site/features.toml. The icon sprite, the nav and the footer are taken from
# site/how-it-works.html, so the generated pages share them with the hand-written ones.
# Usage: python3 .github/scripts/features.py (Python 3.11+, standard library only). The Pages workflow runs it.
import json
import os
import re
import tomllib

SITE = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "site"))
BASE = "https://vassilidev.github.io/TermF/"
ARROW = '<i><svg aria-hidden="true"><use href="#i-arrow"/></svg></i>'

data = tomllib.load(open(os.path.join(SITE, "features.toml"), "rb"))
groups, features, visuals = data["groups"], data["features"], data.get("visuals", {})
by_slug = {f["slug"]: f for f in features}
by_group = {g["id"]: g for g in groups}

# The data is checked before anything is written, so a typo fails the build instead of shipping a broken link
assert len(by_slug) == len(features), "duplicate slug"
for f in features:
    assert re.fullmatch(r"[a-z0-9-]+", f["slug"]), f["slug"]
    assert f["group"] in by_group, f"{f['slug']}: unknown group {f['group']}"
    assert all(r in by_slug and r != f["slug"] for r in f.get("related", [])), f"{f['slug']}: bad related"
    assert f.get("visual", "") == "" or f["visual"] in visuals, f"{f['slug']}: unknown visual"
    for key in ("name", "tagline", "problem", "how", "default", "tune", "limits", "tried"):
        assert f.get(key), f"{f['slug']}: no {key}"

shell = open(os.path.join(SITE, "how-it-works.html")).read()
SPRITE = re.search(r'<svg width="0".*?</svg>', shell, re.S).group(0)
NAV = re.search(r'<header class="nav".*?</header>', shell, re.S).group(0).replace(' aria-current="page"', "")
FOOTER = re.search(r"<footer>.*?</footer>", shell, re.S).group(0)


def esc(s):
    return s.replace("&", "&amp;").replace('"', "&quot;").replace("<", "&lt;").replace(">", "&gt;")


def text(s):
    return re.sub(r"\s+", " ", re.sub(r"<[^>]+>", "", s)).replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&").strip()


def rebase(page, prefix):
    def fix(m):
        url = m.group(2)
        if re.match(r"(https?:|mailto:|data:|#|/)", url):
            return m.group(0)
        return f'{m.group(1)}="{prefix}{url[2:] if url.startswith("./") else url}"'
    return re.sub(r'\b(href|src)="([^"]*)"', fix, page)


def page(path, title, desc, body, ld, og_type="website", current="", script=""):
    url = BASE + path
    nav = NAV.replace(f'href="{current}"', f'href="{current}" aria-current="page"') if current else NAV
    html = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{esc(title)}</title>
<meta name="description" content="{esc(desc)}">
<link rel="canonical" href="{url}">
<meta property="og:type" content="{og_type}">
<meta property="og:site_name" content="TermF">
<meta property="og:title" content="{esc(title)}">
<meta property="og:description" content="{esc(desc)}">
<meta property="og:url" content="{url}">
<meta property="og:image" content="{BASE}og.png">
<meta property="og:image:type" content="image/png">
<meta property="og:image:width" content="1200">
<meta property="og:image:height" content="630">
<meta property="og:image:alt" content="TermF: talk only to the CTO, and a team of Claude Code agents takes your ticket to a pull request">
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="{esc(title)}">
<meta name="twitter:description" content="{esc(desc)}">
<meta name="twitter:image" content="{BASE}og.png">
<meta name="twitter:image:alt" content="TermF: talk only to the CTO, and a team of Claude Code agents takes your ticket to a pull request">
<meta name="theme-color" content="#eceef2" media="(prefers-color-scheme: light)">
<meta name="theme-color" content="#0c0e12" media="(prefers-color-scheme: dark)">
<meta name="color-scheme" content="light dark">
<link rel="icon" type="image/svg+xml" href="icon.svg">
<link rel="icon" type="image/png" sizes="32x32" href="favicon.png">
<link rel="apple-touch-icon" sizes="180x180" href="apple-touch-icon.png">
<link rel="sitemap" type="application/xml" title="Sitemap" href="sitemap.xml">
<link rel="preload" href="fonts/archivo-condensed.woff2" as="font" type="font/woff2" crossorigin>
<link rel="preload" href="fonts/inter.woff2" as="font" type="font/woff2" crossorigin>
<link rel="stylesheet" href="site.css">
<script type="application/ld+json">
{json.dumps({"@context": "https://schema.org", "@graph": ld}, indent=2, ensure_ascii=False)}
</script>
</head>
<body>
<a class="skip" href="#main">Skip to content</a>
{SPRITE}

{nav}

<main id="main">
{body}
</main>

{FOOTER}

<script>
const nav = document.getElementById('nav');
addEventListener('scroll', () => nav.classList.toggle('scrolled', scrollY > 40), {{passive: true}});{script}
</script>
</body>
</html>
"""
    depth = path.count("/")
    html = rebase(html, "../" * depth) if depth else html
    out = os.path.join(SITE, path)
    os.makedirs(os.path.dirname(out), exist_ok=True)
    open(out, "w").write(html)
    return path


def crumbs(*items):
    return [{"@type": "ListItem", "position": i + 1, "name": name, "item": BASE + href} for i, (name, href) in enumerate(items)]


def figure(key, caption):
    snippet = visuals[key].strip()
    if snippet.startswith('<div class="app"'):
        inner = f'<div class="screen"><p class="screen-cap">{esc(caption)}</p>{snippet}</div>'
    elif snippet.startswith('<div class="bay"'):
        inner = f'<div class="board">{snippet}</div><figcaption>{esc(caption)}</figcaption>'
    else:
        inner = f"{snippet}<figcaption>{esc(caption)}</figcaption>"
    return f"<figure>{inner}</figure>"


def how_board(f):
    strips = "".join(f'<li class="strip" data-state="working"><span class="cs">{i}</span><span class="what">{step}</span></li>' for i, step in enumerate(f["how"], 1))
    return f'<div class="board how"><div class="bay"><p class="plate">How it works</p><ol class="strips">{strips}</ol></div></div>'


def items(xs):
    return "".join(f"<li>{x}</li>" for x in xs)


def feature_strip(f):
    return (f'<li><a class="strip" data-state="done" href="features/{f["slug"]}.html">'
            f'<span class="cs">{f["name"]}</span><span class="what">{f["tagline"]}</span>'
            f'<span class="st"><svg aria-hidden="true"><use href="#i-arrow"/></svg></span></a></li>')


written = []

for f in features:
    g = by_group[f["group"]]
    visual = f.get("visual")
    right = figure(visual, f.get("caption", "")) if visual else how_board(f)
    how = f'<section aria-labelledby="how"><h2 id="how">How it works</h2>{how_board(f)}</section>' if visual else ""
    cmds = "".join(f"<li><code>{esc(c)}</code><span>{d}</span></li>" for c, d in f.get("commands", []))
    cmds_block = f'<div><h2 id="cmds">Commands</h2><ul class="cmds">{cmds}</ul></div>' if cmds else '<div><h2 id="cmds">Commands</h2><p class="status">Nothing to type: it is all in the app.</p></div>'
    related = "".join(feature_strip(by_slug[r]) for r in f.get("related", []))
    read = ""
    if f.get("read"):
        target = f["read"]
        article = {"team.html": "The CTO and the team", "how-it-works.html": "How a ticket becomes a pull request",
                   "security.html": "Security, and its limits", "download.html": "Download TermF"}[target.split("#")[0]]
        read = f'<a class="more" href="{target}">Read: {article}</a>'
    body = f"""<header class="page-head">
  <div class="wrap">
    <p class="crumbs"><a href="./">TermF</a><span aria-hidden="true">/</span><a href="features.html">Features</a><span aria-hidden="true">/</span><a href="features.html#{g['id']}">{g['name']}</a></p>
    <h1>{f['name']}</h1>
    <p class="lead">{f['tagline']}</p>
  </div>
</header>
<div class="wrap fp">
  <section class="split" aria-labelledby="what">
    <div class="text"><h2 id="what">What it does</h2>{f['problem']}{read}</div>
    {right}
  </section>
  {how}
  <section aria-labelledby="defaults">
    <h2 id="defaults">Out of the box, and what you can tune</h2>
    <ol class="strips feat"><li class="strip" data-state="done"><span class="cs">{f['name']}<small>{f['tagline']}</small></span><span class="what"><span class="lab">By default</span><ul>{items(f['default'])}</ul></span><span class="tune"><span class="lab">You can tune</span><ul>{items(f['tune'])}</ul></span></li></ol>
  </section>
  <section class="split" aria-labelledby="cmds">
    {cmds_block}
    <div><h2 id="limits">Limits, honestly</h2><ul class="limits">{items(f['limits'])}</ul><p class="status"><b>Tried so far.</b> {f['tried']}</p></div>
  </section>
  <section aria-labelledby="near">
    <h2 id="near">Next to it</h2>
    <ol class="strips cat">{related}</ol>
    <p style="margin-top:28px"><a class="more" href="features.html">Every feature</a></p>
  </section>
</div>"""
    path = f"features/{f['slug']}.html"
    desc = text(f["tagline"] + " " + f["problem"])
    desc = desc if len(desc) <= 158 else desc[:155].rsplit(" ", 1)[0] + "…"
    ld = [{"@type": "TechArticle", "headline": text(f["name"]), "description": desc, "url": BASE + path, "inLanguage": "en",
           "image": BASE + "og.png", "author": {"@type": "Organization", "name": "TermF contributors", "url": "https://github.com/vassilidev/TermF"},
           "about": {"@type": "SoftwareApplication", "name": "TermF", "applicationCategory": "DeveloperApplication", "operatingSystem": "macOS, Windows, Linux"}},
          {"@type": "BreadcrumbList", "itemListElement": crumbs(("TermF", ""), ("Features", "features.html"), (text(f["name"]), path))}]
    written.append(page(path, f"{text(f['name'])} · TermF features", desc, body, ld, og_type="article", current="features.html"))

sections = []
for g in groups:
    strips = "".join(feature_strip(f) for f in features if f["group"] == g["id"])
    strips += "".join(f'<li><span class="strip" data-state="queued"><span class="cs">Not yet</span><span class="what">{x}</span><span class="st">Roadmap</span></span></li>' for x in g.get("not_yet", []))
    article = f'<a class="more" href="{g["article"]}">Read: {g["article_title"]}</a>' if g.get("article") else ""
    sections.append(f"""<section class="group" id="{g['id']}" aria-labelledby="g-{g['id']}">
  <header><h2 id="g-{g['id']}">{g['name']}</h2><p>{g['intro']}</p>{article}</header>
  <ol class="strips cat">{strips}</ol>
</section>""")
chips = "".join(f'<button type="button" aria-pressed="false" data-g="{g["id"]}">{g["name"]}</button>' for g in groups)
body = f"""<header class="page-head">
  <div class="wrap">
    <h1>Every feature, with its defaults</h1>
    <p class="lead">{len(features)} features, grouped as you would meet them, from the conversation with the CTO to the chores around the agents. Each one has its page: what it does, how it works, what works out of the box, what you can tune, and its limits.</p>
    <form class="filter" role="search" hidden>
      <label for="q">Filter the features</label>
      <input id="q" type="search" placeholder="worktree, Jira, budget, review…" autocomplete="off" spellcheck="false">
      <div class="chips" role="group" aria-label="Groups"><button type="button" aria-pressed="true" data-g="">All</button>{chips}</div>
      <p class="none" hidden>No feature matches. Try another word.</p>
    </form>
  </div>
</header>
<div class="wrap groups">
{chr(10).join(sections)}
</div>"""
script = """

const form = document.querySelector('.filter'), q = document.getElementById('q'), none = form.querySelector('.none');
let group = '';
form.hidden = false;
form.addEventListener('submit', e => e.preventDefault());
function filter() {
  const words = q.value.toLowerCase().split(/\\s+/).filter(Boolean);
  let shown = 0;
  for (const sec of document.querySelectorAll('.group')) {
    let n = 0;
    for (const li of sec.querySelectorAll('li')) {
      const hit = (!group || sec.id === group) && words.every(w => li.dataset.text.includes(w));
      li.hidden = !hit;
      n += hit;
    }
    sec.hidden = !n;
    shown += n;
  }
  none.hidden = shown > 0;
}
q.addEventListener('input', filter);
form.querySelector('.chips').addEventListener('click', e => {
  const b = e.target.closest('button');
  if (!b) return;
  group = b.dataset.g;
  for (const c of form.querySelectorAll('.chips button')) c.setAttribute('aria-pressed', c === b);
  filter();
});"""
keys = {f["slug"]: text(" ".join([f["name"], f["tagline"], f.get("keywords", ""), by_group[f["group"]]["name"]])).lower() for f in features}
body = re.sub(r'<li><a class="strip" data-state="done" href="features/([a-z0-9-]+)\.html">',
              lambda m: f'<li data-text="{esc(keys[m.group(1)])}"><a class="strip" data-state="done" href="features/{m.group(1)}.html">', body)
body = re.sub(r'<li><span class="strip" data-state="queued">(.*?)</li>',
              lambda m: f'<li data-text="{esc(text(m.group(1)).lower())}"><span class="strip" data-state="queued">{m.group(1)}</li>', body)
ld = [{"@type": "CollectionPage", "name": "TermF features", "url": BASE + "features.html", "description": "Every feature of TermF, with what works by default and what you can tune.",
       "mainEntity": {"@type": "ItemList", "itemListElement": [{"@type": "ListItem", "position": i + 1, "name": text(f["name"]), "url": BASE + f"features/{f['slug']}.html"} for i, f in enumerate(features)]}},
      {"@type": "BreadcrumbList", "itemListElement": crumbs(("TermF", ""), ("Features", "features.html"))}]
written.append(page("features.html", "Features · TermF, every feature and what it does by default",
                    f"The {len(features)} features of TermF, from the CTO and its team of Claude Code agents to terminals, worktrees, skills, CI/CD, keys and security, each with what works out of the box and what you can tune.",
                    body, ld, current="features.html", script=script))

pages = ["" if p == "index.html" else p for p in sorted(os.listdir(SITE)) if p.endswith(".html") and p != "404.html"]
pages += [p for p in written if p.startswith("features/")]
urls = "".join(f"  <url>\n    <loc>{BASE}{p}</loc>\n  </url>\n" for p in sorted(set(pages), key=lambda p: (p != "", p)))
open(os.path.join(SITE, "sitemap.xml"), "w").write(f'<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n{urls}</urlset>\n')
print(f"{len(written)} pages and sitemap.xml ({len(set(pages))} urls)")
