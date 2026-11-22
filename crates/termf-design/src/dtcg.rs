//! Design tokens in the W3C format (DTCG, `*.tokens.json`, `design/tokens.json`): every `$value` with its path,
//! its `$type` (inherited from its groups) and a value written the way CSS would.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// `color.brand.500`.
    pub path: String,
    pub kind: Option<String>,
    pub value: String,
    pub line: u32,
}

/// The tokens of a file, or None when it has no `$value` at all (a `tokens.json` that is not design tokens).
pub fn read(text: &str) -> Option<Vec<Token>> {
    let json: Value = serde_json::from_str(text).ok()?;
    let mut out = Vec::new();
    walk(&json, &mut Vec::new(), None, text, &mut out);
    out.sort_by_key(|t| t.line);
    (!out.is_empty()).then_some(out)
}

fn walk(
    node: &Value,
    path: &mut Vec<String>,
    kind: Option<&str>,
    text: &str,
    out: &mut Vec<Token>,
) {
    let Value::Object(map) = node else {
        return;
    };
    let kind = map.get("$type").and_then(Value::as_str).or(kind);
    let mut push = |value: &Value, kind: Option<&str>| {
        if let Some(value) = css(value) {
            out.push(Token {
                path: path.join("."),
                kind: kind.map(str::to_owned),
                value,
                line: line_of(text, path),
            });
        }
    };
    if let Some(value) = map.get("$value") {
        push(value, kind);
        return;
    }
    // A group's own token (format 2025.10).
    if let Some(Value::Object(own)) = map.get("$root")
        && let Some(value) = own.get("$value")
    {
        push(value, own.get("$type").and_then(Value::as_str).or(kind));
    }
    for (key, child) in map {
        if key.starts_with('$') {
            continue;
        }
        path.push(key.clone());
        walk(child, path, kind, text, out);
        path.pop();
    }
}

/// A value as CSS writes it; an alias stays `{path}` for the caller to resolve. Composite values other than a
/// shadow (typography, border, gradient) are left out.
fn css(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().filter_map(css).collect();
            // A cubic Bézier is four numbers, a font stack a list of names.
            if items.len() == 4 && items.iter().all(Value::is_number) {
                Some(format!("cubic-bezier({})", parts.join(", ")))
            } else {
                Some(parts.join(", "))
            }
        }
        Value::Object(map) => {
            let alpha = map.get("alpha").and_then(Value::as_f64).unwrap_or(1.0);
            if let Some(hex) = map
                .get("hex")
                .and_then(Value::as_str)
                .filter(|_| alpha >= 1.0)
            {
                return Some(hex.to_owned());
            }
            if let (Some(Value::Array(c)), Some(space)) = (
                map.get("components"),
                map.get("colorSpace").and_then(Value::as_str),
            ) {
                // `none` is a missing component, which CSS reads as zero.
                let c: Vec<f64> = c.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect();
                if c.len() != 3 {
                    return None;
                }
                let a = if alpha >= 1.0 {
                    String::new()
                } else {
                    format!(" / {alpha}")
                };
                return match space {
                    "srgb" => Some(format!(
                        "rgb({} {} {}{a})",
                        (c[0] * 255.0).round(),
                        (c[1] * 255.0).round(),
                        (c[2] * 255.0).round()
                    )),
                    "oklch" | "oklab" => {
                        Some(format!("{space}({}% {} {}{a})", c[0] * 100.0, c[1], c[2]))
                    }
                    "hsl" => Some(format!("hsl({} {}% {}%{a})", c[0], c[1], c[2])),
                    _ => None,
                };
            }
            if let (Some(v), Some(unit)) =
                (map.get("value"), map.get("unit").and_then(Value::as_str))
            {
                return Some(format!("{}{unit}", css(v)?));
            }
            if map.contains_key("offsetX") {
                let part = |k: &str| map.get(k).and_then(css).unwrap_or_else(|| "0".into());
                let inset = if map.get("inset") == Some(&Value::Bool(true)) {
                    "inset "
                } else {
                    ""
                };
                return Some(format!(
                    "{inset}{} {} {} {} {}",
                    part("offsetX"),
                    part("offsetY"),
                    part("blur"),
                    part("spread"),
                    part("color")
                ));
            }
            None
        }
        _ => None,
    }
}

/// The line of a token: each name of its path searched after the previous one.
fn line_of(text: &str, path: &[String]) -> u32 {
    let mut at = 0;
    for key in path {
        let quoted = format!("\"{key}\"");
        if let Some(found) = text[at..].find(&quoted) {
            at += found + quoted.len();
        }
    }
    text[..at].matches('\n').count() as u32 + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_values_types_aliases_and_lines() {
        let text = r##"{
  "color": {
    "$type": "color",
    "brand": {
      "500": { "$value": "#4f46e5" },
      "600": { "$value": { "colorSpace": "srgb", "components": [0.26, 0.22, 0.79], "hex": "#4338ca" } }
    },
    "link": { "$value": "{color.brand.500}" }
  },
  "space": {
    "md": { "$type": "dimension", "$value": { "value": 16, "unit": "px" } }
  },
  "font": { "sans": { "$type": "fontFamily", "$value": ["Inter", "sans-serif"] } },
  "ease": { "$type": "cubicBezier", "$value": [0.2, 0, 0, 1] },
  "shadow": { "card": { "$type": "shadow", "$value": { "offsetX": "0px", "offsetY": "1px", "blur": "2px", "spread": "0px", "color": "#0000001a" } } }
}"##;
        let tokens = read(text).unwrap();
        let got: Vec<_> = tokens
            .iter()
            .map(|t| (t.path.as_str(), t.kind.as_deref(), t.value.as_str(), t.line))
            .collect();
        assert_eq!(
            got,
            [
                ("color.brand.500", Some("color"), "#4f46e5", 5),
                ("color.brand.600", Some("color"), "#4338ca", 6),
                ("color.link", Some("color"), "{color.brand.500}", 8),
                ("space.md", Some("dimension"), "16px", 11),
                ("font.sans", Some("fontFamily"), "Inter, sans-serif", 13),
                (
                    "ease",
                    Some("cubicBezier"),
                    "cubic-bezier(0.2, 0, 0, 1)",
                    14
                ),
                (
                    "shadow.card",
                    Some("shadow"),
                    "0px 1px 2px 0px #0000001a",
                    15
                ),
            ]
        );
        assert!(read(r#"{"access_token": "x"}"#).is_none());
        assert!(read("not json").is_none());
    }
}
