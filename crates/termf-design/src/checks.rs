//! The inconsistencies a design reviewer would point out, each with where it is. Kept to what is likely a real
//! problem: a value is hard-coded only where the project has tokens of its kind, colors of one numbered scale
//! are never "too close", a token is undefined only when nothing could define it, and so on.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::color::{self, Rgba};
use crate::{
    Component, Family, Finding, Format, Kind, Level, Loc, Note, NoteKind, Pair, Read, TAILWIND,
    Theme, Token, px,
};

/// Two colors under this distance (OKLab ΔE × 100) look the same to most people.
const NEAR: f64 = 2.0;
const AA: f64 = 4.5;
const AA_LARGE: f64 = 3.0;
const MAX_AT: usize = 30;
const MAX_PAIRS: usize = 200;

#[allow(clippy::too_many_arguments)]
fn finding(
    kind: Kind,
    level: Level,
    family: Option<Family>,
    subject: &str,
    other: Vec<String>,
    measure: Option<f64>,
    theme: Option<Theme>,
    mut at: Vec<Loc>,
) -> Finding {
    let count = at.len();
    at.truncate(MAX_AT);
    Finding {
        count,
        id: format!(
            "{kind:?}:{family:?}:{subject}:{}:{}",
            other.first().map_or("", String::as_str),
            theme.map_or("", |t| if t == Theme::Dark { "dark" } else { "light" })
        ),
        kind,
        level,
        family,
        subject: subject.to_owned(),
        other,
        measure: measure.map(|m| (m * 100.0).round() / 100.0),
        theme,
        at,
    }
}

fn def_loc(read: &Read, t: &Token) -> Loc {
    let file = read.files.iter().position(|f| f.rel == t.file);
    match file {
        Some(f) => read.loc(f, t.line),
        None => Loc {
            file: t.file.clone(),
            line: t.line,
            text: String::new(),
        },
    }
}

pub(crate) fn run(
    read: &Read,
    tokens: &[Token],
    dark: bool,
) -> (Vec<Finding>, Vec<Pair>, Vec<Note>) {
    let mut out = Vec::new();
    let mut notes = Vec::new();
    let base = grid(tokens);
    hardcoded(read, tokens, base, &mut out);
    near(read, tokens, &mut out);
    duplicates(read, tokens, &mut out);
    off_scale(read, tokens, base, &mut out);
    unused(read, tokens, &mut out, &mut notes);
    undefined(read, tokens, &mut out, &mut notes);
    if dark {
        dark_theme(read, tokens, &mut out);
    }
    let pairs = contrast(read, tokens, dark, &mut out);
    fonts(read, tokens, &mut out);
    (out, pairs, notes)
}

/// The spacing grid: the largest of 8, 4, 2 px that three quarters of the spacing tokens are multiples of.
fn grid(tokens: &[Token]) -> Option<f64> {
    let values: Vec<f64> = tokens
        .iter()
        .filter(|t| t.family == Family::Spacing && !t.alias)
        .filter_map(|t| t.px)
        .filter(|&v| v > 2.0)
        .collect();
    if values.len() < 4 {
        return None;
    }
    [8.0, 4.0, 2.0]
        .into_iter()
        .find(|b| values.iter().filter(|v| (*v % b).abs() < 0.01).count() * 4 >= values.len() * 3)
}

fn color_prop(prop: &str) -> bool {
    matches!(
        prop,
        "color"
            | "background"
            | "background-color"
            | "fill"
            | "stroke"
            | "caret-color"
            | "accent-color"
    ) || ["border", "outline", "text-decoration", "column-rule"]
        .iter()
        .any(|p| {
            prop.starts_with(p)
                && !prop.contains("radius")
                && !prop.contains("width")
                && !prop.contains("style")
        })
}

/// What a property paints: 0 text, 1 background, 2 border.
fn prop_role(prop: &str) -> Option<usize> {
    if matches!(
        prop,
        "color" | "caret-color" | "fill" | "text-decoration-color"
    ) {
        Some(0)
    } else if prop.starts_with("background") {
        Some(1)
    } else if ["border", "outline", "column-rule", "stroke"]
        .iter()
        .any(|p| prop.starts_with(p))
    {
        Some(2)
    } else {
        None
    }
}

/// What a color token is for, by its name.
fn name_role(name: &str) -> Option<usize> {
    let n = bare(name);
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    if text_like(&n) {
        Some(0)
    } else if ground_like(&n) || has(&["bg", "background", "surface"]) {
        Some(1)
    } else if has(&["border", "line", "divider", "outline", "ring"]) {
        Some(2)
    } else {
        None
    }
}

fn size_family(prop: &str) -> Option<Family> {
    if prop.starts_with("margin")
        || prop.starts_with("padding")
        || matches!(prop, "gap" | "row-gap" | "column-gap")
    {
        Some(Family::Spacing)
    } else if prop == "font-size" {
        Some(Family::Text)
    } else if prop.ends_with("radius") {
        Some(Family::Radius)
    } else {
        None
    }
}

fn utility_family(utility: &str, value: &str) -> Option<Family> {
    const COLORS: [&str; 17] = [
        "bg",
        "text",
        "border",
        "ring",
        "outline",
        "fill",
        "stroke",
        "from",
        "via",
        "to",
        "decoration",
        "accent",
        "caret",
        "divide",
        "placeholder",
        "shadow",
        "ring-offset",
    ];
    const SPACING: [&str; 23] = [
        "p", "px", "py", "pt", "pr", "pb", "pl", "ps", "pe", "m", "mx", "my", "mt", "mr", "mb",
        "ml", "ms", "me", "gap", "gap-x", "gap-y", "space-x", "space-y",
    ];
    if color::parse(value).is_some() {
        let root = utility.split('-').next().unwrap_or(utility);
        return (COLORS.contains(&utility) || COLORS.contains(&root)).then_some(Family::Color);
    }
    px(value)?;
    if SPACING.contains(&utility) {
        Some(Family::Spacing)
    } else if utility == "text" {
        Some(Family::Text)
    } else if utility == "rounded" || utility.starts_with("rounded-") {
        Some(Family::Radius)
    } else {
        None
    }
}

fn fmt_px(v: f64) -> String {
    format!("{}px", (v * 100.0).round() / 100.0)
}

/// Literal colors and sizes where the project has tokens of their kind, grouped by value.
fn hardcoded(read: &Read, tokens: &[Token], base: Option<f64>, out: &mut Vec<Finding>) {
    let count = |f: Family| {
        tokens
            .iter()
            .filter(|t| t.family == f && (t.color.is_some() || t.px.is_some()))
            .count()
    };
    let colors: Vec<(&Token, Rgba, Rgba)> = tokens
        .iter()
        .filter(|t| t.family == Family::Color && t.light)
        .filter_map(|t| {
            let light = color::parse(t.color.as_deref()?)?;
            let dark = t
                .dark_color
                .as_deref()
                .and_then(color::parse)
                .unwrap_or(light);
            Some((t, light, dark))
        })
        .collect();
    // Each literal color with where it is and what it is used for (text, background, border).
    let mut by_color: BTreeMap<String, (Rgba, Vec<Loc>, [usize; 3])> = BTreeMap::new();
    let mut by_size: BTreeMap<(Family, String), (f64, Vec<Loc>)> = BTreeMap::new();
    let mut size = |family: Family, v: f64, loc: Loc| {
        let skip = match family {
            Family::Spacing => v <= 2.0,
            Family::Radius => v <= 2.0 || v >= 999.0,
            _ => false,
        };
        if !skip && count(family) > 0 {
            by_size
                .entry((family, fmt_px(v)))
                .or_insert((v, Vec::new()))
                .1
                .push(loc);
        }
    };
    // A literal that equals or nears a token is always worth a word; any other one only in a real palette.
    let enough_colors = colors.len() >= 3;
    for (f, d) in &read.decls {
        if !colors.is_empty() && color_prop(&d.prop) {
            for (_, c) in color::literals(&d.value) {
                let entry = by_color.entry(c.hex()).or_insert((c, Vec::new(), [0; 3]));
                entry.1.push(read.loc(*f, d.line));
                if let Some(r) = prop_role(&d.prop) {
                    entry.2[r] += 1;
                }
            }
        }
        if let Some(family) = size_family(&d.prop) {
            for part in d.value.split_whitespace() {
                if let Some(v) = px(part).filter(|&v| v > 0.0) {
                    size(family, v, read.loc(*f, d.line));
                }
            }
        }
    }
    for (f, line, utility, value) in &read.arbitrary {
        match utility_family(utility, value) {
            Some(Family::Color) if !colors.is_empty() => {
                if let Some(c) = color::parse(value) {
                    let entry = by_color.entry(c.hex()).or_insert((c, Vec::new(), [0; 3]));
                    entry.1.push(read.loc(*f, *line));
                    let prop = match utility.as_str() {
                        "bg" | "from" | "via" | "to" => "background",
                        "border" | "ring" | "outline" | "divide" => "border",
                        _ => "color",
                    };
                    if let Some(r) = prop_role(prop) {
                        entry.2[r] += 1;
                    }
                }
            }
            Some(family @ (Family::Spacing | Family::Text | Family::Radius)) => {
                if let Some(v) = px(value).filter(|&v| v > 0.0) {
                    size(family, v, read.loc(*f, *line));
                }
            }
            _ => {}
        }
    }
    for (hex, (c, at, roles)) in by_color {
        // The token to use: an equal one before a close one, one made for the same use (a text color for
        // `color`), the default theme's values before the dark ones.
        let wanted = (0..3).filter(|&r| roles[r] > 0).max_by_key(|&r| roles[r]);
        let penalty = |t: &Token| match (wanted, name_role(&t.name)) {
            (Some(w), Some(r)) if w == r => 0,
            (Some(_), Some(_)) => 2,
            _ => 1,
        };
        let pick = |dark: bool| {
            colors
                .iter()
                .map(|(t, l, d)| (t, color::delta_e(&c, if dark { d } else { l })))
                .filter(|(_, e)| *e < NEAR)
                .min_by(|a, b| {
                    (a.1 >= 0.5, penalty(a.0))
                        .cmp(&(b.1 >= 0.5, penalty(b.0)))
                        .then(a.1.total_cmp(&b.1))
                })
        };
        let best = pick(false).or_else(|| pick(true));
        let (level, other, measure) = match best {
            Some((t, e)) if e < 0.5 => (Level::Warn, vec![t.name.clone()], None),
            Some((t, e)) if e < NEAR => (Level::Warn, vec![t.name.clone()], Some(e)),
            _ if enough_colors => (Level::Info, Vec::new(), None),
            _ => continue,
        };
        out.push(finding(
            Kind::Hardcoded,
            level,
            Some(Family::Color),
            &hex,
            other,
            measure,
            None,
            at,
        ));
    }
    for ((family, label), (v, at)) in by_size {
        let same = tokens
            .iter()
            .filter(|t| t.family == family && !t.alias)
            .find(|t| t.px.is_some_and(|p| (p - v).abs() < 0.01));
        let scale = || {
            let mut values: Vec<f64> = tokens
                .iter()
                .filter(|t| t.family == family)
                .filter_map(|t| t.px)
                .collect();
            values.sort_by(f64::total_cmp);
            values.dedup();
            values.into_iter().take(16).map(fmt_px).collect::<Vec<_>>()
        };
        let scaled = count(family) >= if family == Family::Radius { 2 } else { 3 };
        let finding = match (same, family) {
            (None, _) if !scaled => continue,
            (Some(t), _) => finding(
                Kind::Hardcoded,
                Level::Warn,
                Some(family),
                &label,
                vec![t.name.clone()],
                None,
                None,
                at,
            ),
            (None, Family::Text) => finding(
                Kind::OffScale,
                Level::Warn,
                Some(family),
                &label,
                scale(),
                None,
                None,
                at,
            ),
            // Corners have their own scale, not the spacing grid.
            (None, Family::Radius) => finding(
                Kind::OffScale,
                Level::Info,
                Some(family),
                &label,
                scale(),
                None,
                None,
                at,
            ),
            (None, _) if base.is_some_and(|b| (v % b).abs() > 0.01) => finding(
                Kind::OffScale,
                Level::Warn,
                Some(family),
                &label,
                scale(),
                None,
                None,
                at,
            ),
            (None, _) => finding(
                Kind::Hardcoded,
                Level::Info,
                Some(family),
                &label,
                Vec::new(),
                None,
                None,
                at,
            ),
        };
        out.push(finding);
    }
}

/// `--gray-100` and `--gray-200` are steps of one scale: close on purpose.
fn stem(name: &str) -> Option<&str> {
    let trimmed = name.trim_end_matches(|c: char| c.is_ascii_digit());
    (trimmed.len() < name.len()).then(|| trimmed.trim_end_matches(['-', '.', '_']))
}

/// Each token's own color in a theme, when written as a literal (an alias only repeats another token).
fn own_colors(tokens: &[Token], theme: Theme) -> Vec<(&Token, Rgba)> {
    tokens
        .iter()
        .filter(|t| t.family == Family::Color)
        .filter_map(|t| {
            let value = match theme {
                Theme::Light => t.light.then_some(&t.value)?,
                Theme::Dark => &t.dark.as_ref()?.value,
            };
            Some((t, color::parse(value)?))
        })
        .collect()
}

fn near(read: &Read, tokens: &[Token], out: &mut Vec<Finding>) {
    for theme in [Theme::Light, Theme::Dark] {
        let colors = own_colors(tokens, theme);
        for (i, (a, ca)) in colors.iter().enumerate() {
            for (b, cb) in &colors[i + 1..] {
                let e = color::delta_e(ca, cb);
                if e == 0.0 || e >= NEAR || ca.hex() == cb.hex() {
                    continue;
                }
                if stem(&a.name).is_some() && stem(&a.name) == stem(&b.name) {
                    continue;
                }
                let level = if e < 1.0 { Level::Warn } else { Level::Info };
                let at = vec![def_loc(read, a), def_loc(read, b)];
                out.push(finding(
                    Kind::Near,
                    level,
                    Some(Family::Color),
                    &a.name,
                    vec![b.name.clone()],
                    Some(e),
                    Some(theme),
                    at,
                ));
            }
        }
    }
}

fn duplicates(read: &Read, tokens: &[Token], out: &mut Vec<Finding>) {
    for theme in [Theme::Light, Theme::Dark] {
        let mut groups: BTreeMap<String, Vec<&Token>> = BTreeMap::new();
        for (t, c) in own_colors(tokens, theme) {
            groups.entry(c.hex()).or_default().push(t);
        }
        if theme == Theme::Light {
            for t in tokens
                .iter()
                .filter(|t| t.family != Family::Color && t.light && !t.alias)
            {
                let value = match t.px {
                    Some(v) => fmt_px(v),
                    None => t
                        .value
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_ascii_lowercase(),
                };
                groups
                    .entry(format!("{:?}:{value}", t.family))
                    .or_default()
                    .push(t);
            }
        }
        for (key, list) in groups {
            if list.len() < 2 {
                continue;
            }
            let value = key
                .split_once(':')
                .filter(|_| !key.starts_with('#'))
                .map_or(key.as_str(), |(_, v)| v);
            let names: Vec<String> = list.iter().map(|t| t.name.clone()).collect();
            let at = list.iter().map(|t| def_loc(read, t)).collect();
            let theme = (theme == Theme::Dark).then_some(theme);
            out.push(finding(
                Kind::Duplicate,
                Level::Info,
                Some(list[0].family),
                value,
                names,
                None,
                theme,
                at,
            ));
        }
    }
}

fn off_scale(read: &Read, tokens: &[Token], base: Option<f64>, out: &mut Vec<Finding>) {
    let Some(b) = base else { return };
    for t in tokens
        .iter()
        .filter(|t| t.family == Family::Spacing && !t.alias)
    {
        if let Some(v) =
            t.px.filter(|&v| v > 2.0 && v < 999.0 && (v % b).abs() > 0.01)
        {
            out.push(finding(
                Kind::OffScale,
                Level::Info,
                Some(t.family),
                &t.name,
                vec![fmt_px(v), fmt_px(b)],
                None,
                None,
                vec![def_loc(read, t)],
            ));
        }
    }
}

fn unused(read: &Read, tokens: &[Token], out: &mut Vec<Finding>, notes: &mut Vec<Note>) {
    if read.published {
        if tokens.iter().any(|t| t.uses == 0) {
            notes.push(Note {
                kind: NoteKind::Published,
                file: None,
                detail: None,
            });
        }
        return;
    }
    let mut by_family: BTreeMap<Family, Vec<&Token>> = BTreeMap::new();
    for t in tokens.iter().filter(|t| t.uses == 0) {
        // The base of Tailwind's spacing and its breakpoints are used by the framework itself.
        let framework = t.format == Format::Tailwind
            && (t.name == "--spacing"
                || ["--breakpoint-", "--container-", "--default-"]
                    .iter()
                    .any(|p| t.name.starts_with(p)));
        if !framework
            && !(t.format == Format::Config && t.class.as_deref().is_none_or(str::is_empty))
        {
            by_family.entry(t.family).or_default().push(t);
        }
    }
    for (family, list) in by_family {
        let names = list.iter().map(|t| t.name.clone()).collect();
        let at = list.iter().map(|t| def_loc(read, t)).collect();
        out.push(finding(
            Kind::Unused,
            Level::Info,
            Some(family),
            &format!("{family:?}").to_lowercase(),
            names,
            None,
            None,
            at,
        ));
    }
}

fn package_styles(read: &Read) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for (f, target) in &read.imports {
        let t = target.trim();
        // In SCSS and Less, a bare name is a partial of the project; a package is `~name` or `@scope/name`.
        let rel = &read.files[*f].rel;
        let sass = rel.ends_with(".scss")
            || rel.ends_with(".less")
            || rel.ends_with(".svelte")
            || rel.ends_with(".vue");
        if t.contains(':') && !t.starts_with("http")
            || sass && !(t.starts_with('~') || t.starts_with('@') && !t.starts_with("@/"))
        {
            continue;
        }
        let local = t.starts_with('.')
            || t.starts_with('/')
            || t.starts_with("~/")
            || t.starts_with('$')
            || t.starts_with("@/");
        let tailwind =
            t == "tailwindcss" || t.starts_with("tailwindcss/") || t.starts_with("tw-animate-css");
        if !t.is_empty()
            && !local
            && !tailwind
            && !t.starts_with("http")
            && !t.contains("fonts.googleapis")
            && !t.starts_with("@fontsource")
        {
            let package: String = if t.starts_with('@') {
                t.splitn(3, '/').take(2).collect::<Vec<_>>().join("/")
            } else {
                t.split('/').next().unwrap_or(t).to_owned()
            };
            out.insert(package.trim_start_matches('~').to_owned());
        }
    }
    out.into_iter().collect()
}

fn undefined(read: &Read, tokens: &[Token], out: &mut Vec<Finding>, notes: &mut Vec<Note>) {
    let known: BTreeSet<&str> = tokens.iter().map(|t| t.name.as_str()).collect();
    let packages = package_styles(read);
    let mut list: Vec<(&String, &Vec<(usize, u32)>)> = read
        .bare
        .iter()
        .filter(|(name, _)| !read.defined.contains(*name) && !known.contains(name.as_str()))
        .filter(|(name, _)| !name.starts_with("--tw-"))
        .filter(|(name, _)| {
            !read.tailwind
                || !(TAILWIND.iter().any(|(p, _)| name.starts_with(p))
                    || name.starts_with("--spacing")
                    || name.starts_with("--default-"))
        })
        .collect();
    if list.is_empty() {
        return;
    }
    if !packages.is_empty() {
        notes.push(Note {
            kind: NoteKind::Packages,
            file: None,
            detail: Some(packages.join(", ")),
        });
        return;
    }
    list.sort();
    for (name, at) in list {
        let at = at.iter().map(|(f, l)| read.loc(*f, *l)).collect();
        out.push(finding(
            Kind::Undefined,
            Level::Warn,
            None,
            name,
            Vec::new(),
            None,
            None,
            at,
        ));
    }
}

fn semantic(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [
        "bg",
        "background",
        "surface",
        "text",
        "fg",
        "foreground",
        "border",
        "panel",
        "card",
        "muted",
        "canvas",
        "popover",
    ]
    .iter()
    .any(|w| n.split(['-', '.', '_']).any(|p| p == *w))
}

fn dark_theme(read: &Read, tokens: &[Token], out: &mut Vec<Finding>) {
    for t in tokens.iter().filter(|t| t.family == Family::Color) {
        if !t.light && t.dark.is_some() {
            out.push(finding(
                Kind::Dark,
                Level::Warn,
                Some(Family::Color),
                &t.name,
                vec!["dark-only".into()],
                None,
                None,
                vec![def_loc(read, t)],
            ));
            continue;
        }
        let follows = t.color.is_some() && t.dark_color != t.color;
        if t.dark.is_none()
            && !follows
            && t.color.is_some()
            && stem(&t.name).is_none()
            && t.uses > 0
        {
            let level = if semantic(&t.name) {
                Level::Warn
            } else {
                Level::Info
            };
            out.push(finding(
                Kind::Dark,
                level,
                Some(Family::Color),
                &t.name,
                Vec::new(),
                None,
                None,
                vec![def_loc(read, t)],
            ));
        }
    }
}

/// A token's name without its prefix: `--color-primary-foreground` → `primary-foreground`.
fn bare(name: &str) -> String {
    let n = name.trim_start_matches(['-', '$']).to_ascii_lowercase();
    let n = n
        .strip_prefix("color-")
        .or_else(|| n.strip_prefix("colors."))
        .unwrap_or(&n)
        .to_owned();
    n.replace('.', "-")
}

fn text_like(n: &str) -> bool {
    matches!(
        n,
        "text" | "fg" | "foreground" | "muted" | "ink" | "body" | "heading"
    ) || n.starts_with("text-")
        || ["-text", "-fg", "-foreground", "-ink"]
            .iter()
            .any(|s| n.ends_with(s))
}

/// The surfaces text is laid on everywhere; a special one (`--term-bg`) is measured only where a rule uses it.
fn ground_like(n: &str) -> bool {
    matches!(
        n,
        "bg" | "background"
            | "surface"
            | "canvas"
            | "panel"
            | "card"
            | "page"
            | "raised"
            | "popover"
            | "base"
    )
}

/// The pairs worth measuring: a rule's color on its background, `X-foreground` on `X`, and every text color on
/// every background color; each in each theme, measured against WCAG AA.
fn contrast(read: &Read, tokens: &[Token], dark: bool, out: &mut Vec<Finding>) -> Vec<Pair> {
    let list: Vec<&Token> = tokens
        .iter()
        .filter(|t| t.family == Family::Color && t.color.is_some())
        .collect();
    let colors: HashMap<&str, &Token> = list.iter().map(|t| (t.name.as_str(), *t)).collect();
    // `--color-primary: var(--primary)` (Tailwind's theme over shadcn's variables) is `--primary` again: the
    // token with its own value stands for both.
    let mut by_bare: HashMap<String, &Token> = HashMap::new();
    for t in list
        .iter()
        .filter(|t| t.alias)
        .chain(list.iter().filter(|t| !t.alias))
    {
        by_bare.insert(bare(&t.name), t);
    }
    let list: Vec<&Token> = list
        .into_iter()
        .filter(|t| {
            by_bare
                .get(&bare(&t.name))
                .is_some_and(|o| o.name == t.name)
        })
        .collect();
    let themes: &[Theme] = if dark {
        &[Theme::Light, Theme::Dark]
    } else {
        &[Theme::Light]
    };
    let get = |side: &str, theme: Theme| -> Option<Rgba> {
        match colors.get(side) {
            Some(t) => color::parse(match theme {
                Theme::Dark => t.dark_color.as_deref().or(t.color.as_deref())?,
                Theme::Light => t.color.as_deref()?,
            }),
            None => color::parse(side),
        }
    };
    let ground = |theme: Theme| -> Rgba {
        ["bg", "background", "surface", "canvas", "page"]
            .iter()
            .filter_map(|n| by_bare.get(*n))
            .find_map(|t| get(&t.name, theme).filter(|c| c.a >= 0.999))
            .unwrap_or(if theme == Theme::Dark {
                Rgba {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                }
            } else {
                Rgba {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                }
            })
    };
    let ratio = |fg: &str, bg: &str, theme: Theme| -> Option<(f64, Rgba, Rgba)> {
        let back = get(bg, theme)?.over(&ground(theme));
        let front = get(fg, theme)?;
        Some((color::contrast(&front, &back), front.over(&back), back))
    };
    let mut candidates: Vec<(String, String, Option<Loc>)> = Vec::new();
    // A rule that sets both: its text color (and where), and its background.
    type Rule = (Option<(String, Loc)>, Option<String>);
    let mut rules: BTreeMap<usize, Rule> = BTreeMap::new();
    // A literal translucent background shows what is under it, which a rule alone does not say.
    let side = |value: &str, back: bool| -> Option<String> {
        let v = value.trim();
        if let Some(inner) = v.strip_prefix("var(").and_then(|r| r.strip_suffix(')')) {
            let name = inner.split(',').next()?.trim();
            return colors.contains_key(name).then(|| name.to_owned());
        }
        color::parse(v)
            .filter(|c| !back || c.a >= 0.999)
            .map(|_| v.to_owned())
    };
    let mut backgrounds: BTreeSet<String> = BTreeSet::new();
    for (f, d) in &read.decls {
        match d.prop.as_str() {
            "color" => {
                if let Some(s) = side(&d.value, false) {
                    rules.entry(d.rule).or_default().0 = Some((s, read.loc(*f, d.line)));
                }
            }
            "background" | "background-color" => {
                if let Some(s) = side(&d.value, true) {
                    backgrounds.insert(s.clone());
                    rules.entry(d.rule).or_default().1 = Some(s);
                }
            }
            _ => {}
        }
    }
    for (_, (fg, bg)) in rules {
        if let (Some((fg, at)), Some(bg)) = (fg, bg) {
            candidates.push((fg, bg, Some(at)));
        }
    }
    let mut paired: BTreeSet<&str> = BTreeSet::new();
    for t in &list {
        let n = bare(&t.name);
        if n.contains("disabled") || n.contains("placeholder") {
            continue;
        }
        let strict = ["-foreground", "-fg", "-contrast", "-on"]
            .iter()
            .find_map(|s| n.strip_suffix(s))
            .or_else(|| n.strip_prefix("on-"));
        if let Some(base) = strict.and_then(|b| by_bare.get(b)) {
            candidates.push((t.name.clone(), base.name.clone(), None));
            paired.insert(base.name.as_str());
            paired.insert(t.name.as_str());
            continue;
        }
        // `--accent-text` is text on the accent when the accent is a background somewhere, else accent-colored
        // text, measured on the grounds like any other.
        if let Some(base) = n
            .strip_suffix("-text")
            .and_then(|b| by_bare.get(b))
            .filter(|b| backgrounds.contains(&b.name))
        {
            candidates.push((t.name.clone(), base.name.clone(), None));
            paired.insert(t.name.as_str());
        }
    }
    let texts: Vec<&&Token> = list
        .iter()
        .filter(|t| text_like(&bare(&t.name)) && !paired.contains(t.name.as_str()))
        .collect();
    let grounds: Vec<&&Token> = list
        .iter()
        .filter(|t| {
            ground_like(&bare(&t.name)) && get(&t.name, Theme::Light).is_some_and(|c| c.a >= 0.999)
        })
        .collect();
    for fg in &texts {
        for bg in &grounds {
            candidates.push((fg.name.clone(), bg.name.clone(), None));
        }
    }
    let mut pairs = Vec::new();
    let mut seen = BTreeSet::new();
    for (fg, bg, at) in candidates {
        if fg == bg {
            continue;
        }
        for &theme in themes {
            if pairs.len() >= MAX_PAIRS
                || !seen.insert((fg.clone(), bg.clone(), theme == Theme::Dark))
            {
                continue;
            }
            let Some((r, front, back)) = ratio(&fg, &bg, theme) else {
                continue;
            };
            if r < AA {
                let level = if r < AA_LARGE {
                    Level::Error
                } else {
                    Level::Warn
                };
                let theme_of = dark.then_some(theme);
                let mut locs: Vec<Loc> = at.iter().cloned().collect();
                for side in [&fg, &bg] {
                    if let Some(t) = colors.get(side.as_str()) {
                        locs.push(def_loc(read, t));
                    }
                }
                out.push(finding(
                    Kind::Contrast,
                    level,
                    Some(Family::Color),
                    &fg,
                    vec![bg.clone()],
                    Some(r),
                    theme_of,
                    locs,
                ));
            }
            pairs.push(Pair {
                fg: fg.clone(),
                bg: bg.clone(),
                theme,
                fg_color: front.hex(),
                bg_color: back.hex(),
                ratio: (r * 100.0).round() / 100.0,
                at: at.clone(),
            });
        }
    }
    pairs.sort_by(|a, b| a.ratio.total_cmp(&b.ratio));
    pairs
}

fn font_key(name: &str) -> String {
    let n = crate::css::unquote(name)
        .to_ascii_lowercase()
        .replace('-', " ");
    n.strip_suffix(" variable").unwrap_or(&n).trim().to_owned()
}

fn fonts(read: &Read, tokens: &[Token], out: &mut Vec<Finding>) {
    let declared: BTreeSet<String> = tokens
        .iter()
        .filter(|t| t.family == Family::Font)
        .flat_map(|t| t.value.split(',').map(font_key).collect::<Vec<_>>())
        .collect();
    if declared.is_empty() {
        return;
    }
    let mut loaded: BTreeMap<String, (String, Vec<Loc>)> = BTreeMap::new();
    for (f, line, font) in &read.loaded {
        let key = font_key(font);
        if !key.is_empty() && !declared.contains(&key) {
            loaded
                .entry(key)
                .or_insert_with(|| (crate::css::unquote(font), Vec::new()))
                .1
                .push(read.loc(*f, *line));
        }
    }
    for (_, (font, at)) in loaded {
        out.push(finding(
            Kind::Font,
            Level::Warn,
            Some(Family::Font),
            &font,
            Vec::new(),
            None,
            None,
            at,
        ));
    }
}

/// Components that share a name, or their markup: likely one too many.
pub(crate) fn components(read: &Read, list: &[Component]) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut by_name: BTreeMap<String, Vec<&Component>> = BTreeMap::new();
    for c in list {
        by_name
            .entry(c.name.to_ascii_lowercase())
            .or_default()
            .push(c);
    }
    for same in by_name.values().filter(|l| l.len() > 1) {
        let files: Vec<String> = same.iter().map(|c| c.file.clone()).collect();
        let at = files
            .iter()
            .map(|f| Loc {
                file: f.clone(),
                line: 1,
                text: String::new(),
            })
            .collect();
        out.push(finding(
            Kind::Component,
            Level::Warn,
            None,
            &same[0].name,
            files[1..].to_vec(),
            None,
            None,
            at,
        ));
    }
    let mut by_shape: BTreeMap<&Vec<String>, Vec<&crate::components::Found>> = BTreeMap::new();
    for f in read.found.iter().filter(|f| f.skeleton.len() >= 6) {
        by_shape.entry(&f.skeleton).or_default().push(f);
    }
    for same in by_shape.values().filter(|l| l.len() > 1) {
        let names: BTreeSet<String> = same.iter().map(|f| f.name.to_ascii_lowercase()).collect();
        if names.len() < 2 {
            continue;
        }
        let files: Vec<String> = same
            .iter()
            .map(|f| read.files[f.file].rel.clone())
            .collect();
        let at = files
            .iter()
            .map(|f| Loc {
                file: f.clone(),
                line: 1,
                text: String::new(),
            })
            .collect();
        let others = same[1..].iter().map(|f| f.name.clone()).collect();
        out.push(finding(
            Kind::Component,
            Level::Info,
            None,
            &same[0].name,
            others,
            None,
            None,
            at,
        ));
    }
    out
}
