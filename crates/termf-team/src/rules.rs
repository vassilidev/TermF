//! Workflow rules (ARCHITECTURE §7.6): which apply at which level, and the checks of the shipped ones, run on
//! what a task changed. `scope`: files outside the role's `writes` or the plan's files for the task;
//! `text-changes`: visible or editorial text changed without being asked for.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use termf_library::{Kind, Layer, Scope};

use crate::plan::{Report, Task};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Off,
    /// Instructions in the prompt, no check.
    Advise,
    /// Checked; what is found is reported.
    Warn,
    /// Checked; what is found fails the step.
    Block,
}

impl Level {
    pub fn parse(text: &str) -> Result<Level> {
        serde_json::from_value(Value::String(text.trim().to_lowercase())).map_err(|_| {
            anyhow::anyhow!("{text:?} is not a rule level: off, advise, warn or block")
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Level::Off => "off",
            Level::Advise => "advise",
            Level::Warn => "warn",
            Level::Block => "block",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Rule {
    pub name: String,
    pub level: Level,
    /// A shipped check (`scope`, `text-changes`, `ui-changed`), `script` for a rule's own, or none.
    pub check: Option<String>,
    pub applies_to: Vec<String>,
}

impl Rule {
    pub fn applies(&self, role: &str) -> bool {
        self.applies_to.is_empty() || self.applies_to.iter().any(|a| a == "all" || a == role)
    }
}

/// Something a check found.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Violation {
    pub rule: String,
    pub file: Option<String>,
    pub message: String,
    /// It fails the step: the rule is at `block` and it is more than a signal.
    pub blocking: bool,
}

/// `[rules]` of a TOML file: rule levels by name.
pub fn read_levels(file: &Path) -> Result<BTreeMap<String, Level>> {
    let Ok(text) = fs::read_to_string(file) else {
        return Ok(BTreeMap::new());
    };
    let table: toml::Table =
        toml::from_str(&text).with_context(|| format!("invalid {}", file.display()))?;
    let Some(rules) = table.get("rules") else {
        return Ok(BTreeMap::new());
    };
    let Some(rules) = rules.as_table() else {
        bail!("`rules` in {} must be a table", file.display());
    };
    rules
        .iter()
        .map(|(name, level)| {
            let level = level
                .as_str()
                .with_context(|| format!("rule {name}: the level must be text"))?;
            Ok((name.clone(), Level::parse(level)?))
        })
        .collect()
}

/// The library's rules, each at its level: its own when the library has it on (off otherwise), then the
/// levels of `layers`, from the least to the most specific. A rule a cloned repository brought stays off
/// until it is switched on.
pub fn resolve(
    config_dir: &Path,
    scope: &Scope,
    layers: &[&BTreeMap<String, Level>],
) -> Result<Vec<Rule>> {
    let mut rules = Vec::new();
    for item in termf_library::resolve(config_dir, scope)? {
        if item.kind != Kind::Rule {
            continue;
        }
        let own = item
            .meta
            .get("level")
            .and_then(Value::as_str)
            .map(Level::parse);
        let mut level = match item.active {
            true => own.unwrap_or(Ok(Level::Block)).unwrap_or(Level::Block),
            false => Level::Off,
        };
        let trusted = item.layer != Layer::Repo || item.active;
        for layer in layers {
            if let Some(l) = layer.get(&item.name).filter(|_| trusted) {
                level = *l;
            }
        }
        let check = match item.meta.get("check") {
            Some(Value::String(name)) => Some(name.clone()),
            Some(Value::Object(_)) => Some("script".to_owned()),
            _ => None,
        };
        rules.push(Rule {
            name: item.name,
            level,
            check,
            applies_to: item.applies_to,
        });
    }
    Ok(rules)
}

/// `*` within a folder, `**` across folders, `?` one character.
pub fn glob(pattern: &str, path: &str) -> bool {
    fn go(p: &[u8], s: &[u8]) -> bool {
        match p {
            [] => s.is_empty(),
            [b'*', b'*', rest @ ..] => {
                let rest = rest.strip_prefix(b"/").unwrap_or(rest);
                (0..=s.len()).any(|i| go(rest, &s[i..]))
            }
            [b'*', rest @ ..] => (0..=s.len())
                .take_while(|&i| i == 0 || s[i - 1] != b'/')
                .any(|i| go(rest, &s[i..])),
            [b'?', rest @ ..] => s.first().is_some_and(|c| *c != b'/') && go(rest, &s[1..]),
            [c, rest @ ..] => s.first() == Some(c) && go(rest, &s[1..]),
        }
    }
    go(pattern.as_bytes(), path.as_bytes())
}

/// Whether `hint` (a file, a folder or a glob) takes in `path`.
pub fn covers(hint: &str, path: &str) -> bool {
    let hint = hint.trim().trim_start_matches("./").trim_end_matches('/');
    !hint.is_empty() && (hint == path || path.starts_with(&format!("{hint}/")) || glob(hint, path))
}

/// Whether two tasks may touch the same files. A task without hints may touch anything.
pub fn overlap(a: &[String], b: &[String]) -> bool {
    let fixed = |h: &str| {
        let h = h.trim().trim_start_matches("./");
        h[..h.find(['*', '?']).unwrap_or(h.len())]
            .trim_end_matches('/')
            .to_owned()
    };
    a.is_empty()
        || b.is_empty()
        || a.iter().any(|x| {
            b.iter().any(|y| {
                let (x, y) = (fixed(x), fixed(y));
                x.is_empty() || y.is_empty() || covers(&x, &y) || covers(&y, &x)
            })
        })
}

/// The `scope` check: each file outside what the role may write, or neither planned for the task nor explained
/// in its report; and a diff out of proportion with the task's size, which is only a signal.
pub fn check_scope(
    task: &Task,
    writes: &[String],
    report: &Report,
    files: &[String],
    lines: usize,
) -> Vec<(Option<String>, String, bool)> {
    let said = format!(
        "{}\n{}\n{}",
        report.summary,
        report.out_of_scope_findings.join("\n"),
        report
            .text_changes
            .iter()
            .map(|t| t.file.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    let mut found = Vec::new();
    for file in files {
        if !writes.is_empty() && !writes.iter().any(|w| covers(w, file)) {
            found.push((
                Some(file.clone()),
                "outside what the role may write".to_owned(),
                true,
            ));
        } else if !task.files_hint.is_empty()
            && !task.files_hint.iter().any(|h| covers(h, file))
            && !said.contains(file.as_str())
        {
            found.push((
                Some(file.clone()),
                format!(
                    "not among the files planned for task {}, nor explained in its report",
                    task.id
                ),
                true,
            ));
        }
    }
    let size = task.size;
    if lines > size.lines() {
        found.push((
            None,
            format!("{lines} lines changed for a task sized {size:?}"),
            false,
        ));
    }
    found
}

/// Texts of a diff (`git diff -U0`) that changed, by file: string literals that read as prose, text between
/// tags, `alt`/`title`/`placeholder`/`aria-label` values, Markdown and translation lines, and existing comments
/// that changed. A text both removed and added in the same file only moved. Tests, snapshots and lockfiles are
/// left out.
// shortcut: a line-based heuristic, not the tree-sitter parse ARCHITECTURE §7.6 plans; it misses text split
// over several lines and may take a prose-like code string for text.
pub fn changed_texts(diff: &str) -> Vec<(String, String)> {
    let mut by_file: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)> = BTreeMap::new();
    let mut file: Option<String> = None;
    // Between `diff --git` and the first hunk: an added `++ x` or a removed `-- x` is not a header.
    let mut header = false;
    for line in diff.lines() {
        if line.starts_with("diff ") {
            (header, file) = (true, None);
            continue;
        }
        if line.starts_with("@@") {
            header = false;
            continue;
        }
        if header {
            if let Some(path) = line.strip_prefix("+++ ") {
                file = path
                    .strip_prefix("b/")
                    .map(str::to_owned)
                    .filter(|f| !excluded(f));
            }
            continue;
        }
        let Some(file) = &file else { continue };
        let (added, text) = match line.as_bytes().first() {
            Some(b'+') => (true, &line[1..]),
            Some(b'-') => (false, &line[1..]),
            _ => continue,
        };
        let entry = by_file.entry(file.clone()).or_default();
        let set = if added { &mut entry.0 } else { &mut entry.1 };
        set.extend(texts(file, text, !added));
    }
    let mut changed = Vec::new();
    for (file, (added, removed)) in by_file {
        for text in added.symmetric_difference(&removed) {
            changed.push((file.clone(), text.clone()));
        }
    }
    changed
}

/// Tests, snapshots, fixtures and lockfiles, by their folders and names.
fn excluded(file: &str) -> bool {
    let lower = file.to_lowercase();
    let mut parts: Vec<&str> = lower.split('/').collect();
    let name = parts.pop().unwrap_or_default();
    let folders = [
        "test",
        "tests",
        "spec",
        "specs",
        "__tests__",
        "__snapshots__",
        "fixtures",
        "__fixtures__",
    ];
    parts.iter().any(|p| folders.contains(p))
        || [".test.", ".spec.", "_test.", "-lock."]
            .iter()
            .any(|p| name.contains(p))
        || name.starts_with("test_")
        || name.ends_with(".snap")
        || name.ends_with(".lock")
}

fn extension(file: &str) -> String {
    Path::new(file)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Whether the file holds the interface: components, styles, templates, translations.
pub fn ui_file(file: &str) -> bool {
    let ext = extension(file);
    [
        "tsx", "jsx", "vue", "svelte", "css", "scss", "sass", "less", "html", "hbs",
    ]
    .contains(&ext.as_str())
        || translations(file)
}

fn translations(file: &str) -> bool {
    let lower = file.to_lowercase();
    ["po", "xliff", "xlf"].contains(&extension(file).as_str())
        || (["json", "yaml", "yml"].contains(&extension(file).as_str())
            && ["i18n", "locale", "lang", "translation", "messages"]
                .iter()
                .any(|d| lower.contains(d)))
}

fn prose(text: &str) -> bool {
    let text = text.trim();
    let letters = text.chars().filter(|c| c.is_alphabetic()).count();
    letters >= 2
        && !text.contains("://")
        && (text.contains(' ') || text.ends_with(['.', '!', '?', ':']) || !text.is_ascii())
}

fn texts(file: &str, line: &str, removed: bool) -> Vec<String> {
    let ext = extension(file);
    let trimmed = line.trim();
    if ["md", "mdx", "markdown", "txt", "rst"].contains(&ext.as_str()) {
        let text = trimmed.trim_start_matches(['#', '-', '*', '>', ' ']).trim();
        return match text.chars().any(char::is_alphabetic) {
            true => vec![text.to_owned()],
            false => Vec::new(),
        };
    }
    if ["json", "yaml", "yml", "po"].contains(&ext.as_str()) && translations(file) {
        let value = match ext.as_str() {
            "yaml" | "yml" => trimmed
                .split_once(':')
                .map(|(_, v)| v.trim().trim_matches(['"', '\'']).to_owned()),
            // JSON, PO: the last string of the line is the translation.
            _ => literals(trimmed).pop(),
        };
        return value
            .filter(|v| v.chars().any(char::is_alphabetic))
            .into_iter()
            .collect();
    }
    let mut found = Vec::new();
    found.extend(literals(trimmed).into_iter().filter(|l| prose(l)));
    for attr in ["alt=", "title=", "placeholder=", "aria-label="] {
        let mut rest = trimmed;
        while let Some(at) = rest.find(attr) {
            rest = &rest[at + attr.len()..];
            if let Some(value) = literals(rest).into_iter().next()
                && value.chars().any(char::is_alphabetic)
            {
                found.push(value);
            }
        }
    }
    if ["tsx", "jsx", "vue", "svelte", "html", "hbs", "xliff", "xlf"].contains(&ext.as_str()) {
        let mut rest = trimmed;
        while let Some(start) = rest.find('>') {
            rest = &rest[start + 1..];
            let end = rest.find('<').unwrap_or(rest.len());
            let text = rest[..end].trim();
            if text.chars().any(char::is_alphabetic) && !text.contains(['{', '}', '=', ';', '(']) {
                found.push(text.to_owned());
            }
            rest = &rest[end..];
        }
    }
    if removed && let Some(comment) = comment(trimmed, &ext) {
        found.push(comment);
    }
    found.retain(|t| !t.is_empty());
    found
}

/// The quoted strings of a line.
fn literals(line: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut chars = line.char_indices().peekable();
    while let Some((start, quote)) = chars.next() {
        if !matches!(quote, '"' | '\'' | '`') {
            continue;
        }
        let mut text = String::new();
        let mut closed = false;
        while let Some((_, c)) = chars.next() {
            match c {
                '\\' => {
                    if let Some((_, next)) = chars.next() {
                        text.push(next);
                    }
                }
                c if c == quote => {
                    closed = true;
                    break;
                }
                c => text.push(c),
            }
        }
        // An apostrophe in prose (`don't`) is not a quote.
        let word_before = line[..start]
            .chars()
            .last()
            .is_some_and(char::is_alphanumeric);
        if closed && !(quote == '\'' && word_before) {
            found.push(text);
        }
    }
    found
}

fn comment(line: &str, ext: &str) -> Option<String> {
    let hash = ["py", "sh", "rb", "toml", "yaml", "yml", "conf"].contains(&ext);
    let text = if let Some(rest) = line.strip_prefix("//") {
        rest
    } else if let Some(rest) = line.strip_prefix("/*").or_else(|| line.strip_prefix('*')) {
        rest.trim_end_matches("*/")
    } else if let Some(rest) = line.strip_prefix("<!--") {
        rest.trim_end_matches("-->")
    } else if hash && let Some(rest) = line.strip_prefix('#') {
        rest
    } else {
        return None;
    };
    let text = text.trim();
    text.chars()
        .any(char::is_alphabetic)
        .then(|| text.to_owned())
}

fn normal(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The `text-changes` check: each changed text that neither the ticket nor `report.text_changes` names.
pub fn check_texts(
    diff: &str,
    ticket: &str,
    report: &Report,
) -> Vec<(Option<String>, String, bool)> {
    let ticket = normal(ticket);
    let declared: Vec<String> = report
        .text_changes
        .iter()
        .map(|t| normal(&t.text))
        .filter(|t| !t.is_empty())
        .collect();
    let mut found = Vec::new();
    for (file, text) in changed_texts(diff) {
        let text_n = normal(&text);
        let justified = ticket.contains(&text_n)
            || declared
                .iter()
                .any(|d| d.contains(&text_n) || text_n.contains(d.as_str()));
        if !justified {
            found.push((
                Some(file),
                format!("text {text:?} changed without the ticket asking for it or report.text_changes declaring it"),
                true,
            ));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{ReportStatus, Size, TextChange};

    fn report(summary: &str, texts: &[&str]) -> Report {
        Report {
            status: ReportStatus::Done,
            summary: summary.into(),
            covers: vec![],
            text_changes: texts
                .iter()
                .map(|t| TextChange {
                    file: "src/a.tsx".into(),
                    text: t.to_string(),
                    justified_by: "AC1".into(),
                })
                .collect(),
            affected_views: vec![],
            out_of_scope_findings: vec![],
            questions: vec![],
        }
    }

    #[test]
    fn matches_globs_hints_and_overlaps() {
        assert!(glob("src/*.ts", "src/a.ts"));
        assert!(!glob("src/*.ts", "src/x/a.ts"));
        assert!(glob("src/**/*.ts", "src/a.ts") && glob("src/**/*.ts", "src/x/y/a.ts"));
        assert!(glob("**", "anything/at/all") && glob("a?c", "abc") && !glob("a?c", "a/c"));
        assert!(covers("src/api", "src/api/users.ts") && covers("./src/api/", "src/api/x"));
        assert!(!covers("src/api", "src/apix.ts") && !covers("", "a"));
        let v = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(!overlap(&v(&["src/api"]), &v(&["web/pages/a.tsx"])));
        assert!(overlap(&v(&["src/api/**"]), &v(&["src/api/users.ts"])));
        assert!(overlap(&v(&["src/a.ts"]), &v(&["src/a.ts"])));
        assert!(overlap(&v(&[]), &v(&["x"])), "no hint: anything");
        assert!(
            overlap(&v(&["**/*.ts"]), &v(&["x"])),
            "a glob from the root"
        );
    }

    #[test]
    fn checks_the_scope_of_a_task() {
        let task = Task {
            id: "T1".into(),
            role: "back".into(),
            repo: "api".into(),
            size: Size::S,
            covers: vec![],
            goal: "g".into(),
            files_hint: vec!["src/api".into()],
            depends_on: vec![],
            ui_impact: false,
        };
        let files: Vec<String> = ["src/api/a.ts", "README.md", "web/x.ts", "src/util.ts"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let found = check_scope(
            &task,
            &["src/**".into(), "README.md".into()],
            &report("Also fixed src/util.ts, which the endpoint needs.", &[]),
            &files,
            250,
        );
        let files: Vec<Option<&str>> = found.iter().map(|f| f.0.as_deref()).collect();
        assert_eq!(
            files,
            [Some("README.md"), Some("web/x.ts"), None],
            "{found:?}"
        );
        assert!(found[1].1.contains("role may write"));
        assert!(found[0].1.contains("planned for task T1"));
        assert!(!found[2].2, "the size is only a signal");
    }

    #[test]
    fn finds_texts_changed_without_being_asked() {
        let diff = r#"diff --git a/src/Invoices.tsx b/src/Invoices.tsx
--- a/src/Invoices.tsx
+++ b/src/Invoices.tsx
@@ -3 +3,2 @@
-      <h1>Factures</h1>
+      <h1>Mes factures</h1>
+      <Button onClick={exportCsv} aria-label="Export">Exporter en CSV</Button>
@@ -9 +10 @@
-  // Loads the invoices once.
+  const url = "/api/invoices?format=csv";
diff --git a/src/api.ts b/src/api.ts
--- a/src/api.ts
+++ b/src/api.ts
@@ -1 +1 @@
-  throw new Error("Cannot load the invoices.");
+    throw new Error("Cannot load the invoices.");
@@ -5 +5 @@
+  log.debug('don\'t care');
diff --git a/src/Invoices.test.tsx b/src/Invoices.test.tsx
--- a/src/Invoices.test.tsx
+++ b/src/Invoices.test.tsx
@@ -1 +1 @@
+  expect(screen.getByText("Some new text here")).toBeVisible();
diff --git a/locales/fr.json b/locales/fr.json
--- a/locales/fr.json
+++ b/locales/fr.json
@@ -2 +2 @@
-  "title": "Factures",
+  "title": "Toutes les factures",
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1 +1 @@
+## Export CSV
"#;
        let texts: Vec<(String, String)> = changed_texts(diff);
        let only: Vec<&str> = texts.iter().map(|(_, t)| t.as_str()).collect();
        for expected in [
            "Mes factures",
            "Factures",
            "Export",
            "Exporter en CSV",
            "Loads the invoices once.",
            "don't care",
            "Toutes les factures",
            "Export CSV",
        ] {
            assert!(only.contains(&expected), "{expected} in {only:?}");
        }
        assert!(
            !only.contains(&"Cannot load the invoices."),
            "only moved: {only:?}"
        );
        assert!(
            !only.iter().any(|t| t.contains("Some new text")),
            "tests are left out"
        );
        assert!(
            !only.iter().any(|t| t.contains("/api/")),
            "a path is not prose"
        );

        let ticket = "<ticket>Add a button « Exporter en CSV » to the Factures page</ticket>";
        let found = check_texts(diff, ticket, &report("", &["Mes factures", "Export CSV"]));
        let flagged: Vec<String> = found.iter().map(|f| f.1.clone()).collect();
        let flagged = flagged.join("\n");
        assert!(!flagged.contains("\"Exporter en CSV\""), "in the ticket");
        assert!(!flagged.contains("\"Mes factures\""), "declared");
        assert!(flagged.contains("Toutes les factures") && flagged.contains("Loads the invoices"));
        assert!(found.iter().all(|f| f.2));
        assert!(
            ui_file("src/Invoices.tsx") && ui_file("locales/fr.json") && !ui_file("src/api.ts")
        );
    }

    #[test]
    fn leaves_out_tests_and_lockfiles_by_name_only() {
        for file in [
            "src/__tests__/a.tsx",
            "src/Invoices.test.tsx",
            "web/b.spec.ts",
            "tests/x.py",
            "pkg/a_test.go",
            "test_api.py",
            "e2e/fixtures/a.json",
            "src/__snapshots__/a.tsx.snap",
            "pnpm-lock.yaml",
            "package-lock.json",
            "Cargo.lock",
        ] {
            assert!(excluded(file), "{file}");
        }
        for file in [
            "src/LatestInvoices.tsx",
            "src/Inspector.svelte",
            "web/special-offers/a.tsx",
            "web/contest/a.tsx",
            "src/blocking.ts",
        ] {
            assert!(!excluded(file), "{file}");
        }
        let diff = "diff --git a/notes.md b/notes.md\n--- a/notes.md\n+++ b/notes.md\n@@ -1 +1,2 @@\n--- old rule\n+++ new rule\n+## Next heading\n";
        let texts: Vec<String> = changed_texts(diff)
            .into_iter()
            .map(|(f, t)| format!("{f}: {t}"))
            .collect();
        assert!(
            texts.contains(&"notes.md: Next heading".to_owned()),
            "{texts:?}"
        );
        assert!(
            texts.contains(&"notes.md: old rule".to_owned()),
            "{texts:?}"
        );
    }

    #[test]
    fn resolves_levels_from_the_least_to_the_most_specific() {
        let dir = std::env::temp_dir().join(format!("termf-team-rules-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("termf.toml"),
            "[library]\nrules = [\"+scope-strict\"]\n[rules]\nno-text-changes = \"warn\"\n",
        )
        .unwrap();
        let global = read_levels(&dir.join("termf.toml")).unwrap();
        let workflow = BTreeMap::from([("scope-strict".to_owned(), Level::Advise)]);
        let ticket = BTreeMap::from([("scope-strict".to_owned(), Level::Warn)]);
        let rules = resolve(&dir, &Scope::default(), &[&global, &workflow, &ticket]).unwrap();
        let level = |name: &str| rules.iter().find(|r| r.name == name).unwrap().level;
        assert_eq!(level("scope-strict"), Level::Warn);
        assert_eq!(
            level("no-text-changes"),
            Level::Warn,
            "switched on by its level"
        );
        assert_eq!(
            level("visual-review"),
            Level::Off,
            "shipped rules start off"
        );
        let scope = rules.iter().find(|r| r.name == "scope-strict").unwrap();
        assert_eq!(scope.check.as_deref(), Some("scope"));
        assert!(scope.applies("anyone"));
        assert!(Level::parse("loud").is_err());
        fs::write(dir.join("termf.toml"), "[rules]\nscope-strict = 3\n").unwrap();
        assert!(read_levels(&dir.join("termf.toml")).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
