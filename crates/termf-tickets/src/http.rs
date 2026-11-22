//! A tracker's JSON API over `curl`, for Linear, GitLab and Azure DevOps: no HTTP or TLS stack built into TermF,
//! the system's certificates and proxy, and the token handed to curl on its standard input, never on a command
//! line. Reads keep their last answer: asked again with its ETag when the tracker gave one, else given back for a
//! minute unless fresh.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::Value;

/// A read → its last answer.
pub(crate) type Answers = Arc<Mutex<HashMap<String, Answer>>>;

#[derive(Clone)]
pub(crate) struct Answer {
    etag: Option<String>,
    at: Instant,
    value: Value,
}

/// How long an answer without ETag stands for the reads that are not fresh.
const KEPT_FOR: Duration = Duration::from_secs(60);

/// A local `http://127.0.0.1:<port>` stays as it is, for tests: nowhere else does a token travel unencrypted.
pub(crate) fn local(site: &str) -> bool {
    ["http://127.0.0.1:", "http://localhost:"]
        .iter()
        .any(|l| site.starts_with(l))
}

/// `acme.example.com` or its URL, as a host: letters, digits, `-` and `.` only.
pub(crate) fn host(site: &str, what: &str, example: &str) -> Result<String> {
    let site = site.trim().trim_end_matches('/');
    let host = site.strip_prefix("https://").unwrap_or(site);
    anyhow::ensure!(
        !host.is_empty()
            && host.contains('.')
            && host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-.".contains(&b)),
        "{site:?} is not a {what}: write it as {example}"
    );
    Ok(host.to_ascii_lowercase())
}

/// A path segment, percent-encoded: `acme/api` → `acme%2Fapi`.
pub(crate) fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub(crate) struct Http {
    /// `https://gitlab.com/api/v4`.
    pub base: String,
    /// Who answers, in errors: `Linear`, `GitLab`.
    pub name: &'static str,
    /// curl options that carry the token, such as `("header", "Authorization: …")` or `("user", ":…")`.
    pub auth: Vec<(&'static str, String)>,
    /// What to do when the token is refused.
    pub refused: String,
    pub curl: PathBuf,
    pub answers: Answers,
}

/// A request's body and its type.
pub(crate) struct Body<'a> {
    pub kind: &'a str,
    pub value: &'a Value,
}

impl<'a> Body<'a> {
    pub fn json(value: &'a Value) -> Self {
        Self {
            kind: "application/json",
            value,
        }
    }
}

/// What the tracker says went wrong, from its error body.
fn reason(body: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return crate::truncate(body.trim(), 300);
    };
    let mut reasons: Vec<String> = Vec::new();
    for key in ["message", "error", "error_description"] {
        match &value[key] {
            Value::String(text) => reasons.push(text.clone()),
            Value::Null => {}
            other => reasons.push(other.to_string()),
        }
    }
    let errors = value["errors"].as_array().into_iter().flatten();
    reasons.extend(errors.filter_map(|e| e["message"].as_str().map(str::to_owned)));
    if reasons.is_empty() {
        crate::truncate(body.trim(), 300)
    } else {
        reasons.join("; ")
    }
}

impl Http {
    /// A write, or a read that must not come from the last answer.
    pub fn call(&self, method: &str, path: &str, body: Option<Body>) -> Result<Value> {
        Ok(self.send(method, path, body.as_ref(), None)?.1)
    }

    /// A read. Unless `fresh`, the last answer comes back when the tracker says it did not change (its ETag), or
    /// when it is less than a minute old and the tracker gives no ETag.
    pub fn read(&self, method: &str, path: &str, body: Option<Body>, fresh: bool) -> Result<Value> {
        let key = format!(
            "{method} {}{path} {}",
            self.base,
            body.as_ref()
                .map(|b| b.value.to_string())
                .unwrap_or_default()
        );
        let kept = self.answers.lock().unwrap().get(&key).cloned();
        let kept = kept.filter(|_| !fresh);
        if let Some(kept) = &kept
            && kept.etag.is_none()
            && kept.at.elapsed() < KEPT_FOR
        {
            return Ok(kept.value.clone());
        }
        let etag = kept.as_ref().and_then(|k| k.etag.as_deref());
        let (etag, value) = match self.send(method, path, body.as_ref(), etag)? {
            (304, _, _) => match kept {
                Some(kept) => (kept.etag, kept.value),
                None => bail!("{} answered 304 to a request it never answered", self.name),
            },
            (_, value, etag) => (etag, value),
        };
        let mut answers = self.answers.lock().unwrap();
        // shortcut: forgets every answer past 256 reads; keep the most used ones if that ever shows.
        if answers.len() >= 256 {
            answers.clear();
        }
        let at = Instant::now();
        let answer = Answer {
            etag,
            at,
            value: value.clone(),
        };
        answers.insert(key, answer);
        Ok(value)
    }

    /// The status, the JSON answer and its ETag. `--disable` keeps the user's `.curlrc` (which could trace the
    /// request to a file) out of it.
    fn send(
        &self,
        method: &str,
        path: &str,
        body: Option<&Body>,
        etag: Option<&str>,
    ) -> Result<(u16, Value, Option<String>)> {
        let mut config = String::new();
        let mut set = |key: &str, value: &str| {
            config.push_str(&format!("{key} = {}\n", crate::jira::quoted(value)));
        };
        set("url", &format!("{}{path}", self.base));
        set("request", method);
        for (option, value) in &self.auth {
            set(option, value);
        }
        set("header", "Accept: application/json");
        if let Some(etag) = etag {
            set("header", &format!("If-None-Match: {etag}"));
        }
        if let Some(body) = body {
            set("header", &format!("Content-Type: {}", body.kind));
            set("data-raw", &body.value.to_string());
        }
        set("write-out", "\n%{http_code}");
        set("max-time", "60");
        config.push_str("silent\nshow-error\ninclude\n");
        let mut child = crate::command(&self.curl)
            .args(["--disable", "--config", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| {
                format!("cannot run curl, which TermF needs to reach {}", self.name)
            })?;
        // A curl that exits early says why on stderr, which a broken pipe would hide.
        if let Some(mut stdin) = child.stdin.take()
            && let Err(e) = stdin.write_all(config.as_bytes())
            && e.kind() != std::io::ErrorKind::BrokenPipe
        {
            return Err(e.into());
        }
        let out = child.wait_with_output()?;
        if !out.status.success() {
            bail!(
                "cannot reach {}: {}",
                self.base,
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let (answer, status) = text.rsplit_once('\n').unwrap_or(("", &text));
        let status: u16 = status.trim().parse().unwrap_or(0);
        // Past the headers of each answer, `100 Continue` included.
        let mut rest = answer;
        let mut head = "";
        while rest.starts_with("HTTP/") {
            let Some((h, b)) = rest
                .split_once("\r\n\r\n")
                .or_else(|| rest.split_once("\n\n"))
            else {
                (head, rest) = (rest, "");
                break;
            };
            (head, rest) = (h, b);
        }
        let etag = head.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("etag")
                .then(|| value.trim().to_owned())
        });
        let body = rest;
        match status {
            304 => Ok((304, Value::Null, etag)),
            200..=299 if body.trim().is_empty() => Ok((status, Value::Null, etag)),
            200..=299 => {
                let value = serde_json::from_str(body)
                    .with_context(|| format!("{} gave no JSON for {path}", self.name))?;
                Ok((status, value, etag))
            }
            401 | 403 => bail!("{} ({status}): {}", self.refused, reason(body)),
            404 => bail!("not found on {} (404): {}", self.base, reason(body)),
            _ => bail!("{} answered {status}: {}", self.name, reason(body)),
        }
    }
}
