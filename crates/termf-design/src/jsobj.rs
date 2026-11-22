//! Reads the literal part of a JavaScript object (a Tailwind config's `theme`, a `cva` call's `variants`) without
//! running anything: strings, numbers, arrays and nested objects come out as they are; any other expression
//! (a variable, a call, a spread) is kept as its text so the caller can say it was not read.

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Object(Vec<(String, Node)>),
    Array(Vec<Node>),
    Str(String),
    Num(String),
    /// An expression that is not a literal, as written.
    Other(String),
}

impl Node {
    pub fn get(&self, key: &str) -> Option<&Node> {
        match self {
            Node::Object(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

/// The object literal that follows `key:` (or `key =`) at its first occurrence, with the line it starts on.
pub fn after_key(text: &str, key: &str) -> Option<(Node, u32)> {
    let mut from = 0;
    while let Some(at) = text[from..].find(key).map(|i| i + from) {
        from = at + key.len();
        let before = text[..at].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.') {
            continue;
        }
        let rest = text[from..].trim_start_matches(['"', '\'']).trim_start();
        let Some(rest) = rest.strip_prefix(':').or_else(|| rest.strip_prefix('=')) else {
            continue;
        };
        let start = text.len() - rest.trim_start().len();
        let line = text[..start].matches('\n').count() as u32 + 1;
        let mut p = Parser {
            s: &text[start..],
            i: 0,
        };
        return Some((p.value(0), line));
    }
    None
}

/// The value that starts `text`.
pub fn at(text: &str) -> Node {
    Parser { s: text, i: 0 }.value(0)
}

struct Parser<'a> {
    s: &'a str,
    i: usize,
}

const MAX_DEPTH: usize = 12;

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.s[self.i..].chars().next()
    }

    fn skip(&mut self) {
        loop {
            let rest = &self.s[self.i..];
            let trimmed = rest.trim_start();
            self.i += rest.len() - trimmed.len();
            if trimmed.starts_with("//") {
                self.i += trimmed.find('\n').unwrap_or(trimmed.len());
            } else if trimmed.starts_with("/*") {
                self.i += trimmed.find("*/").map_or(trimmed.len(), |e| e + 2);
            } else {
                return;
            }
        }
    }

    fn value(&mut self, depth: usize) -> Node {
        self.skip();
        match self.peek() {
            Some('{') if depth < MAX_DEPTH => self.object(depth),
            Some('[') if depth < MAX_DEPTH => self.array(depth),
            Some(q @ ('"' | '\'' | '`')) => match self.string(q) {
                Some(s) if !(q == '`' && s.contains("${")) => Node::Str(s),
                Some(s) => Node::Other(format!("`{s}`")),
                None => Node::Other(String::new()),
            },
            Some(c) if c.is_ascii_digit() || c == '-' || c == '.' => {
                let text = self.expression();
                if text.parse::<f64>().is_ok() {
                    Node::Num(text)
                } else {
                    Node::Other(text)
                }
            }
            _ => Node::Other(self.expression()),
        }
    }

    fn string(&mut self, quote: char) -> Option<String> {
        self.i += 1;
        let mut out = String::new();
        let mut escaped = false;
        for (j, c) in self.s[self.i..].char_indices() {
            if escaped {
                out.push(c);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == quote {
                self.i += j + 1;
                return Some(out);
            } else {
                out.push(c);
            }
        }
        self.i = self.s.len();
        None
    }

    /// Up to the next `,`, `}` or `]` at this level.
    fn expression(&mut self) -> String {
        let start = self.i;
        let mut depth = 0i32;
        let mut quote: Option<char> = None;
        for (j, c) in self.s[start..].char_indices() {
            if let Some(q) = quote {
                if c == q {
                    quote = None;
                }
                continue;
            }
            match c {
                '"' | '\'' | '`' => quote = Some(c),
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' if depth > 0 => depth -= 1,
                ',' | '}' | ']' | ')' if depth == 0 => {
                    self.i = start + j;
                    return self.s[start..self.i].trim().to_owned();
                }
                _ => {}
            }
        }
        self.i = self.s.len();
        self.s[start..].trim().to_owned()
    }

    fn key(&mut self) -> Option<String> {
        self.skip();
        match self.peek()? {
            q @ ('"' | '\'') => self.string(q),
            '[' => {
                // A computed key: not something the literal says.
                let text = self.expression_until(':');
                Some(text)
            }
            _ => {
                let text: String = self.s[self.i..]
                    .chars()
                    .take_while(|c| {
                        c.is_alphanumeric() || *c == '_' || *c == '$' || *c == '-' || *c == '.'
                    })
                    .collect();
                self.i += text.len();
                (!text.is_empty()).then_some(text)
            }
        }
    }

    fn expression_until(&mut self, end: char) -> String {
        let start = self.i;
        let at = self.s[start..]
            .find(end)
            .map_or(self.s.len(), |e| start + e);
        self.i = at;
        self.s[start..at].trim().to_owned()
    }

    fn object(&mut self, depth: usize) -> Node {
        self.i += 1;
        let mut entries = Vec::new();
        loop {
            self.skip();
            match self.peek() {
                None => break,
                Some('}') => {
                    self.i += 1;
                    break;
                }
                Some(',') => {
                    self.i += 1;
                    continue;
                }
                Some('.') if self.s[self.i..].starts_with("...") => {
                    self.i += 3;
                    let text = self.expression();
                    entries.push(("...".to_owned(), Node::Other(format!("...{text}"))));
                    continue;
                }
                _ => {}
            }
            let Some(key) = self.key() else {
                let text = self.expression();
                if text.is_empty() {
                    self.i += 1;
                }
                continue;
            };
            self.skip();
            match self.peek() {
                Some(':') => {
                    self.i += 1;
                    let value = self.value(depth + 1);
                    entries.push((key, value));
                }
                // Shorthand `{ colors }` or a method: not a literal.
                _ => {
                    let text = self.expression();
                    entries.push((key.clone(), Node::Other(format!("{key}{text}"))));
                }
            }
        }
        Node::Object(entries)
    }

    fn array(&mut self, depth: usize) -> Node {
        self.i += 1;
        let mut items = Vec::new();
        loop {
            self.skip();
            match self.peek() {
                None => break,
                Some(']') => {
                    self.i += 1;
                    break;
                }
                Some(',') => self.i += 1,
                _ => {
                    let before = self.i;
                    items.push(self.value(depth + 1));
                    if self.i == before {
                        self.i += 1;
                    }
                }
            }
        }
        Node::Array(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_literals_and_keeps_the_rest_as_text() {
        let text = r##"import colors from "tailwindcss/colors";
export default {
  content: ["./src/**/*.{svelte,ts}"],
  theme: {
    extend: {
      colors: {
        // brand
        brand: { DEFAULT: "#4f46e5", 600: '#4338ca' },
        gray: colors.slate,
        "accent-2": `#fff`,
        ...more,
      },
      spacing: { 18: "4.5rem", px: 1 },
      fontFamily: { sans: ["Inter", "system-ui"] },
    },
  },
  plugins: [require("@tailwindcss/forms")],
};"##;
        let (theme, line) = after_key(text, "theme").unwrap();
        assert_eq!(line, 4);
        let colors = theme.get("extend").unwrap().get("colors").unwrap();
        assert_eq!(
            colors.get("brand").unwrap().get("600"),
            Some(&Node::Str("#4338ca".into()))
        );
        assert_eq!(
            colors.get("gray"),
            Some(&Node::Other("colors.slate".into()))
        );
        assert_eq!(colors.get("accent-2"), Some(&Node::Str("#fff".into())));
        assert_eq!(colors.get("..."), Some(&Node::Other("...more".into())));
        let spacing = theme.get("extend").unwrap().get("spacing").unwrap();
        assert_eq!(spacing.get("px"), Some(&Node::Num("1".into())));
        let sans = theme
            .get("extend")
            .unwrap()
            .get("fontFamily")
            .unwrap()
            .get("sans");
        assert_eq!(
            sans,
            Some(&Node::Array(vec![
                Node::Str("Inter".into()),
                Node::Str("system-ui".into())
            ]))
        );
        assert!(after_key("const mytheme = 1", "theme").is_none());
    }
}
