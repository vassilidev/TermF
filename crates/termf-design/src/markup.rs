//! What templates and scripts say about the design system: inline styles, Tailwind arbitrary values, fonts they
//! load, the custom properties, SCSS variables, utility classes and component tags they use.

use std::collections::HashMap;

use crate::jsobj::{self, Node};

/// Line numbers of byte offsets.
pub struct Lines(Vec<usize>);

impl Lines {
    pub fn new(text: &str) -> Lines {
        Lines(
            std::iter::once(0)
                .chain(text.match_indices('\n').map(|(i, _)| i + 1))
                .collect(),
        )
    }

    pub fn line(&self, at: usize) -> u32 {
        self.0.partition_point(|&s| s <= at) as u32
    }
}

fn is_name(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

/// Declarations of `style="…"` attributes and JSX `style={{…}}` objects: `(offset, property, value)`.
pub fn inline_styles(text: &str) -> Vec<(usize, String, String)> {
    let mut out = Vec::new();
    for (at, _) in text.match_indices("style=") {
        if at > 0 && !text.as_bytes()[at - 1].is_ascii_whitespace() {
            continue;
        }
        let rest = &text[at + 6..];
        match rest.as_bytes().first() {
            Some(&q @ (b'"' | b'\'')) => {
                let Some(end) = rest[1..].find(q as char) else {
                    continue;
                };
                for decl in rest[1..1 + end].split(';') {
                    if let Some((p, v)) = decl.split_once(':') {
                        let p = p.trim();
                        // A template expression is not a value.
                        if !p.is_empty() && !p.contains(['{', '}']) && !v.contains(['{', '}']) {
                            out.push((at, p.to_ascii_lowercase(), v.trim().to_owned()));
                        }
                    }
                }
            }
            Some(b'{') if rest[1..].trim_start().starts_with('{') => {
                let inner = rest[1..].trim_start();
                let Node::Object(entries) = jsobj::at(inner) else {
                    continue;
                };
                for (key, value) in entries {
                    let prop = kebab(&key);
                    let value = match value {
                        Node::Str(s) => s,
                        Node::Num(n) if !UNITLESS.contains(&prop.as_str()) && n != "0" => {
                            format!("{n}px")
                        }
                        _ => continue,
                    };
                    out.push((at, prop, value));
                }
            }
            _ => {}
        }
    }
    out
}

const UNITLESS: [&str; 8] = [
    "line-height",
    "font-weight",
    "opacity",
    "z-index",
    "flex",
    "flex-grow",
    "flex-shrink",
    "order",
];

fn kebab(camel: &str) -> String {
    if camel.starts_with("--") {
        return camel.to_owned();
    }
    let mut out = String::new();
    for c in camel.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Tailwind arbitrary values (`bg-[#1a2b3c]`, `p-[13px]`): `(offset, utility, value)`; a value that uses a
/// custom property is a token's, so it is left out.
pub fn arbitrary(text: &str) -> Vec<(usize, String, String)> {
    let mut out = Vec::new();
    for (at, _) in text.match_indices("-[") {
        let start = text[..at]
            .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .map_or(0, |i| i + 1);
        let utility = &text[start..at];
        let Some(end) = text[at + 2..].find(']') else {
            continue;
        };
        let value = &text[at + 2..at + 2 + end];
        if utility.is_empty()
            || value.is_empty()
            || value.contains(['\n', ' ', '"', '\''])
            || value.contains("var(")
            || value.starts_with("--")
        {
            continue;
        }
        out.push((at, utility.to_owned(), value.replace('_', " ")));
    }
    out
}

/// Fonts a page or a script loads: Google Fonts URLs, `@fontsource` packages and `next/font/google` imports.
pub fn fonts(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (at, _) in text.match_indices("fonts.googleapis.com/css") {
        let end = text[at..]
            .find(['"', '\'', ')', ' ', '\n', '`'])
            .map_or(text.len(), |e| at + e);
        let url = &text[at..end];
        for param in url
            .split(['?', '&'])
            .filter_map(|p| p.strip_prefix("family="))
        {
            for family in param.split('|') {
                let name = family
                    .split(':')
                    .next()
                    .unwrap_or("")
                    .replace(['+'], " ")
                    .replace("%20", " ");
                if !name.trim().is_empty() {
                    out.push((at, name.trim().to_owned()));
                }
            }
        }
    }
    for prefix in ["@fontsource/", "@fontsource-variable/"] {
        for (at, _) in text.match_indices(prefix) {
            let name: String = text[at + prefix.len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            if !name.is_empty() {
                out.push((at, title(&name.replace('-', " "))));
            }
        }
    }
    for (at, _) in text.match_indices("next/font/google") {
        let Some(open) = text[..at].rfind("import") else {
            continue;
        };
        let names = &text[open..at];
        if let (Some(a), Some(b)) = (names.find('{'), names.find('}')) {
            for name in names[a + 1..b].split(',') {
                let name = name.split(" as ").next().unwrap_or("").trim();
                if !name.is_empty() {
                    out.push((open, name.replace('_', " ")));
                }
            }
        }
    }
    out
}

/// Stylesheets a script imports (`import "@radix-ui/themes/styles.css"`), as written.
pub fn css_imports(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for quote in ['"', '\''] {
        for (at, _) in text.match_indices(&format!(".css{quote}")) {
            let Some(start) = text[..at].rfind(quote) else {
                continue;
            };
            let before = text[..start].trim_end();
            if before.ends_with("import")
                || before.ends_with("from")
                || before.ends_with("require(")
            {
                out.push(text[start + 1..at + 4].to_owned());
            }
        }
    }
    out
}

fn title(text: &str) -> String {
    text.split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Every `--name` of a text: `f(name, offset, defined_here, in_var, with_fallback)`.
pub fn custom_props(text: &str, mut f: impl FnMut(&str, usize, bool, bool, bool)) {
    let bytes = text.as_bytes();
    for (at, _) in text.match_indices("--") {
        if at > 0 && (is_name(bytes[at - 1]) || bytes[at - 1] == b'!' || bytes[at - 1] == b'<') {
            continue;
        }
        let len = bytes[at + 2..].iter().take_while(|&&b| is_name(b)).count();
        if len == 0 || bytes[at + 2] == b'-' {
            continue;
        }
        let name = &text[at..at + 2 + len];
        let after = text[at + 2 + len..].trim_start_matches(['"', '\'', '`', ' ', '\t']);
        let before = text[..at].trim_end();
        let defined = after.starts_with(':') && !after.starts_with("::")
            || after.starts_with('=')
            || before.ends_with("setProperty(\"")
            || before.ends_with("setProperty('")
            || before.ends_with("setProperty(`");
        let in_var = before.ends_with("var(");
        let fallback = in_var && text[at + 2 + len..].trim_start().starts_with(',');
        f(name, at, defined, in_var, fallback);
    }
}

/// How many times each `$variable` is used (its definitions aside).
pub fn scss_uses(text: &str, uses: &mut HashMap<String, usize>) {
    let bytes = text.as_bytes();
    for (at, _) in text.match_indices('$') {
        let len = bytes[at + 1..].iter().take_while(|&&b| is_name(b)).count();
        if len == 0 || at > 0 && is_name(bytes[at - 1]) {
            continue;
        }
        let after = text[at + 1 + len..].trim_start();
        if after.starts_with(':') {
            continue;
        }
        *uses.entry(text[at..at + 1 + len].to_owned()).or_default() += 1;
    }
}

/// Each utility class's possible token names: `hover:bg-brand-500/50` counts `brand-500` and `500`, so a theme
/// token `--color-brand-500` (or `colors.brand.500`) is seen used.
pub fn classes(text: &str, suffixes: &mut HashMap<String, usize>) {
    for word in text.split(|c: char| c.is_whitespace() || "'\"`{}()<>,;=".contains(c)) {
        if word.len() > 80 || !word.contains('-') {
            continue;
        }
        let word = word.rsplit(':').next().unwrap_or(word);
        let word = word.trim_start_matches(['!', '-', '.']);
        let word = word.split('/').next().unwrap_or(word);
        for (i, _) in word.match_indices('-') {
            let suffix = &word[i + 1..];
            if !suffix.is_empty() {
                *suffixes.entry(suffix.to_owned()).or_default() += 1;
            }
        }
    }
}

/// Component tags: `<Button`, `<ui.Card`, and Vue's `<my-button` as `MyButton`, with their offsets.
pub fn tags(text: &str) -> Vec<(usize, String)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    for (at, _) in text.match_indices('<') {
        let len = bytes[at + 1..]
            .iter()
            .take_while(|&&b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
            .count();
        if len == 0 {
            continue;
        }
        let raw = &text[at + 1..at + 1 + len];
        let name = raw.rsplit('.').next().unwrap_or(raw);
        if name.starts_with(|c: char| c.is_ascii_uppercase()) {
            out.push((at, name.to_owned()));
        } else if name.contains('-') && name.starts_with(|c: char| c.is_ascii_lowercase()) {
            out.push((at, pascal(name)));
        }
    }
    out
}

/// The markup's element sequence, for spotting two components built the same way.
pub fn skeleton(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    text.match_indices('<')
        .filter_map(|(at, _)| {
            let len = bytes[at + 1..]
                .iter()
                .take_while(|&&b| b.is_ascii_alphanumeric() || b == b'-')
                .count();
            (len > 0 && bytes[at + 1].is_ascii_alphabetic())
                .then(|| text[at + 1..at + 1 + len].to_ascii_lowercase())
        })
        .filter(|t| !["script", "style", "template", "slot"].contains(&t.as_str()))
        .collect()
}

pub fn pascal(text: &str) -> String {
    text.split(['-', '_', ' ', '.'])
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_inline_styles_of_html_and_jsx() {
        let text = r#"<div style="color: #333; padding:12px"></div>
<p style={{ backgroundColor: '#fff', marginTop: 8, lineHeight: 1.5, color: theme.fg }} />
<i style="color: {dynamic}" />"#;
        let got: Vec<_> = inline_styles(text)
            .into_iter()
            .map(|(_, p, v)| (p, v))
            .collect();
        let want = [
            ("color", "#333"),
            ("padding", "12px"),
            ("background-color", "#fff"),
            ("margin-top", "8px"),
        ];
        assert_eq!(got, want.map(|(p, v)| (p.to_owned(), v.to_owned())));
    }

    #[test]
    fn reads_arbitrary_values_fonts_and_uses() {
        let text = r#"<a class="bg-[#1a2b3c] hover:text-[15px] p-[var(--x)] w-[calc(100%_-_2px)] grid-cols-[1fr_2fr]">
@import url("https://fonts.googleapis.com/css2?family=Inter:wght@400;700&family=Roboto+Mono&display=swap");
import "@fontsource-variable/jetbrains-mono";
import { Inter, Roboto_Mono as Mono } from "next/font/google";"#;
        let arb: Vec<_> = arbitrary(text)
            .into_iter()
            .map(|(_, u, v)| (u, v))
            .collect();
        assert_eq!(
            arb,
            [
                ("bg".to_owned(), "#1a2b3c".to_owned()),
                ("text".to_owned(), "15px".to_owned()),
                ("w".to_owned(), "calc(100% - 2px)".to_owned()),
                ("grid-cols".to_owned(), "1fr 2fr".to_owned()),
            ]
        );
        let fonts: Vec<_> = fonts(text).into_iter().map(|(_, f)| f).collect();
        assert_eq!(
            fonts,
            [
                "Inter",
                "Roboto Mono",
                "Jetbrains Mono",
                "Inter",
                "Roboto Mono"
            ]
        );
    }

    #[test]
    fn tells_definitions_from_uses_of_custom_properties() {
        let text = "a { --x: 1; color: var(--y); margin: var(--z, 4px) } <!-- c --> i--; el.style.setProperty('--w', 1); <B --v=\"2\" style:--u={a}/> `--flag`";
        let mut seen = Vec::new();
        custom_props(text, |n, _, d, v, f| seen.push((n.to_owned(), d, v, f)));
        let want = [
            ("--x", true, false, false),
            ("--y", false, true, false),
            ("--z", false, true, true),
            ("--w", true, false, false),
            ("--v", true, false, false),
            ("--u", true, false, false),
            ("--flag", false, false, false),
        ];
        assert_eq!(seen, want.map(|(n, d, v, f)| (n.to_owned(), d, v, f)));
    }

    #[test]
    fn counts_classes_scss_variables_and_tags() {
        let mut s = HashMap::new();
        classes(
            r#"<div class="hover:bg-brand-500/50 font-sans !mt-gutter">"#,
            &mut s,
        );
        for k in ["brand-500", "500", "sans", "gutter"] {
            assert_eq!(s.get(k), Some(&1), "{k}");
        }
        let mut u = HashMap::new();
        scss_uses("$a: 1px; .b { margin: $a $a; padding: $b-c; }", &mut u);
        assert_eq!(u.get("$a"), Some(&2));
        assert_eq!(u.get("$b-c"), Some(&1));
        let t: Vec<_> = tags("<Button><ui.Card/><my-button></my-button><div>")
            .into_iter()
            .map(|(_, n)| n)
            .collect();
        assert_eq!(t, ["Button", "Card", "MyButton"]);
        assert_eq!(
            skeleton("<script>x</script><div><p>a</p><Icon/></div>"),
            ["div", "p", "icon"]
        );
    }
}
