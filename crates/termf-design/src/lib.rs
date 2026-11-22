//! A repository's design system, read from its files alone (nothing of the repository runs): its tokens by
//! family (CSS custom properties, Tailwind's `@theme` and config, SCSS variables, W3C design tokens), where each
//! is defined and how often it is used, its components, the inconsistencies a reviewer would point out, and its
//! brand guidelines (`brand/`, `BRAND.md`) with their PDF export.

pub mod brand;
mod checks;
pub mod color;
mod components;
pub mod css;
mod dtcg;
mod jsobj;
mod markup;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use serde::Serialize;

pub use css::Theme;

const MAX_FILES: usize = 4000;
const MAX_SIZE: u64 = 512 * 1024;
const MAX_BYTES: usize = 40 * 1024 * 1024;
const BUDGET: Duration = Duration::from_secs(8);

/// Never the project's own design: installed or vendored code, build output, tests and their data.
const SKIPPED: [&str; 23] = [
    "node_modules",
    "bower_components",
    "vendor",
    "vendors",
    "third_party",
    "third-party",
    "dist",
    "build",
    "out",
    "coverage",
    "target",
    "storybook-static",
    "__snapshots__",
    "__mocks__",
    "__tests__",
    "__fixtures__",
    "fixtures",
    "testdata",
    "test",
    "tests",
    "e2e",
    "cypress",
    "Pods",
];

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    Color,
    Font,
    Text,
    Weight,
    Leading,
    Spacing,
    Radius,
    Shadow,
    Motion,
    Other,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Css,
    Tailwind,
    Scss,
    Dtcg,
    /// `tailwind.config.*`.
    Config,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Def {
    pub value: String,
    pub file: String,
    pub line: u32,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Token {
    /// How code names it: `--accent`, `$primary`, `colors.brand.500`.
    pub name: String,
    /// The design-token path (`color.brand.500`) it also has.
    pub path: Option<String>,
    pub family: Family,
    pub format: Format,
    /// Its value in the default theme (else its dark one), where it is defined.
    pub value: String,
    pub file: String,
    pub line: u32,
    /// Defined for the default theme (a token defined only for the dark theme is not).
    pub light: bool,
    /// Its own dark-theme definition.
    pub dark: Option<Def>,
    /// The color it gives, other tokens resolved, in the default and the dark theme.
    pub color: Option<String>,
    pub dark_color: Option<String>,
    /// A length in pixels (a rem is 16 px).
    pub px: Option<f64>,
    /// Its value is another token.
    pub alias: bool,
    pub uses: usize,
    /// Its file says it is generated.
    pub generated: bool,
    #[serde(skip)]
    class: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Loc {
    pub file: String,
    pub line: u32,
    /// The line, trimmed and cut.
    pub text: String,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warn,
    Info,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// A literal where a token exists: `subject` the value, `other` the token it equals or nears.
    Hardcoded,
    /// A value off the scale: `subject` the value or the token, `other` the scale.
    OffScale,
    /// Two tokens a person can hardly tell apart: `measure` the ΔE.
    Near,
    /// Tokens with the same value: `subject` the value, `other` the tokens.
    Duplicate,
    Unused,
    Undefined,
    /// A color token without a dark value (`other` empty), or with only one (`other` = ["dark-only"]).
    Dark,
    /// Text on a background under WCAG AA: `subject` the text, `other` the background, `measure` the ratio.
    Contrast,
    /// A font loaded but in no token.
    Font,
    /// Two components with the same name, or the same markup: `other` the second one.
    Component,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Stable across scans while the finding stays: the selection of findings is kept by it.
    pub id: String,
    pub kind: Kind,
    pub level: Level,
    pub family: Option<Family>,
    pub subject: String,
    pub other: Vec<String>,
    pub measure: Option<f64>,
    pub theme: Option<Theme>,
    pub at: Vec<Loc>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Pair {
    /// Token names, or a literal color.
    pub fg: String,
    pub bg: String,
    pub theme: Theme,
    pub fg_color: String,
    pub bg_color: String,
    pub ratio: f64,
    /// Where a rule sets both; none when the pair comes from the names (`--primary` and `--primary-foreground`).
    pub at: Option<Loc>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Variant {
    pub prop: String,
    pub values: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    pub name: String,
    pub file: String,
    pub framework: &'static str,
    pub variants: Vec<Variant>,
    pub uses: usize,
    /// The files that use it, the first ten.
    pub used_in: Vec<String>,
    /// Its Storybook story ids.
    pub stories: Vec<String>,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NoteKind {
    /// A Tailwind config value that is not a literal: `detail` its path.
    Unread,
    /// The scan stopped at its limits: `detail` the files read.
    Truncated,
    /// Files over 512 KB left out: `detail` how many.
    Large,
    /// Custom properties may come from these stylesheet packages (`detail`): undefined ones are not reported.
    Packages,
    /// A published package: its tokens are used elsewhere, unused ones are not reported.
    Published,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Note {
    pub kind: NoteKind,
    pub file: Option<String>,
    pub detail: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Design {
    pub tokens: Vec<Token>,
    pub pairs: Vec<Pair>,
    pub components: Vec<Component>,
    pub findings: Vec<Finding>,
    /// The files tokens were read from.
    pub sources: Vec<String>,
    pub notes: Vec<Note>,
    /// A dark theme is defined.
    pub dark: bool,
    /// The repository has `.storybook/`.
    pub storybook: bool,
    /// `DESIGN.md` at its root.
    pub design_md: bool,
    pub brand: Option<brand::Brand>,
    /// Files read.
    pub files: usize,
}

/// What the files gave, before the tokens are put together.
#[derive(Default)]
struct Read {
    files: Vec<File>,
    vars: Vec<(usize, css::VarDef)>,
    scss: Vec<(usize, String, String, u32)>,
    dtcg: Vec<(usize, dtcg::Token, bool)>,
    config: Vec<(usize, Configured)>,
    /// Declarations that may hold a literal (generated files aside): `(file, decl)`.
    decls: Vec<(usize, css::Decl)>,
    arbitrary: Vec<(usize, u32, String, String)>,
    loaded: Vec<(usize, u32, String)>,
    imports: Vec<(usize, String)>,
    tailwind: bool,
    /// Each custom property's uses (definitions aside), and its `var()` uses without a fallback.
    uses: HashMap<String, usize>,
    bare: HashMap<String, Vec<(usize, u32)>>,
    defined: HashSet<String>,
    scss_uses: HashMap<String, usize>,
    classes: HashMap<String, usize>,
    tags: HashMap<String, (usize, BTreeSet<usize>)>,
    found: Vec<components::Found>,
    stories: Vec<String>,
    notes: Vec<Note>,
    published: bool,
    /// Rule ids, unique across every stylesheet.
    rules: usize,
}

struct File {
    rel: String,
    text: String,
    generated: bool,
}

impl Read {
    fn loc(&self, file: usize, line: u32) -> Loc {
        let f = &self.files[file];
        let text = f
            .text
            .lines()
            .nth(line.saturating_sub(1) as usize)
            .unwrap_or("")
            .trim();
        let text = match text.char_indices().nth(160) {
            Some((cut, _)) => format!("{}…", &text[..cut]),
            None => text.to_owned(),
        };
        Loc {
            file: f.rel.clone(),
            line,
            text,
        }
    }
}

/// A token of `tailwind.config.*`.
struct Configured {
    name: String,
    class: String,
    family: Family,
    value: String,
    line: u32,
}

/// Reads the repository and reports on its design system; bounded in files, size and time.
pub fn scan(repo: &Path) -> Design {
    let read = walk(repo);
    let tokens = assemble(&read);
    let dark = tokens.iter().any(|t| t.dark.is_some());
    let (mut findings, pairs, more) = checks::run(&read, &tokens, dark);
    let components = components::inventory(repo, &read);
    findings.extend(checks::components(&read, &components));
    findings.sort_by(|a, b| a.level.cmp(&b.level).then(b.at.len().cmp(&a.at.len())));
    let sources: BTreeSet<String> = tokens.iter().map(|t| t.file.clone()).collect();
    Design {
        tokens,
        pairs,
        components,
        findings,
        sources: sources.into_iter().collect(),
        notes: read.notes.iter().cloned().chain(more).collect(),
        dark,
        storybook: repo.join(".storybook").is_dir(),
        design_md: repo.join("DESIGN.md").is_file(),
        brand: brand::read(repo),
        files: read.files.len(),
    }
}

enum Lang {
    Style { scss: bool },
    Component(&'static str),
    Script { jsx: bool },
    Html,
    Tokens,
}

fn lang(name: &str) -> Option<Lang> {
    if name.ends_with(".tokens.json") || name.ends_with(".tokens") || name == "tokens.json" {
        return Some(Lang::Tokens);
    }
    let ext = name.rsplit('.').next()?;
    Some(match ext {
        "css" | "pcss" | "postcss" => Lang::Style { scss: false },
        "scss" | "less" => Lang::Style { scss: true },
        "svelte" => Lang::Component("svelte"),
        "vue" => Lang::Component("vue"),
        "astro" => Lang::Component("astro"),
        "jsx" | "tsx" => Lang::Script { jsx: true },
        "js" | "ts" | "mjs" | "cjs" | "mts" | "cts" => Lang::Script { jsx: false },
        "html" | "htm" => Lang::Html,
        _ => return None,
    })
}

fn test_file(name: &str) -> bool {
    [".test.", ".spec.", ".stories.", ".story.", ".cy."]
        .iter()
        .any(|m| name.contains(m))
        || name.ends_with(".d.ts")
}

fn generated(name: &str, text: &str) -> bool {
    if [".min.", ".generated.", ".gen."]
        .iter()
        .any(|m| name.contains(m))
    {
        return true;
    }
    let head: String = text
        .chars()
        .take(400)
        .collect::<String>()
        .to_ascii_lowercase();
    [
        "@generated",
        "auto-generated",
        "autogenerated",
        "do not edit",
        "generated by",
    ]
    .iter()
    .any(|m| head.contains(m))
}

fn walk(repo: &Path) -> Read {
    let mut read = Read::default();
    let deadline = Instant::now() + BUDGET;
    let mut walker = ignore::WalkBuilder::new(repo);
    walker
        .hidden(true)
        .parents(false)
        .git_global(false)
        .require_git(false)
        .max_depth(Some(12))
        .sort_by_file_name(|a, b| a.cmp(b))
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            let dir = e.file_type().is_some_and(|t| t.is_dir());
            // The brand guidelines are a document, with the colors written out on purpose.
            !(dir && (SKIPPED.contains(&name.as_ref()) || e.depth() == 1 && name == "brand"))
        });
    let (mut bytes, mut large, mut stopped) = (0usize, 0usize, false);
    for entry in walker.build().filter_map(Result::ok) {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = entry
            .path()
            .strip_prefix(repo)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");
        if name.contains(".stories.") || name.contains(".story.") {
            read.stories.push(rel);
            continue;
        }
        if name == "package.json" && entry.depth() == 1 {
            read.published = fs::read_to_string(entry.path())
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .is_some_and(|p| {
                    p.get("private") != Some(&serde_json::Value::Bool(true))
                        && (p.get("exports").is_some() || p.get("files").is_some())
                });
            continue;
        }
        let Some(kind) = lang(&name) else {
            continue;
        };
        if test_file(&name) {
            continue;
        }
        if read.files.len() >= MAX_FILES || bytes >= MAX_BYTES || Instant::now() > deadline {
            stopped = true;
            break;
        }
        if entry.metadata().map_or(true, |m| m.len() > MAX_SIZE) {
            large += 1;
            continue;
        }
        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        bytes += text.len();
        let index = read.files.len();
        read.files.push(File {
            generated: generated(&name, &text),
            rel,
            text,
        });
        file(&mut read, index, &name, kind);
    }
    if stopped {
        read.notes.push(Note {
            kind: NoteKind::Truncated,
            file: None,
            detail: Some(read.files.len().to_string()),
        });
    }
    if large > 0 {
        read.notes.push(Note {
            kind: NoteKind::Large,
            file: None,
            detail: Some(large.to_string()),
        });
    }
    read
}

fn file(read: &mut Read, index: usize, name: &str, kind: Lang) {
    let text = std::mem::take(&mut read.files[index].text);
    let generated = read.files[index].generated;
    let lines = markup::Lines::new(&text);
    let mut next = read.rules;
    let sheet = |read: &mut Read, s: css::Sheet| {
        read.tailwind |= s.tailwind;
        read.vars.extend(s.vars.into_iter().map(|v| (index, v)));
        read.scss
            .extend(s.scss.into_iter().map(|(n, v, l)| (index, n, v, l)));
        read.loaded
            .extend(s.faces.into_iter().map(|(f, l)| (index, l, f)));
        read.imports
            .extend(s.imports.into_iter().map(|(i, _)| (index, i)));
        if !generated {
            read.decls.extend(s.decls.into_iter().map(|d| (index, d)));
        }
    };
    match kind {
        Lang::Tokens => {
            read.rules = next;
            if let Some(tokens) = dtcg::read(&text) {
                let dark = name.to_ascii_lowercase().contains("dark");
                read.dtcg
                    .extend(tokens.into_iter().map(|t| (index, t, dark)));
            }
            read.files[index].text = text;
            return;
        }
        Lang::Style { scss } => {
            let s = css::parse(&text, scss, 1, &mut next);
            sheet(read, s);
        }
        Lang::Component(_) | Lang::Html => {
            for (block, line, scss) in css::style_blocks(&text) {
                let s = css::parse(block, scss, line, &mut next);
                sheet(read, s);
            }
        }
        Lang::Script { .. } => {}
    }
    if name.starts_with("tailwind.config.") {
        config(read, index, &text);
    }
    if !matches!(kind, Lang::Style { .. }) && !generated {
        let jsx = matches!(kind, Lang::Script { jsx: true });
        if jsx || !matches!(kind, Lang::Script { .. }) {
            for (at, prop, value) in markup::inline_styles(&text) {
                next += 1;
                let decl = css::Decl {
                    prop,
                    value,
                    line: lines.line(at),
                    rule: next,
                };
                read.decls.push((index, decl));
            }
        }
        for (at, utility, value) in markup::arbitrary(&text) {
            read.arbitrary.push((index, lines.line(at), utility, value));
        }
    }
    for (at, font) in markup::fonts(&text) {
        read.loaded.push((index, lines.line(at), font));
    }
    markup::custom_props(&text, |name, at, defined, in_var, fallback| {
        if defined {
            read.defined.insert(name.to_owned());
            return;
        }
        *read.uses.entry(name.to_owned()).or_default() += 1;
        if in_var && !fallback {
            let at = (index, lines.line(at));
            read.bare.entry(name.to_owned()).or_default().push(at);
        }
    });
    if matches!(kind, Lang::Style { scss: true })
        || name.ends_with(".svelte")
        || name.ends_with(".vue")
    {
        markup::scss_uses(&text, &mut read.scss_uses);
    }
    if !matches!(kind, Lang::Style { .. }) || text.contains("@apply") {
        markup::classes(&text, &mut read.classes);
    }
    if !matches!(kind, Lang::Style { .. }) {
        for (_, tag) in markup::tags(&text) {
            let entry = read.tags.entry(tag).or_default();
            entry.0 += 1;
            entry.1.insert(index);
        }
    }
    if matches!(kind, Lang::Script { .. }) {
        read.imports
            .extend(markup::css_imports(&text).into_iter().map(|i| (index, i)));
    }
    if let Some(found) = components::found(index, &read.files[index].rel, name, &kind, &text) {
        read.found.push(found);
    }
    read.rules = next;
    read.files[index].text = text;
}

/// The literal tokens of a Tailwind 3 config's `theme` and `theme.extend`; the rest is noted as not read.
fn config(read: &mut Read, index: usize, text: &str) {
    const KEYS: [(&str, Family); 14] = [
        ("colors", Family::Color),
        ("backgroundColor", Family::Color),
        ("textColor", Family::Color),
        ("borderColor", Family::Color),
        ("spacing", Family::Spacing),
        ("borderRadius", Family::Radius),
        ("fontFamily", Family::Font),
        ("fontSize", Family::Text),
        ("fontWeight", Family::Weight),
        ("lineHeight", Family::Leading),
        ("boxShadow", Family::Shadow),
        ("transitionDuration", Family::Motion),
        ("transitionTimingFunction", Family::Motion),
        ("letterSpacing", Family::Other),
    ];
    let rel = read.files[index].rel.clone();
    let mut unread = |path: String| {
        read.notes.push(Note {
            kind: NoteKind::Unread,
            file: Some(rel.clone()),
            detail: Some(path),
        })
    };
    let Some((theme, _)) = jsobj::after_key(text, "theme") else {
        return;
    };
    if !matches!(theme, jsobj::Node::Object(_)) {
        unread("theme".into());
        return;
    }
    let mut out = Vec::new();
    let roots = [
        ("theme", Some(&theme)),
        ("theme.extend", theme.get("extend")),
    ];
    for (root, node) in roots {
        let Some(node) = node else { continue };
        for (key, family) in KEYS {
            let Some(value) = node.get(key) else { continue };
            flatten(
                value,
                &format!("{root}.{key}"),
                key,
                "",
                family,
                &mut out,
                &mut unread,
            );
        }
    }
    for (name, class, family, value) in out {
        let line = value_line(text, &value);
        read.config.push((
            index,
            Configured {
                name,
                class,
                family,
                value,
                line,
            },
        ));
    }
}

fn flatten(
    node: &jsobj::Node,
    path: &str,
    name: &str,
    class: &str,
    family: Family,
    out: &mut Vec<(String, String, Family, String)>,
    unread: &mut dyn FnMut(String),
) {
    use jsobj::Node;
    match node {
        Node::Object(entries) => {
            for (key, value) in entries {
                if key == "..." || key.starts_with('[') {
                    unread(format!("{path}.{key}"));
                    continue;
                }
                let (name, class) = if key == "DEFAULT" {
                    (name.to_owned(), class.to_owned())
                } else if class.is_empty() {
                    (format!("{name}.{key}"), key.clone())
                } else {
                    (format!("{name}.{key}"), format!("{class}-{key}"))
                };
                flatten(
                    value,
                    &format!("{path}.{key}"),
                    &name,
                    &class,
                    family,
                    out,
                    unread,
                );
            }
        }
        Node::Str(s) | Node::Num(s) => {
            out.push((name.to_owned(), class.to_owned(), family, s.clone()))
        }
        // A font stack, or a font size with its line height.
        Node::Array(items) => {
            let parts: Vec<&str> = items
                .iter()
                .filter_map(|i| match i {
                    Node::Str(s) | Node::Num(s) => Some(s.as_str()),
                    _ => None,
                })
                .collect();
            let value = if family == Family::Font {
                parts.join(", ")
            } else {
                parts.first().map(|s| s.to_string()).unwrap_or_default()
            };
            if value.is_empty() {
                unread(path.to_owned());
            } else {
                out.push((name.to_owned(), class.to_owned(), family, value));
            }
        }
        Node::Other(_) => unread(path.to_owned()),
    }
}

fn value_line(text: &str, value: &str) -> u32 {
    text.find(value)
        .map_or(1, |at| text[..at].matches('\n').count() as u32 + 1)
}

/// Pixels of a length: `16px`, `1rem` (16 px), `0`.
pub(crate) fn px(value: &str) -> Option<f64> {
    let v = value.trim();
    if v == "0" {
        return Some(0.0);
    }
    if let Some(n) = v.strip_suffix("px") {
        return n.trim().parse().ok();
    }
    if let Some(n) = v.strip_suffix("rem") {
        return n.trim().parse::<f64>().ok().map(|r| r * 16.0);
    }
    None
}

/// Tailwind 4's theme namespaces, the family of their tokens, and the utilities that use them.
const TAILWIND: [(&str, Family); 17] = [
    ("--color-", Family::Color),
    ("--font-weight-", Family::Weight),
    ("--font-", Family::Font),
    ("--text-", Family::Text),
    ("--leading-", Family::Leading),
    ("--tracking-", Family::Other),
    ("--spacing-", Family::Spacing),
    ("--radius-", Family::Radius),
    ("--inset-shadow-", Family::Shadow),
    ("--drop-shadow-", Family::Shadow),
    ("--shadow-", Family::Shadow),
    ("--ease-", Family::Motion),
    ("--animate-", Family::Motion),
    ("--breakpoint-", Family::Other),
    ("--container-", Family::Other),
    ("--blur-", Family::Other),
    ("--aspect-", Family::Other),
];

fn family(name: &str, value: &str, kind: Option<&str>) -> Family {
    match kind {
        Some("color") => return Family::Color,
        Some("fontFamily") => return Family::Font,
        Some("fontWeight") => return Family::Weight,
        Some("duration" | "cubicBezier" | "transition") => return Family::Motion,
        Some("shadow") => return Family::Shadow,
        _ => {}
    }
    let n = name.to_ascii_lowercase();
    let v = value.trim().to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    if color::parse(&v).is_some() && !has(&["shadow", "elevation"]) {
        return Family::Color;
    }
    if has(&["shadow", "elevation"]) {
        return Family::Shadow;
    }
    if v.starts_with("cubic-bezier(")
        || has(&[
            "duration",
            "ease",
            "easing",
            "motion",
            "transition",
            "dur-",
            "-dur",
        ])
        || v.ends_with("ms") && v[..v.len() - 2].parse::<f64>().is_ok()
    {
        return Family::Motion;
    }
    if has(&["radius", "rounded"]) {
        return Family::Radius;
    }
    if has(&["weight"]) {
        return Family::Weight;
    }
    if has(&["leading", "line-height", "lineheight"]) {
        return Family::Leading;
    }
    if has(&["font", "family", "typeface"])
        && (v.contains(',')
            || v.contains('"')
            || v.contains('\'')
            || v.ends_with("serif")
            || v.ends_with("monospace"))
    {
        return Family::Font;
    }
    if px(&v).is_some() || v.ends_with("em") {
        if has(&["font-size", "fontsize", "text", "fs-", "type", "size-"])
            && !has(&["space", "gap"])
        {
            return Family::Text;
        }
        if has(&[
            "space", "spacing", "gap", "gutter", "pad", "margin", "inset", "stack",
        ]) {
            return Family::Spacing;
        }
    }
    if has(&[
        "color",
        "-bg",
        "bg-",
        "background",
        "-fg",
        "foreground",
        "surface",
        "border",
        "accent",
    ]) {
        return Family::Color;
    }
    Family::Other
}

/// The tokens of every format, merged by name, resolved through one another.
fn assemble(read: &Read) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let rel = |f: usize| read.files[f].rel.clone();
    let mut add = |tokens: &mut Vec<Token>,
                   name: &str,
                   path: Option<String>,
                   format: Format,
                   theme: Theme,
                   value: &str,
                   f: usize,
                   line: u32,
                   class: Option<String>| {
        let def = Def {
            value: value.to_owned(),
            file: rel(f),
            line,
        };
        match index.get(name) {
            Some(&i) => {
                let t = &mut tokens[i];
                if t.path.is_none() {
                    t.path = path;
                }
                match theme {
                    Theme::Dark if t.dark.is_none() => t.dark = Some(def),
                    Theme::Light if !t.light => {
                        t.light = true;
                        (t.value, t.file, t.line) = (def.value, def.file, def.line);
                    }
                    _ => {}
                }
            }
            None => {
                index.insert(name.to_owned(), tokens.len());
                let light = theme == Theme::Light;
                tokens.push(Token {
                    name: name.to_owned(),
                    path,
                    family: Family::Other,
                    format,
                    value: def.value.clone(),
                    file: def.file.clone(),
                    line,
                    light,
                    dark: (!light).then_some(def),
                    color: None,
                    dark_color: None,
                    px: None,
                    alias: false,
                    uses: 0,
                    generated: read.files[f].generated,
                    class,
                });
            }
        }
    };
    for (f, v) in read.vars.iter().filter(|(_, v)| v.global) {
        // Tailwind's own sub-properties (`--text-lg--line-height`) and resets (`--color-*: initial`).
        if v.name[2..].contains("--") || v.name.contains('*') {
            continue;
        }
        let class = v
            .tailwind
            .then(|| {
                TAILWIND
                    .iter()
                    .find_map(|(p, _)| v.name.strip_prefix(p).map(str::to_owned))
            })
            .flatten();
        let format = if v.tailwind {
            Format::Tailwind
        } else {
            Format::Css
        };
        add(
            &mut tokens,
            &v.name,
            None,
            format,
            v.theme,
            &v.value,
            *f,
            v.line,
            class,
        );
    }
    for (f, name, value, line) in &read.scss {
        add(
            &mut tokens,
            name,
            None,
            Format::Scss,
            Theme::Light,
            value,
            *f,
            *line,
            None,
        );
    }
    for (f, t, dark) in &read.dtcg {
        let name = format!("--{}", t.path.replace('.', "-"));
        let theme = if *dark { Theme::Dark } else { Theme::Light };
        add(
            &mut tokens,
            &name,
            Some(t.path.clone()),
            Format::Dtcg,
            theme,
            &t.value,
            *f,
            t.line,
            None,
        );
    }
    for (f, c) in &read.config {
        add(
            &mut tokens,
            &c.name,
            None,
            Format::Config,
            Theme::Light,
            &c.value,
            *f,
            c.line,
            Some(c.class.clone()),
        );
    }
    let by_path: HashMap<String, usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.path.clone().map(|p| (p, i)))
        .collect();
    let dtcg_kind: HashMap<&str, &str> = read
        .dtcg
        .iter()
        .filter_map(|(_, t, _)| Some((t.path.as_str(), t.kind.as_deref()?)))
        .collect();
    let config_family: HashMap<&str, Family> = read
        .config
        .iter()
        .map(|(_, c)| (c.name.as_str(), c.family))
        .collect();
    let aliases: Vec<usize> = read
        .dtcg
        .iter()
        .filter_map(|(_, t, _)| t.value.strip_prefix('{')?.strip_suffix('}'))
        .filter_map(|p| by_path.get(p).copied())
        .collect();
    for i in 0..tokens.len() {
        let light = resolve(&tokens, &index, &by_path, &tokens[i].value, Theme::Light, 0);
        let dark_value = tokens[i]
            .dark
            .as_ref()
            .map_or(tokens[i].value.clone(), |d| d.value.clone());
        let dark = resolve(&tokens, &index, &by_path, &dark_value, Theme::Dark, 0);
        let t = &tokens[i];
        let kind = t.path.as_deref().and_then(|p| dtcg_kind.get(p).copied());
        let family = config_family
            .get(t.name.as_str())
            .copied()
            .or_else(|| {
                (t.format == Format::Tailwind)
                    .then(|| {
                        TAILWIND
                            .iter()
                            .find(|(p, _)| t.name.starts_with(p))
                            .map(|(_, f)| *f)
                    })
                    .flatten()
            })
            .unwrap_or_else(|| family(&t.name, &light, kind));
        let alias = is_ref(&t.value);
        let t = &mut tokens[i];
        t.family = family;
        t.alias = alias;
        t.color = color::parse(&light).map(|c| c.hex());
        t.dark_color = color::parse(&dark).map(|c| c.hex());
        t.px = px(&light);
        t.uses = match t.format {
            Format::Scss => read.scss_uses.get(&t.name).copied().unwrap_or(0),
            Format::Config => t
                .class
                .as_ref()
                .and_then(|c| read.classes.get(c))
                .copied()
                .unwrap_or(0),
            _ => {
                read.uses.get(&t.name).copied().unwrap_or(0)
                    + t.class
                        .as_ref()
                        .and_then(|c| read.classes.get(c))
                        .copied()
                        .unwrap_or(0)
            }
        };
    }
    for i in aliases {
        tokens[i].uses += 1;
    }
    tokens
}

fn is_ref(value: &str) -> bool {
    let v = value.trim();
    v.starts_with("var(--")
        || v.starts_with('$') && !v.contains(' ')
        || v.starts_with('{') && v.ends_with('}')
}

/// A value with the tokens it names replaced by theirs, in a theme (the default value where the theme has none).
fn resolve(
    tokens: &[Token],
    index: &HashMap<String, usize>,
    by_path: &HashMap<String, usize>,
    value: &str,
    theme: Theme,
    depth: usize,
) -> String {
    let v = value.trim();
    if depth > 8 {
        return v.to_owned();
    }
    let target = if let Some(inner) = v.strip_prefix("var(").and_then(|r| r.strip_suffix(')')) {
        let (name, fallback) = match inner.split_once(',') {
            Some((n, f)) => (n.trim(), Some(f.trim())),
            None => (inner.trim(), None),
        };
        match (index.get(name), fallback) {
            (Some(&i), _) => i,
            (None, Some(f)) => return resolve(tokens, index, by_path, f, theme, depth + 1),
            (None, None) => return v.to_owned(),
        }
    } else if v.starts_with('$') && !v.contains(' ') {
        match index.get(v) {
            Some(&i) => i,
            None => return v.to_owned(),
        }
    } else if let Some(path) = v.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
        match by_path.get(path) {
            Some(&i) => i,
            None => return v.to_owned(),
        }
    } else {
        return v.to_owned();
    };
    let t = &tokens[target];
    let next = match (theme, &t.dark) {
        (Theme::Dark, Some(d)) => d.value.as_str(),
        _ => t.value.as_str(),
    };
    resolve(tokens, index, by_path, next, theme, depth + 1)
}

#[cfg(test)]
mod tests;
