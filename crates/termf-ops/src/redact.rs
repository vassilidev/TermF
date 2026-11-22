//! What a recipe printed, masked before anyone sees it (ARCHITECTURE §12.4): the vault's values first (the user
//! may reveal each later, never an agent), then what looks like a secret (`termf_claude::mask`: token shapes,
//! keys and passwords, JWTs, long random strings), shorter random-looking words, and the personal data the
//! policy names. Then cut to the recipe's limits: masked first, so a cut never splits a value it would hide.

use std::sync::LazyLock;

use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use termf_claude::mask::{MASK, mask_text};
use termf_vault::{Masker, Secret};

use crate::sql::Table;
use crate::{Kind, Limits, Raw};

/// Personal data a recipe masks; the vault's values and what looks like a secret are always masked.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Redact {
    Email,
    Phone,
    Iban,
    Jwt,
    VaultValues,
}

impl Redact {
    /// What an agent with `redacted` access never sees.
    pub const PERSONAL: [Redact; 3] = [Redact::Email, Redact::Phone, Redact::Iban];
}

/// A piece of the masked output: text, or a value of the vault, which only the user may reveal.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum Part {
    Text { text: String },
    Hidden { hidden: usize },
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    /// Masked, cut to the limits.
    pub text: String,
    /// The same, the vault's values apart; empty when there are none.
    pub parts: Vec<Part>,
    /// What the client said on its error output, masked, 8 KiB at most.
    pub errors: String,
    pub exit: Option<i32>,
    pub stopped: Option<String>,
    pub truncated: bool,
    /// What the client printed, before masking and cutting.
    pub bytes: u64,
    pub masks: usize,
    pub millis: u64,
    /// A query's result, each cell masked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table: Option<Table>,
    /// A `.env`'s keys, in its order: their values are `hidden`, at the same place.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
    /// The values behind `Part::Hidden`, or behind `keys`.
    #[serde(skip)]
    pub hidden: Vec<Secret>,
}

const ERRORS: usize = 8 << 10;
/// Shorter values are not looked for, as in the vault's own masker.
const MIN: usize = 4;

static JWT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}").unwrap()
});
static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z0-9+/_-]{20,}").unwrap());
static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}").unwrap()
});
static IBAN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b[A-Z]{2}\d{2}(?: ?[A-Z0-9]{4}){2,7}(?: ?[A-Z0-9]{1,4})?\b").unwrap()
});
static PHONE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\+\d{1,3}[ .-]?(?:\(\d{1,4}\)[ .-]?)?\d{1,4}(?:[ .-]?\d{2,4}){2,4}\b|\b0[1-9](?:[ .-]?\d{2}){4}\b")
        .unwrap()
});

/// A word of 20 characters or more, upper and lower case and digits mixed, of high entropy: a key, not a word,
/// a path, a hash (hexadecimal stays under 4 bits a character) or an id.
fn random(word: &str) -> bool {
    let has = |f: fn(&char) -> bool| word.chars().any(|c| f(&c));
    if !(has(char::is_ascii_uppercase)
        && has(char::is_ascii_lowercase)
        && has(char::is_ascii_digit))
    {
        return false;
    }
    let mut counts = [0usize; 256];
    for b in word.bytes() {
        counts[b as usize] += 1;
    }
    let n = word.len() as f64;
    let entropy: f64 = counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / n;
            -p * p.log2()
        })
        .sum();
    entropy >= 4.0
}

fn iban(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if !(15..=34).contains(&compact.len()) {
        return false;
    }
    let moved = format!("{}{}", &compact[4..], &compact[..4]);
    let mut rest = 0u32;
    for c in moved.chars() {
        let digits = match c.to_digit(36) {
            Some(d) => d.to_string(),
            None => return false,
        };
        for d in digits.chars() {
            rest = (rest * 10 + d.to_digit(10).unwrap_or(0)) % 97;
        }
    }
    rest == 1
}

/// Masks everything but the vault's values in `text`; returns it and how many masks went in.
fn scrub(text: &str, personal: &[Redact]) -> (String, usize) {
    let mut count = 0;
    let mut replace =
        |re: &Regex, text: &str, with: &dyn Fn(&str) -> Option<&'static str>| -> String {
            re.replace_all(text, |c: &Captures| match with(&c[0]) {
                Some(mask) => {
                    count += 1;
                    mask.to_owned()
                }
                None => c[0].to_owned(),
            })
            .into_owned()
        };
    let text = replace(&JWT, text, &|_| Some(MASK));
    let before = text.matches(MASK).count();
    let text = mask_text(&text);
    let pattern = text.matches(MASK).count().saturating_sub(before);
    let mut text = replace(&WORD, &text, &|w| random(w).then_some(MASK));
    if personal.contains(&Redact::Email) {
        text = replace(&EMAIL, &text, &|_| Some("[email]"));
    }
    if personal.contains(&Redact::Iban) {
        text = replace(&IBAN, &text, &|m| iban(m).then_some("[iban]"));
    }
    if personal.contains(&Redact::Phone) {
        let digits = |m: &str| m.chars().filter(char::is_ascii_digit).count();
        text = replace(&PHONE, &text, &|m| {
            (9..=15).contains(&digits(m)).then_some("[phone]")
        });
    }
    (text, count + pattern)
}

/// The values to look for, the longest first, each once.
fn values<'a>(masker: Option<&'a Masker>, used: &'a [Secret]) -> Vec<&'a str> {
    let mut values: Vec<&str> = masker
        .into_iter()
        .flat_map(Masker::values)
        .chain(used.iter().map(Secret::expose))
        .filter(|v| v.len() >= MIN)
        .collect();
    values.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    values.dedup();
    values
}

/// Masks `text`, the vault's values found in it apart.
fn split(text: &str, values: &[&str], personal: &[Redact]) -> (Vec<Part>, Vec<Secret>, usize) {
    let mut spans: Vec<(usize, usize, usize)> = Vec::new();
    for (i, value) in values.iter().enumerate() {
        for (at, _) in text.match_indices(value) {
            let end = at + value.len();
            if !spans.iter().any(|&(s, e, _)| at < e && s < end) {
                spans.push((at, end, i));
            }
        }
    }
    spans.sort();
    let (mut parts, mut hidden, mut found): (Vec<Part>, Vec<(usize, Secret)>, usize) =
        (Vec::new(), Vec::new(), 0);
    let mut at = 0;
    let mut gap = |parts: &mut Vec<Part>, piece: &str| {
        if !piece.is_empty() {
            let (text, n) = scrub(piece, personal);
            found += n;
            parts.push(Part::Text { text });
        }
    };
    for (start, end, value) in spans {
        gap(&mut parts, &text[at..start]);
        let index = match hidden.iter().position(|(v, _)| *v == value) {
            Some(i) => i,
            None => {
                hidden.push((value, Secret::new(values[value].to_owned())));
                hidden.len() - 1
            }
        };
        parts.push(Part::Hidden { hidden: index });
        at = end;
    }
    gap(&mut parts, &text[at..]);
    let vault = parts
        .iter()
        .filter(|p| matches!(p, Part::Hidden { .. }))
        .count();
    (
        parts,
        hidden.into_iter().map(|(_, s)| s).collect(),
        found + vault,
    )
}

fn render(parts: &[Part]) -> String {
    parts
        .iter()
        .map(|p| match p {
            Part::Text { text } => text.as_str(),
            Part::Hidden { .. } => MASK,
        })
        .collect()
}

/// Keeps `lines` lines and `bytes` bytes of `parts`; true when something was cut.
fn cut(parts: &mut Vec<Part>, lines: usize, bytes: usize) -> bool {
    let (mut seen_lines, mut seen_bytes) = (0, 0);
    for i in 0..parts.len() {
        let len = match &parts[i] {
            Part::Hidden { .. } => MASK.len(),
            Part::Text { text } => text.len(),
        };
        let mut stop = None;
        if let Part::Text { text } = &parts[i] {
            for (at, c) in text.char_indices() {
                if seen_bytes + at + c.len_utf8() > bytes {
                    stop = Some(at);
                    break;
                }
                if c == '\n' {
                    seen_lines += 1;
                    if seen_lines >= lines {
                        stop = Some(at + 1).filter(|&end| end < text.len() || i + 1 < parts.len());
                        break;
                    }
                }
            }
        } else if seen_bytes + len > bytes {
            stop = Some(0);
        }
        match stop {
            Some(end) => {
                if let Part::Text { text } = &mut parts[i] {
                    text.truncate(end);
                    parts.truncate(i + 1);
                } else {
                    parts.truncate(i);
                }
                return true;
            }
            None => seen_bytes += len,
        }
    }
    false
}

/// The client's error output, masked, 8 KiB at most.
fn errors(stderr: &str, values: &[&str], personal: &[Redact]) -> String {
    let (errors, _, _) = split(stderr, values, personal);
    let mut errors = render(&errors);
    if errors.len() > ERRORS {
        let end = (0..=ERRORS)
            .rev()
            .find(|&i| errors.is_char_boundary(i))
            .unwrap_or(0);
        errors.truncate(end);
    }
    errors
}

/// Masks a client's output for whoever reads it. `personal`: the personal data to mask too.
pub fn mask(raw: Raw, masker: Option<&Masker>, personal: &[Redact], limits: &Limits) -> Output {
    let values = values(masker, &raw.used);
    let (mut parts, hidden, masks) = split(&raw.stdout, &values, personal);
    let truncated = cut(&mut parts, limits.lines, limits.bytes);
    let text = render(&parts);
    let any_hidden = parts.iter().any(|p| matches!(p, Part::Hidden { .. }));
    Output {
        text,
        parts: if any_hidden { parts } else { Vec::new() },
        errors: errors(&raw.stderr, &values, personal),
        exit: raw.exit,
        stopped: raw.stopped,
        truncated,
        bytes: raw.bytes,
        masks,
        millis: raw.millis,
        table: None,
        keys: Vec::new(),
        hidden,
    }
}

/// A query's output as a table, each cell masked as a text is (a value of the vault hidden, not revealable
/// there), its rows cut to `limits.rows`. A query that failed has no table: its errors say why.
pub fn table(
    raw: Raw,
    kind: Kind,
    masker: Option<&Masker>,
    personal: &[Redact],
    limits: &Limits,
) -> Output {
    let values = values(masker, &raw.used);
    let mut table = crate::sql::table(kind, &raw.stdout);
    let rows = limits.rows.unwrap_or(crate::sql::ROWS);
    let truncated = table.rows.len() > rows;
    table.rows.truncate(rows);
    let mut masks = 0;
    for cell in table
        .columns
        .iter_mut()
        .chain(table.rows.iter_mut().flatten())
    {
        let (parts, _, n) = split(cell, &values, personal);
        masks += n;
        *cell = render(&parts);
    }
    Output {
        text: crate::sql::render(&table),
        errors: errors(&raw.stderr, &values, personal),
        exit: raw.exit,
        stopped: raw.stopped,
        truncated,
        bytes: raw.bytes,
        masks,
        millis: raw.millis,
        table: (!table.columns.is_empty()).then_some(table),
        ..Output::default()
    }
}

/// A `.env`'s output: its keys, each value kept apart for the user to reveal one by one, never in the text.
pub fn dotenv(mut raw: Raw, masker: Option<&Masker>, limits: &Limits) -> Output {
    let stdout = termf_vault::Zeroizing::new(std::mem::take(&mut raw.stdout));
    let parsed = crate::env::parse(&stdout);
    let truncated = parsed.len() > limits.lines;
    let (keys, hidden): (Vec<String>, Vec<Secret>) = parsed
        .into_iter()
        .take(limits.lines)
        .map(|(k, v)| (k, Secret::new(v)))
        .unzip();
    let mut values = values(masker, &raw.used);
    values.extend(hidden.iter().map(Secret::expose).filter(|v| v.len() >= MIN));
    // The longest first, as `values` gives them: a shorter value must not leave the end of a longer one shown.
    values.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    values.dedup();
    Output {
        text: keys.iter().map(|k| format!("{k}={MASK}\n")).collect(),
        errors: errors(&raw.stderr, &values, &[]),
        exit: raw.exit,
        stopped: raw.stopped,
        truncated,
        bytes: raw.bytes,
        masks: keys.len(),
        millis: raw.millis,
        keys,
        hidden,
        ..Output::default()
    }
}

/// Masks a stream line by line: the vault's values (never revealable there), what looks like a secret, and the
/// personal data its recipe names.
pub struct Lines {
    values: Vec<Secret>,
    personal: Vec<Redact>,
}

impl Lines {
    pub fn new(masker: Option<&Masker>, used: &[Secret], personal: &[Redact]) -> Self {
        Self {
            values: values(masker, used)
                .into_iter()
                .map(|v| Secret::new(v.to_owned()))
                .collect(),
            personal: personal.to_vec(),
        }
    }

    /// The line masked, and how many masks went in.
    pub fn mask(&self, line: &str) -> (String, usize) {
        let values: Vec<&str> = self.values.iter().map(Secret::expose).collect();
        let (parts, _, n) = split(line, &values, &self.personal);
        (render(&parts), n)
    }
}

/// Masks a short text (a test's message) with these values and what looks like a secret.
pub fn mask_with(text: &str, used: &[Secret]) -> String {
    let values = values(None, used);
    render(&split(text, &values, &[]).0)
}
