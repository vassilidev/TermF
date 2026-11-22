//! Claude Code's settings schema, from a dated copy, and the part of JSON Schema its keys use.

use std::sync::OnceLock;

use serde_json::Value;

/// https://json.schemastore.org/claude-code-settings.json, copied on 2026-11-22 (served as last modified on
/// 2026-11-20). Embedded, never fetched while TermF runs: refresh it by hand and change these dates.
pub const TEXT: &str = include_str!("../schema/claude-code-settings.json");

pub fn schema() -> &'static Value {
    static SCHEMA: OnceLock<Value> = OnceLock::new();
    SCHEMA.get_or_init(|| serde_json::from_str(TEXT).expect("the embedded settings schema is JSON"))
}

fn resolve(node: &'static Value) -> &'static Value {
    match node["$ref"]
        .as_str()
        .and_then(|r| r.strip_prefix("#/$defs/"))
    {
        Some(name) => resolve(&schema()["$defs"][name]),
        None => node,
    }
}

/// The schema of a dotted key (`permissions.allow`, `env.FOO`), `None` when the schema does not know it.
pub fn at(key: &str) -> Option<&'static Value> {
    let mut node = resolve(schema());
    for part in key.split('.') {
        let next = node["properties"]
            .get(part)
            .or_else(|| Some(&node["additionalProperties"]).filter(|a| a.is_object()))?;
        node = resolve(next);
    }
    Some(node)
}

/// The schema of an item of the list at `key`.
pub fn item(key: &str) -> Option<&'static Value> {
    at(key)?.get("items").map(resolve)
}

/// Checks `value` against `node`; each error says where, as `permissions.allow[2]`.
// shortcut: `pattern`, `format` and `propertyNames` are not checked (no regex engine here), and
// `additionalProperties: false` is ignored because this community schema lags behind Claude Code; upgrade if
// a wrong key slips through.
pub fn check(node: &Value, value: &Value, at: &str, errors: &mut Vec<String>) {
    let node = match node["$ref"]
        .as_str()
        .and_then(|r| r.strip_prefix("#/$defs/"))
    {
        Some(name) => &schema()["$defs"][name],
        None => node,
    };
    let start = errors.len();
    let branches = node["anyOf"].as_array().or(node["oneOf"].as_array());
    if let Some(branches) = branches {
        // The closest form explains best what is wrong.
        let best = branches
            .iter()
            .map(|b| {
                let mut e = Vec::new();
                check(b, value, at, &mut e);
                e
            })
            .min_by_key(Vec::len)
            .unwrap_or_default();
        errors.extend(best);
    }
    if let Some(not) = node.get("not") {
        let mut e = Vec::new();
        check(not, value, at, &mut e);
        if e.is_empty() {
            errors.push(format!("{at}: this value is not accepted"));
        }
    }
    if let Some(types) = node.get("type") {
        let types: Vec<&str> = match types {
            Value::String(t) => vec![t],
            Value::Array(list) => list.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        if !types.iter().any(|t| is_type(value, t)) {
            errors.push(format!("{at}: expected {}", types.join(" or ")));
            return;
        }
    }
    if let Some(list) = node["enum"].as_array()
        && !list.contains(value)
    {
        let names: Vec<String> = list.iter().map(Value::to_string).collect();
        errors.push(format!("{at}: takes one of {}", names.join(", ")));
    }
    if let Some(c) = node.get("const")
        && c != value
    {
        errors.push(format!("{at}: must be {c}"));
    }
    if let Some(n) = value.as_f64() {
        let bound = |k: &str| node[k].as_f64();
        if bound("minimum").is_some_and(|m| n < m)
            || bound("exclusiveMinimum").is_some_and(|m| n <= m)
        {
            errors.push(format!("{at}: too small"));
        }
        if bound("maximum").is_some_and(|m| n > m)
            || bound("exclusiveMaximum").is_some_and(|m| n >= m)
        {
            errors.push(format!("{at}: too large"));
        }
    }
    if let Some(s) = value.as_str()
        && node["minLength"]
            .as_u64()
            .is_some_and(|m| (s.chars().count() as u64) < m)
    {
        errors.push(format!("{at}: must not be empty"));
    }
    if let Some(items) = value.as_array() {
        let len = items.len() as u64;
        if node["minItems"].as_u64().is_some_and(|m| len < m) {
            errors.push(format!("{at}: too few items"));
        }
        if node["maxItems"].as_u64().is_some_and(|m| len > m) {
            errors.push(format!("{at}: at most {} items", node["maxItems"]));
        }
        if node["uniqueItems"] == true
            && items
                .iter()
                .enumerate()
                .any(|(i, x)| items[..i].contains(x))
        {
            errors.push(format!("{at}: an item appears twice"));
        }
        if let Some(schema) = node.get("items").filter(|s| s.is_object()) {
            for (i, x) in items.iter().enumerate() {
                check(schema, x, &format!("{at}[{i}]"), errors);
            }
        }
    }
    if let Some(map) = value.as_object() {
        for key in node["required"].as_array().into_iter().flatten() {
            if let Some(key) = key.as_str()
                && !map.contains_key(key)
            {
                errors.push(format!("{at}: {key} is missing"));
            }
        }
        for (k, v) in map {
            let sub = node["properties"]
                .get(k)
                .or_else(|| Some(&node["additionalProperties"]).filter(|a| a.is_object()));
            if let Some(sub) = sub {
                check(sub, v, &format!("{at}.{k}"), errors);
            }
        }
    }
    // A form that does not apply says so once, not once per nested rule.
    errors.truncate(start + 3);
}

fn is_type(value: &Value, t: &str) -> bool {
    match t {
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "null" => value.is_null(),
        _ => true,
    }
}

/// The tools a permission rule may name, as the schema's rule pattern lists them.
pub fn tools() -> &'static [String] {
    static TOOLS: OnceLock<Vec<String>> = OnceLock::new();
    TOOLS.get_or_init(|| {
        // `^((Agent|Bash|…)(\([^)]+\))?|mcp__.*)$`: the first group is the list.
        let pattern = schema()["$defs"]["permissionRule"]["pattern"]
            .as_str()
            .unwrap_or_default();
        pattern
            .strip_prefix("^((")
            .and_then(|p| p.split_once(')'))
            .map(|(names, _)| names.split('|').map(str::to_owned).collect())
            .unwrap_or_default()
    })
}
