//! Hides what looks like a secret before anything is shown, copied or exported.

use serde_json::Value;

pub const MASK: &str = "••••••";

const SECRET_WORDS: &[&str] = &[
    "secret",
    "secrets",
    "password",
    "passwd",
    "pwd",
    "token",
    "apikey",
    "credential",
    "credentials",
    "cookie",
    "authorization",
    "auth",
    "bearer",
    "privatekey",
];
const SECRET_PAIRS: &[[&str; 2]] = &[
    ["api", "key"],
    ["access", "key"],
    ["private", "key"],
    ["client", "secret"],
    ["session", "key"],
    ["signing", "key"],
];
/// A key that names how a secret is obtained or used, not the secret: `apiKeyHelper`, `authMethod`.
const NOT_A_SECRET: &[&str] = &[
    "helper", "refresh", "export", "method", "type", "enabled", "disabled", "url", "path", "dir",
    "file", "provider",
];
const TOKEN_PREFIXES: &[&str] = &[
    "sk-",
    "sk_",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "glpat-",
    "xoxb-",
    "xoxp-",
    "xapp-",
    "akia",
    "asia",
    "aiza",
    "ya29.",
    "pk_live_",
    "rk_live_",
    "npm_",
    "pypi-",
    "hf_",
    "dop_v1_",
];

/// Lower-case words of a key: `ANTHROPIC_API_KEY`, `apiKeyHelper` and `x-api-key` all split.
fn words(key: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut prev_lower = false;
    for c in key.chars() {
        if !c.is_alphanumeric() {
            words.push(std::mem::take(&mut word));
            prev_lower = false;
            continue;
        }
        if c.is_uppercase() && prev_lower {
            words.push(std::mem::take(&mut word));
        }
        prev_lower = c.is_lowercase() || c.is_ascii_digit();
        word.extend(c.to_lowercase());
    }
    words.push(word);
    words.retain(|w| !w.is_empty());
    words
}

pub fn secret_key(key: &str) -> bool {
    let words = words(key);
    let Some(last) = words.last() else {
        return false;
    };
    if NOT_A_SECRET.contains(&last.as_str()) {
        return false;
    }
    words.iter().any(|w| SECRET_WORDS.contains(&w.as_str()))
        || words.windows(2).any(|pair| {
            SECRET_PAIRS
                .iter()
                .any(|p| pair[0] == p[0] && pair[1] == p[1])
        })
}

pub fn secret_value(value: &str) -> bool {
    let v = value.trim();
    let lower = v.to_ascii_lowercase();
    if ["bearer ", "basic ", "token "]
        .iter()
        .any(|p| lower.starts_with(p))
    {
        return true;
    }
    if v.len() >= 16 && TOKEN_PREFIXES.iter().any(|p| lower.starts_with(p)) {
        return true;
    }
    if v.starts_with("eyJ") && v.split('.').count() == 3 && v.len() > 30 {
        return true;
    }
    // A long run of random-looking characters: a key, not a word or a path.
    v.len() >= 32
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || "+=_-.".contains(c))
        && v.chars().any(|c| c.is_ascii_digit())
        && v.chars().any(|c| c.is_ascii_alphabetic())
}

/// Masks the secrets inside free text, such as a command line, a URL or a header.
pub fn mask_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    // The word before was `Bearer`, or a secret key whose value follows: `"GITHUB_TOKEN": "…"`.
    let mut mask_next = false;
    for piece in text.split_inclusive(char::is_whitespace) {
        let word = piece.trim_end_matches(char::is_whitespace);
        let space = &piece[word.len()..];
        let core = word.trim_matches(|c| "\"'`,;()[]{}".contains(c));
        let start = word.find(core).unwrap_or(0);
        let (before, after) = (&word[..start], &word[start + core.len()..]);
        let scheme = ["bearer", "basic", "token"].contains(&core.to_ascii_lowercase().as_str());
        let masked = if mask_next && !core.is_empty() && !scheme {
            MASK.to_owned()
        } else {
            mask_word(core)
        };
        if !core.is_empty() {
            mask_next = scheme
                || (core.ends_with([':', '='])
                    && secret_key(core.trim_end_matches([':', '=', '"'])));
        }
        out.push_str(before);
        out.push_str(&masked);
        out.push_str(after);
        out.push_str(space);
    }
    out
}

fn mask_word(word: &str) -> String {
    if word.contains("://") {
        return mask_url(word);
    }
    // `KEY=value`, `--api-key=value`, `Header:value`.
    if let Some(i) = word.find(['=', ':'])
        && i > 0
    {
        let (key, value) = (&word[..i], &word[i + 1..]);
        if !value.is_empty() && (secret_key(key) || secret_value(value)) {
            return format!("{key}{}{MASK}", &word[i..=i]);
        }
        return word.to_owned();
    }
    if secret_value(word) {
        MASK.to_owned()
    } else {
        word.to_owned()
    }
}

/// Hides the password of `scheme://user:password@host` and the secret query parameters.
fn mask_url(url: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap_or(("", url));
    let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(host_end);
    let authority = match authority.rsplit_once('@') {
        Some((user, host)) => match user.split_once(':') {
            Some((name, _)) => format!("{name}:{MASK}@{host}"),
            None => format!("{user}@{host}"),
        },
        None => authority.to_owned(),
    };
    let tail = match tail.split_once('?') {
        Some((path, query)) => {
            let query: Vec<String> = query
                .split('&')
                .map(|param| match param.split_once('=') {
                    Some((k, v)) if secret_key(k) || secret_value(v) => format!("{k}={MASK}"),
                    _ => param.to_owned(),
                })
                .collect();
            format!("{path}?{}", query.join("&"))
        }
        None => tail.to_owned(),
    };
    format!("{scheme}://{authority}{tail}")
}

/// Masks a JSON value in place: whole values under a secret-looking key, and secrets inside every string.
pub fn mask(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, v) in map.iter_mut() {
                if secret_key(key) && matches!(v, Value::String(_) | Value::Number(_)) {
                    *v = MASK.into();
                } else {
                    mask(v);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(mask),
        Value::String(s) => *s = mask_text(s),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tells_secret_keys() {
        for key in [
            "ANTHROPIC_API_KEY",
            "GITHUB_TOKEN",
            "Authorization",
            "x-api-key",
            "clientSecret",
            "DB_PASSWORD",
            "AWS_SECRET_ACCESS_KEY",
            "auth_token",
        ] {
            assert!(secret_key(key), "{key}");
        }
        for key in [
            "MAX_THINKING_TOKENS",
            "apiKeyHelper",
            "authMethod",
            "author",
            "model",
            "NODE_ENV",
            "awsAuthRefresh",
        ] {
            assert!(!secret_key(key), "{key}");
        }
    }

    #[test]
    fn masks_secrets_in_text() {
        assert_eq!(
            mask_text("curl -H \"Authorization: Bearer abc123\" https://x.test"),
            format!("curl -H \"Authorization: Bearer {MASK}\" https://x.test")
        );
        assert_eq!(
            mask_text("API_KEY=hunter2 npx srv --verbose"),
            format!("API_KEY={MASK} npx srv --verbose")
        );
        assert_eq!(
            mask_text("npx srv --api-key=s3cr3t"),
            format!("npx srv --api-key={MASK}")
        );
        assert_eq!(
            mask_text("token ghp_0123456789abcdefghij0123456789abcd"),
            format!("token {MASK}")
        );
        assert_eq!(
            mask_text("https://me:pw@host.test/mcp?token=abc&x=1"),
            format!("https://me:{MASK}@host.test/mcp?token={MASK}&x=1")
        );
        assert_eq!(
            mask_text("  \"GITHUB_TOKEN\": \"abc\",\n  \"MAX_THINKING_TOKENS\": \"8000\""),
            format!("  \"GITHUB_TOKEN\": \"{MASK}\",\n  \"MAX_THINKING_TOKENS\": \"8000\"")
        );
        let plain = "/Users/me/.claude/statusline.sh --short";
        assert_eq!(mask_text(plain), plain);
        assert_eq!(mask_text("claude-opus-4-8"), "claude-opus-4-8");
    }

    #[test]
    fn masks_json_values() {
        let mut v = json!({
            "env": {"OPENAI_API_KEY": "sk-proj-0123456789abcdef", "NODE_ENV": "production", "MAX_THINKING_TOKENS": "8000"},
            "headers": {"Authorization": "Bearer xyz"},
            "apiKeyHelper": "~/bin/key.sh",
            "args": ["--token", "ghp_0123456789abcdefghij0123456789abcd"],
        });
        mask(&mut v);
        assert_eq!(v["env"]["OPENAI_API_KEY"], MASK);
        assert_eq!(v["env"]["NODE_ENV"], "production");
        assert_eq!(v["env"]["MAX_THINKING_TOKENS"], "8000");
        assert_eq!(v["headers"]["Authorization"], MASK);
        assert_eq!(v["apiKeyHelper"], "~/bin/key.sh");
        assert_eq!(v["args"][1], MASK);
    }
}
