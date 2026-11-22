//! A tolerant reader of stylesheets (CSS, SCSS, Less, and the `<style>` of components): custom properties with
//! the theme and scope they are defined in, the other declarations with their rule, SCSS variables, `@font-face`
//! families and imports. It never fails: what it cannot make sense of is skipped.

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
}

#[derive(Debug, Clone)]
pub struct VarDef {
    pub name: String,
    pub value: String,
    pub line: u32,
    pub theme: Theme,
    /// On `:root`, `html`, `:host`, a theme selector or Tailwind's `@theme`: a token, not a component's own value.
    pub global: bool,
    /// In Tailwind's `@theme`.
    pub tailwind: bool,
}

#[derive(Debug, Clone)]
pub struct Decl {
    pub prop: String,
    pub value: String,
    pub line: u32,
    /// The rule it belongs to, so a color and a background of the same rule make a pair.
    pub rule: usize,
}

#[derive(Debug, Default)]
pub struct Sheet {
    pub vars: Vec<VarDef>,
    pub decls: Vec<Decl>,
    pub scss: Vec<(String, String, u32)>,
    pub faces: Vec<(String, u32)>,
    pub imports: Vec<(String, u32)>,
    /// Imports Tailwind (`@import "tailwindcss"`, `@tailwind`).
    pub tailwind: bool,
}

/// Rule ids are unique across every sheet of a scan, `next` carrying on from one sheet to the next.
pub fn parse(text: &str, scss: bool, first_line: u32, next: &mut usize) -> Sheet {
    let clean = strip_comments(text, scss);
    let starts: Vec<usize> = std::iter::once(0)
        .chain(clean.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let line_of = |at: usize| first_line + starts.partition_point(|&s| s <= at) as u32 - 1;
    let mut sheet = Sheet::default();
    let mut stack: Vec<(String, usize)> = Vec::new();
    let mut from = 0;
    let mut quote: Option<char> = None;
    let mut parens = 0usize;
    let mut prev = '\0';
    for (i, c) in clean.char_indices() {
        if let Some(q) = quote {
            if c == q && prev != '\\' {
                quote = None;
            }
            prev = c;
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' => parens += 1,
            ')' => parens = parens.saturating_sub(1),
            // SCSS interpolation `#{…}` is part of a selector or value, not a block.
            '{' if prev == '#' => parens += 1,
            '{' if parens == 0 => {
                let prelude = clean[from..i].trim().to_owned();
                *next += 1;
                stack.push((prelude, *next));
                from = i + 1;
            }
            '}' if parens > 0 => parens -= 1,
            ';' | '}' if parens == 0 => {
                statement(&clean[from..i], from, &stack, scss, &line_of, &mut sheet);
                if c == '}' {
                    stack.pop();
                }
                from = i + 1;
            }
            _ => {}
        }
        prev = c;
    }
    sheet
}

fn statement(
    text: &str,
    at: usize,
    stack: &[(String, usize)],
    scss: bool,
    line_of: &dyn Fn(usize) -> u32,
    sheet: &mut Sheet,
) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    let line = line_of(at + (text.len() - text.trim_start().len()));
    if let Some(rule) = trimmed.strip_prefix('@') {
        let (name, rest) = rule.split_once(char::is_whitespace).unwrap_or((rule, ""));
        match name {
            "import" | "use" | "forward" => {
                let target = rest
                    .trim()
                    .trim_start_matches("url(")
                    .split(['"', '\''])
                    .find(|s| !s.trim().is_empty() && !s.trim().starts_with(')'))
                    .unwrap_or("")
                    .trim()
                    .trim_end_matches(')')
                    .to_owned();
                sheet.tailwind |= target == "tailwindcss" || target.starts_with("tailwindcss/");
                sheet.imports.push((target, line));
            }
            "tailwind" => sheet.tailwind = true,
            _ => {}
        }
        return;
    }
    let Some((prop, value)) = trimmed.split_once(':') else {
        return;
    };
    let prop = prop.trim();
    let value = value.trim().to_owned();
    if scss && let Some(name) = prop.strip_prefix('$') {
        let value = value
            .trim_end_matches("!default")
            .trim_end_matches("!global")
            .trim()
            .to_owned();
        sheet.scss.push((format!("${name}"), value, line));
        return;
    }
    if prop.is_empty() || prop.contains(char::is_whitespace) && !prop.starts_with("--") {
        // A selector with a pseudo-class (`a:hover`) left without a block, or SCSS nesting: not a declaration.
        return;
    }
    let Some((top, rule)) = stack.last() else {
        return;
    };
    if top.starts_with("@font-face") {
        if prop.eq_ignore_ascii_case("font-family") {
            sheet.faces.push((unquote(&value), line));
        }
        return;
    }
    if prop.starts_with("--") {
        let tailwind = stack.iter().any(|(p, _)| p.starts_with("@theme"));
        sheet.vars.push(VarDef {
            name: prop.to_owned(),
            value,
            line,
            theme: if stack.iter().any(|(p, _)| dark(p)) {
                Theme::Dark
            } else {
                Theme::Light
            },
            global: tailwind || innermost_rule(stack).is_some_and(rootish),
            tailwind,
        });
        return;
    }
    sheet.decls.push(Decl {
        prop: prop.to_ascii_lowercase(),
        value,
        line,
        rule: *rule,
    });
}

/// The selector of the innermost style rule, at-rules (`@media`, `@layer`) aside.
fn innermost_rule(stack: &[(String, usize)]) -> Option<&str> {
    stack
        .iter()
        .rev()
        .map(|(p, _)| p.as_str())
        .find(|p| !p.starts_with('@'))
}

/// `:not(…)` says what the rule is not, so it never makes a selector dark.
fn without_not(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(":not(") {
        out.push_str(&rest[..at]);
        let mut depth = 0;
        let mut end = rest.len();
        for (i, c) in rest[at + 4..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = at + 4 + i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn dark(prelude: &str) -> bool {
    let p = without_not(&prelude.to_ascii_lowercase());
    p.match_indices("dark").any(|(i, _)| {
        let before = p[..i].chars().next_back();
        let after = p[i + 4..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

fn rootish(selector: &str) -> bool {
    selector.split(',').all(|part| {
        let p = part.trim().trim_start_matches('&').to_ascii_lowercase();
        [
            ":root",
            "html",
            ":host",
            "body",
            ".dark",
            ".light",
            ".theme-",
            "[data-theme",
            "[data-mode",
            "[data-color-scheme",
            "[data-color-mode",
            "[class~=",
        ]
        .iter()
        .any(|s| p.starts_with(s))
    })
}

pub fn unquote(text: &str) -> String {
    text.trim().trim_matches(['"', '\'']).trim().to_owned()
}

/// Comments become spaces, newlines kept, so positions and lines stay right. `//` comments only for SCSS and
/// Less, and only where they start a line or follow a space (never inside `url(http://…)`).
fn strip_comments(text: &str, line_comments: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();
    let mut quote: Option<char> = None;
    let mut prev = '\n';
    while let Some((_, c)) = chars.next() {
        if let Some(q) = quote {
            if c == q && prev != '\\' {
                quote = None;
            }
            out.push(c);
            prev = c;
            continue;
        }
        let next = chars.peek().map(|&(_, n)| n);
        if c == '/' && next == Some('*') {
            chars.next();
            out.push_str("  ");
            let mut last = '\0';
            for (_, c) in chars.by_ref() {
                out.push(if c == '\n' { '\n' } else { ' ' });
                if last == '*' && c == '/' {
                    break;
                }
                last = c;
            }
            prev = ' ';
            continue;
        }
        if line_comments && c == '/' && next == Some('/') && prev.is_whitespace() {
            out.push(' ');
            for (_, c) in chars.by_ref() {
                if c == '\n' {
                    out.push('\n');
                    break;
                }
                out.push(' ');
            }
            prev = '\n';
            continue;
        }
        if c == '"' || c == '\'' {
            quote = Some(c);
        }
        out.push(c);
        prev = c;
    }
    out
}

/// The `<style>` blocks of a component or page: their text, the line each starts on, and whether it is SCSS
/// or Less. Indented Sass (`lang="sass"`) is left out.
pub fn style_blocks(text: &str) -> Vec<(&str, u32, bool)> {
    let lower = text.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(open) = lower[from..].find("<style").map(|i| i + from) {
        let Some(tag_end) = lower[open..].find('>').map(|i| i + open + 1) else {
            break;
        };
        let Some(close) = lower[tag_end..].find("</style").map(|i| i + tag_end) else {
            break;
        };
        let tag = &lower[open..tag_end];
        if !tag.contains("sass\"") && !tag.contains("sass'") {
            let line = text[..tag_end].matches('\n').count() as u32 + 1;
            out.push((
                &text[tag_end..close],
                line,
                tag.contains("scss") || tag.contains("less"),
            ));
        }
        from = close;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(text: &str) -> Sheet {
        parse(text, false, 1, &mut 0)
    }

    #[test]
    fn reads_tokens_with_their_theme_and_scope() {
        let s = sheet(
            r#"@import "tailwindcss";
/* --commented: #000; */
:root {
  --bg: #fff;
  --text: #111; --link: var(--accent);
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) { --bg: #000; }
}
[data-theme="dark"] { --text: #eee }
:root:not(.dark) { --only-light: red; }
@theme inline {
  --color-brand: oklch(0.6 0.2 250);
}
.button { --pad: 4px; color: var(--text); background: url("data:image/png;base64,AA;B"); }
@font-face { font-family: "Acme Sans"; src: url(a.woff2); }
"#,
        );
        assert!(s.tailwind);
        let vars: Vec<_> = s
            .vars
            .iter()
            .map(|v| (v.name.as_str(), v.value.as_str(), v.line, v.theme, v.global))
            .collect();
        assert_eq!(
            vars,
            [
                ("--bg", "#fff", 4, Theme::Light, true),
                ("--text", "#111", 5, Theme::Light, true),
                ("--link", "var(--accent)", 5, Theme::Light, true),
                ("--bg", "#000", 8, Theme::Dark, true),
                ("--text", "#eee", 10, Theme::Dark, true),
                ("--only-light", "red", 11, Theme::Light, true),
                (
                    "--color-brand",
                    "oklch(0.6 0.2 250)",
                    13,
                    Theme::Light,
                    true
                ),
                ("--pad", "4px", 15, Theme::Light, false),
            ]
        );
        assert!(s.vars[6].tailwind);
        let decls: Vec<_> = s.decls.iter().map(|d| (d.prop.as_str(), d.line)).collect();
        assert_eq!(decls, [("color", 15), ("background", 15)]);
        assert_eq!(s.decls[1].value, r#"url("data:image/png;base64,AA;B")"#);
        assert_eq!(s.faces, [("Acme Sans".to_owned(), 16)]);
    }

    #[test]
    fn reads_scss_variables_and_nesting() {
        let s = parse(
            "// $commented: 1px;\n$primary: #3b82f6 !default;\n$space: 8px;\n.card {\n  padding: $space; // trailing\n  &:hover { color: darken($primary, 5%); }\n  .title#{$x} { margin: 0 }\n}\n",
            true,
            10,
            &mut 0,
        );
        assert_eq!(
            s.scss,
            [
                ("$primary".to_owned(), "#3b82f6".to_owned(), 11),
                ("$space".to_owned(), "8px".to_owned(), 12)
            ]
        );
        let decls: Vec<_> = s
            .decls
            .iter()
            .map(|d| (d.prop.as_str(), d.value.as_str(), d.line))
            .collect();
        assert_eq!(
            decls,
            [
                ("padding", "$space", 14),
                ("color", "darken($primary, 5%)", 15),
                ("margin", "0", 16)
            ]
        );
    }

    #[test]
    fn finds_style_blocks_of_components() {
        let text = "<script>\nlet a = 1;\n</script>\n<div />\n<style lang=\"scss\">\n.a { color: red }\n</style>\n<style lang=\"sass\">\n.b\n  color: red\n</style>";
        let blocks = style_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].1, 5);
        assert!(blocks[0].2);
        let s = parse(blocks[0].0, true, blocks[0].1, &mut 0);
        assert_eq!(s.decls[0].line, 6);
    }
}
