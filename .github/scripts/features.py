#!/usr/bin/env python3
# Builds the website's feature catalogue (site/features.html), one page per feature (site/features/<slug>.html) and
# site/sitemap.xml from site/features.toml, and fills the generated parts of the hand-written pages (the team roster,
# the feature grid and their counts, between <!-- gen:name --> markers) from site/features.toml, site/roles.toml and
# the roles of library/roles/. The icon sprite, the nav and the footer are taken from site/how-it-works.html, so the
# generated pages share them with the hand-written ones.
# Usage: python3 .github/scripts/features.py (Python 3.11+, standard library only). The Pages workflow runs it.
import glob
import json
import os
import re
import tomllib

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
SITE = os.path.join(ROOT, "site")
BASE = "https://vassilidev.github.io/TermF/"

data = tomllib.load(open(os.path.join(SITE, "features.toml"), "rb"))
groups, features, visuals, icons = data["groups"], data["features"], data.get("visuals", {}), data["icons"]
by_slug = {f["slug"]: f for f in features}
by_group = {g["id"]: g for g in groups}
crew = tomllib.load(open(os.path.join(SITE, "roles.toml"), "rb"))


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
assert roles, "no role in library/roles/: the team pages need at least one"
order = {name: i for i, name in enumerate(crew["roles"])}
roles.sort(key=lambda r: (order.get(r["name"], len(order)), r["name"]))
by_crew = {g["id"]: [r for r in roles if r["group"] == g["id"]] for g in crew["groups"]}
names = [r["name"] for r in roles]
COUNTS = {"roles": str(len(roles)), "role_names": " and ".join(filter(None, [", ".join(names[:-1]), names[-1]]))}


def counted(value):
    """{roles} and {role_names} in features.toml, filled from library/roles/."""
    if isinstance(value, str):
        return re.sub(r"\{(roles|role_names)\}", lambda m: COUNTS[m.group(1)], value)
    if isinstance(value, list):
        return [counted(v) for v in value]
    return value


features = [{k: counted(v) for k, v in f.items()} for f in features]
by_slug = {f["slug"]: f for f in features}

# The data is checked before anything is written, so a typo fails the build instead of shipping a broken link
assert len(by_slug) == len(features), "duplicate slug"
assert all(g.get("icon") in icons for g in groups), "a group without a known icon"
assert all(r["group"] in by_crew for r in roles), "a role in an unknown group"
for f in features:
    assert re.fullmatch(r"[a-z0-9-]+", f["slug"]), f["slug"]
    assert f["group"] in by_group, f"{f['slug']}: unknown group {f['group']}"
    assert all(r in by_slug and r != f["slug"] for r in f.get("related", [])), f"{f['slug']}: bad related"
    assert f.get("visual", "") == "" or f["visual"] in visuals or f["visual"].startswith("shot:"), f"{f['slug']}: unknown visual"
    if f.get("visual", "").startswith("shot:"):
        shot = f["visual"][5:].partition("|")[0]
        assert all(os.path.exists(os.path.join(SITE, "shots", f"{shot}-{t}.webp")) for t in ("light", "dark", "light-800", "dark-800")), f"{f['slug']}: no shots/{shot}-*.webp"
    for key in ("name", "tagline", "problem", "how", "default", "tune", "limits", "tried"):
        assert f.get(key), f"{f['slug']}: no {key}"

shell = open(os.path.join(SITE, "how-it-works.html"), encoding="utf-8").read()
SPRITE = re.search(r'<svg width="0".*?</svg>', shell, re.S).group(0)
NAV = re.search(r'<header class="nav".*?</header>', shell, re.S).group(0).replace(' aria-current="page"', "")
FOOTER = re.search(r"<footer>.*?</footer>", shell, re.S).group(0)


def esc(s):
    return s.replace("&", "&amp;").replace('"', "&quot;").replace("<", "&lt;").replace(">", "&gt;")


def text(s):
    return re.sub(r"\s+", " ", re.sub(r"<[^>]+>", "", s)).replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&").strip()


def sprite(names):
    symbols = "".join(f'<symbol id="g-{n}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" '
                      f'stroke-linecap="round" stroke-linejoin="round">{icons[n]}</symbol>' for n in names)
    return f'<svg width="0" height="0" style="position:absolute" aria-hidden="true">{symbols}</svg>'


def ico(name):
    return f'<svg aria-hidden="true"><use href="#g-{name}"/></svg>'


GROUP_ICONS = sorted({g["icon"] for g in groups})


def rebase(page, prefix):
    def fix(m):
        url = m.group(2)
        if re.match(r"(https?:|mailto:|data:|#|/)", url):
            return m.group(0)
        return f'{m.group(1)}="{prefix}{url[2:] if url.startswith("./") else url}"'
    page = re.sub(r'\b(href|src)="([^"]*)"', fix, page)
    return re.sub(r'\bsrcset="([^"]*)"', lambda m: 'srcset="' + ", ".join(prefix + u.strip() for u in m.group(1).split(",")) + '"', page)


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
<meta name="theme-color" content="#f4f5f9" media="(prefers-color-scheme: light)">
<meta name="theme-color" content="#0d0f15" media="(prefers-color-scheme: dark)">
<meta name="color-scheme" content="light dark">
<link rel="icon" type="image/svg+xml" href="icon.svg">
<link rel="icon" type="image/png" sizes="32x32" href="favicon.png">
<link rel="apple-touch-icon" sizes="180x180" href="apple-touch-icon.png">
<link rel="sitemap" type="application/xml" title="Sitemap" href="sitemap.xml">
<link rel="preload" href="fonts/inter.woff2" as="font" type="font/woff2" crossorigin>
<link rel="stylesheet" href="site.css">
<script type="application/ld+json">
{json.dumps({"@context": "https://schema.org", "@graph": ld}, indent=2, ensure_ascii=False)}
</script>
</head>
<body>
<a class="skip" href="#main">Skip to content</a>
{SPRITE}
{sprite(GROUP_ICONS)}

{nav}

<main id="main">
{body}
</main>

{FOOTER}

<script>
const nav = document.getElementById('nav');
addEventListener('scroll', () => nav.classList.toggle('scrolled', scrollY > 40), {{passive: true}});
const menu = nav.querySelector('.menu');
menu.addEventListener('click', e => {{ if (e.target.closest('a')) menu.open = false; }});
addEventListener('click', e => {{ if (menu.open && !menu.contains(e.target)) menu.open = false; }});
addEventListener('keydown', e => {{ if (e.key === 'Escape' && menu.open) {{ menu.open = false; menu.querySelector('summary').focus(); }} }});{script}
</script>
</body>
</html>
"""
    depth = path.count("/")
    html = rebase(html, "../" * depth) if depth else html
    out = os.path.join(SITE, path)
    os.makedirs(os.path.dirname(out), exist_ok=True)
    open(out, "w", encoding="utf-8").write(html)
    return path


def crumbs(*items):
    return [{"@type": "ListItem", "position": i + 1, "name": name, "item": BASE + href} for i, (name, href) in enumerate(items)]


def picture(name, alt, sizes="(max-width: 980px) 100vw, 680px", lazy=True):
    """A screenshot of the app from site/shots/, light and dark, 800 and 1600 px wide."""
    def src(theme):
        return f"shots/{name}-{theme}-800.webp 800w, shots/{name}-{theme}.webp 1600w"
    load = ' loading="lazy"' if lazy else ""
    return (f'<picture><source media="(prefers-color-scheme: dark)" srcset="{src("dark")}" sizes="{sizes}">'
            f'<img src="shots/{name}-light.webp" srcset="{src("light")}" sizes="{sizes}" width="1600" height="1000"{load} alt="{esc(alt)}"></picture>')


def figure(key, caption):
    if key.startswith("shot:"):
        name, _, alt = key[5:].partition("|")
        return f'<figure class="shot"><div class="frame">{picture(name, alt or caption, lazy=False)}</div><figcaption>{esc(caption)}</figcaption></figure>'
    snippet = visuals[key].strip()
    if snippet.startswith('<div class="app'):
        return f'<figure class="shot"><div class="screen">{snippet}</div><figcaption>{esc(caption)}</figcaption></figure>'
    return f'<figure class="shot">{snippet}<figcaption>{esc(caption)}</figcaption></figure>'


def items(xs):
    return "".join(f"<li>{x}</li>" for x in xs)


def card(f, attrs=""):
    """A feature as a card: its group's icon, its name and its one-line summary, the whole card a link."""
    icon = by_group[f["group"]]["icon"]
    return (f'<li{attrs}><a class="fcard" href="features/{f["slug"]}.html"><span class="fi">{ico(icon)}</span>'
            f'<span class="fn">{f["name"]}</span><span class="ft">{f["tagline"]}</span></a></li>')


ARTICLES = {"team.html": "The team", "how-it-works.html": "How a ticket becomes a pull request",
            "security.html": "Security, and its limits", "download.html": "Download TermF",
            "team-vs-solo.html": "The team or one session?"}

written = []

for f in features:
    g = by_group[f["group"]]
    steps = f'<ol class="howto">{items(f["how"])}</ol>'
    if f.get("visual"):
        top = figure(f["visual"], f.get("caption", ""))
        how = f'<section aria-labelledby="how"><h2 id="how">How to use it</h2>{steps}</section>'
    else:
        top = f'<aside class="howto-card" aria-labelledby="how"><h2 id="how">How to use it</h2>{steps}</aside>'
        how = ""
    cmds = "\n\n".join(f"<span class=\"c\"># {esc(text(d))}</span>\n{esc(c)}" for c, d in f.get("commands", []))
    cmds_block = (f'<pre class="code"><code>{cmds}</code></pre>' if cmds
                  else '<p class="plain">Nothing to type: it is all in the app.</p>')
    read = ""
    if f.get("read"):
        target = f["read"]
        read = f'<p><a class="more" href="{target}">Read: {ARTICLES[target.split("#")[0]]}</a></p>'
    related = "".join(card(by_slug[r]) for r in f.get("related", []))
    body = f"""<header class="page-head fp-head">
  <div class="wrap">
    <p class="crumbs"><a href="./">TermF</a><span aria-hidden="true">/</span><a href="features.html">Features</a><span aria-hidden="true">/</span><a class="crumb-g" href="features.html#{g['id']}">{ico(g['icon'])}{g['name']}</a></p>
    <h1>{f['name']}</h1>
    <p class="lead">{f['tagline']}</p>
  </div>
</header>
<div class="wrap fp">
  <section class="fp-top" aria-labelledby="what">
    <div class="text"><h2 id="what">What it does</h2>{f['problem']}{read}</div>
    {top}
  </section>
  {how}
  <section aria-labelledby="defaults">
    <h2 id="defaults">Out of the box, and what you can tune</h2>
    <div class="dt">
      <div><h3><span class="tick done" aria-hidden="true"></span>Works by default</h3><ul>{items(f['default'])}</ul></div>
      <div><h3><span class="tick" aria-hidden="true"></span>You can tune</h3><ul>{items(f['tune'])}</ul></div>
    </div>
  </section>
  <section class="fp-two">
    <div><h2 id="cmds">Commands</h2>{cmds_block}</div>
    <div><h2 id="limits">Limits</h2><div class="callout"><ul>{items(f['limits'])}</ul><p><b>Tried so far.</b> {f['tried']}</p></div></div>
  </section>
  <section aria-labelledby="near">
    <h2 id="near">Related features</h2>
    <ul class="cards three">{related}</ul>
    <p class="after"><a class="more" href="features.html">Every feature</a></p>
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


def in_group(g):
    return [f for f in features if f["group"] == g["id"]]


sections = []
for g in groups:
    cards = "".join(card(f, f' data-text="{esc(text(" ".join([f["name"], f["tagline"], f.get("keywords", ""), g["name"]])).lower())}"')
                    for f in in_group(g))
    soon = f'<div class="soon"><h3>Still to come</h3><ul>{items(g["not_yet"])}</ul></div>' if g.get("not_yet") else ""
    article = f'<p><a class="more" href="{g["article"]}">Read: {g["article_title"]}</a></p>' if g.get("article") else ""
    sections.append(f"""<section class="group" id="{g['id']}" aria-labelledby="g-{g['id']}">
  <header><span class="gi">{ico(g['icon'])}</span><div><h2 id="g-{g['id']}">{g['name']}</h2><p>{g['intro']}</p>{article}</div></header>
  <ul class="cards">{cards}</ul>
  {soon}
</section>""")
toc = "".join(f'<li><a href="#{g["id"]}">{ico(g["icon"])}<span>{g["name"]}</span><small>{len(in_group(g))}</small></a></li>' for g in groups)
body = f"""<header class="page-head">
  <div class="wrap">
    <p class="crumbs"><a href="./">TermF</a><span aria-hidden="true">/</span><span>Features</span></p>
    <h1>Everything TermF does</h1>
    <p class="lead">{len(features)} features in {len(groups)} groups. Each one has its own page: what it does, how to use it, what works out of the box, what you can tune, and its limits.</p>
  </div>
</header>
<div class="wrap catalog">
  <nav class="cat-toc" aria-label="Feature groups"><ol>{toc}</ol></nav>
  <div class="cat-main">
    <form class="filter" role="search" hidden>
      <label for="q">Find a feature</label>
      <input id="q" type="search" placeholder="worktree, Jira, budget, review…" autocomplete="off" spellcheck="false">
      <p class="none" hidden>No feature matches. Try another word.</p>
    </form>
{chr(10).join(sections)}
  </div>
</div>"""
script = """

const form = document.querySelector('.filter'), q = document.getElementById('q'), none = form.querySelector('.none');
form.hidden = false;
form.addEventListener('submit', e => e.preventDefault());
q.addEventListener('input', () => {
  const words = q.value.toLowerCase().split(/\\s+/).filter(Boolean);
  let shown = 0;
  for (const sec of document.querySelectorAll('.group')) {
    let n = 0;
    for (const li of sec.querySelectorAll('.cards li')) {
      li.hidden = !words.every(w => li.dataset.text.includes(w));
      n += !li.hidden;
    }
    sec.hidden = !n;
    const soon = sec.querySelector('.soon');
    if (soon) soon.hidden = words.length > 0;
    const entry = document.querySelector(`.cat-toc a[href="#${sec.id}"]`);
    entry.parentNode.hidden = !n;
    entry.querySelector('small').textContent = n;
    shown += n;
  }
  none.hidden = shown > 0;
});"""
ld = [{"@type": "CollectionPage", "name": "TermF features", "url": BASE + "features.html", "description": "Every feature of TermF, with what works by default and what you can tune.",
       "mainEntity": {"@type": "ItemList", "itemListElement": [{"@type": "ListItem", "position": i + 1, "name": text(f["name"]), "url": BASE + f"features/{f['slug']}.html"} for i, f in enumerate(features)]}},
      {"@type": "BreadcrumbList", "itemListElement": crumbs(("TermF", ""), ("Features", "features.html"))}]
written.append(page("features.html", "Features · Everything TermF does, with its defaults",
                    f"The {len(features)} features of TermF, from the CTO and its team of Claude Code agents to terminals, worktrees, skills, CI/CD, keys and security, each with what works out of the box and what you can tune.",
                    body, ld, current="features.html", script=script))


# The generated parts of the hand-written pages

def agent_meta(r):
    chips = [f"{r['model']} · {r['effort']}" if r["effort"] else r["model"], r["access"], *r["limits"]]
    return "".join(f"<li>{c}</li>" for c in chips if c)


def agent_card(r, big=False):
    rows = [("Called", r.get("when")), ("Never", r.get("never"))]
    dl = "".join(f"<div><dt>{k}</dt><dd>{v}</dd></div>" for k, v in rows if v)
    return (f'<article class="agent{" big" if big else ""}" id="role-{r["name"]}">'
            f'<header><span class="mark" aria-hidden="true">{r["mark"]}</span><div><h3>{r["label"]}</h3><code>{r["name"]}</code></div></header>'
            f'<p>{r["does"]}</p><dl>{dl}</dl><ul class="chips" aria-label="Settings">{agent_meta(r)}</ul></article>')


def team_cards():
    out = []
    for g in crew["groups"]:
        members = by_crew[g["id"]]
        if not members:
            continue
        cards = "".join(agent_card(r, big=r["name"] == "cto") for r in members)
        out.append(f'<section class="crew c-{g["color"]}" id="{g["id"]}" aria-labelledby="crew-{g["id"]}">'
                   f'<header><h2 id="crew-{g["id"]}">{g["name"]}</h2><p>{g["intro"]}</p></header>'
                   f'<div class="agents">{cards}</div></section>')
    return "\n".join(out)


def roster():
    """The home's team: you, the CTO and the expert, then every group at once, each agent a link to its card."""
    def who(r):
        return f'<span class="mark" aria-hidden="true">{r["mark"]}</span><span><span class="rn">{r["label"]}</span><span class="rs">{r["short"]}</span></span>'
    first = crew["groups"][0]
    leads = "".join(f'<a class="org-lead c-{first["color"]}" href="team.html#role-{r["name"]}">{who(r)}</a>' for r in by_crew[first["id"]])
    def badge(r):
        return f'<li><a class="badge-a" href="team.html#role-{r["name"]}">{who(r)}</a></li>'

    def group(g):
        members = by_crew[g["id"]]
        wide = " wide" if len(members) > 6 else ""
        return (f'<section class="org-g c-{g["color"]}{wide}" aria-labelledby="org-{g["id"]}"><h3 id="org-{g["id"]}">{g["name"]}<small>{len(members)}</small></h3>'
                f'<ul>{"".join(badge(r) for r in members)}</ul></section>')
    groups = "".join(group(g) for g in crew["groups"][1:] if by_crew[g["id"]])
    return (f'<div class="org"><div class="org-top"><p class="org-you"><span class="mark" aria-hidden="true">You</span><span>talk to the CTO only</span></p>'
            f'<span class="org-arrow" aria-hidden="true"></span>{leads}</div><div class="org-groups">{groups}</div></div>')


def feature_grid():
    out = []
    for g in groups:
        names = "".join(f'<li><a href="features/{f["slug"]}.html">{f["name"]}</a></li>' for f in in_group(g))
        out.append(f'<section class="fam" aria-labelledby="fam-{g["id"]}"><span class="gi">{ico(g["icon"])}</span>'
                   f'<h3 id="fam-{g["id"]}"><a href="features.html#{g["id"]}">{g["name"]}</a></h3><p>{g["intro"]}</p><ul>{names}</ul></section>')
    out.append(f'<a class="fam-all" href="features.html"><b>Browse all {len(features)} features</b>'
               '<span>Each with what works out of the box, what you can tune, and its limits.</span></a>')
    return "".join(out)


fills = {"count": COUNTS["roles"], "fcount": str(len(features)), "gcount": str(len(groups)),
         "icons": sprite(sorted(set(GROUP_ICONS) | {"laptop", "code-xml", "badge-check"})),
         "roster": roster(), "groups": feature_grid(), "team": team_cards()}
expected = {"index.html": ("count", "fcount", "gcount", "icons", "roster", "groups"), "team.html": ("count", "roster", "team")}
for name, keys in expected.items():
    path = os.path.join(SITE, name)
    source = open(path, encoding="utf-8").read()
    filled = source
    for key in keys:
        filled, n = re.subn(rf"(<!-- gen:{key} -->).*?(<!-- /gen:{key} -->)", lambda m: m.group(1) + fills[key] + m.group(2), filled, flags=re.S)
        assert n, f"{name}: no <!-- gen:{key} --> … <!-- /gen:{key} --> marker"
    if filled != source:
        open(path, "w", encoding="utf-8").write(filled)
        print(f"{name}: generated parts refreshed")

pages = ["" if p == "index.html" else p for p in sorted(os.listdir(SITE)) if p.endswith(".html") and p != "404.html"]
pages += [p for p in written if p.startswith("features/")]
urls = "".join(f"  <url>\n    <loc>{BASE}{p}</loc>\n  </url>\n" for p in sorted(set(pages), key=lambda p: (p != "", p)))
open(os.path.join(SITE, "sitemap.xml"), "w", encoding="utf-8").write(f'<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n{urls}</urlset>\n')
print(f"{len(written)} pages and sitemap.xml ({len(set(pages))} urls), {len(roles)} roles")
