//! Hides the vault's values in what TermF shows or passes on, then whatever else looks like a secret
//! (`termf_claude::mask`).

use termf_claude::mask::{MASK, mask_text};

use crate::Secret;

/// Shorter values are not looked for: they would hide common words, and the masks would tell the value.
const MIN: usize = 4;

#[derive(Default, Clone, Debug)]
pub struct Masker {
    /// The longest first, so a value inside another does not split it.
    values: Vec<Secret>,
}

#[derive(Debug, PartialEq)]
pub struct Masked {
    pub text: String,
    /// How many masks were put in, for the audit.
    pub count: usize,
}

impl Masker {
    pub fn new<'a>(values: impl IntoIterator<Item = &'a Secret>) -> Self {
        let mut masker = Self::default();
        for value in values {
            masker.add(value);
        }
        masker
    }

    /// Also a value known only while it is used, such as a resolved reference.
    pub fn add(&mut self, value: &Secret) {
        let value = value.expose();
        // A value on several lines (a PEM key) also hides those of its lines that hold key material, never
        // its armor nor a JSON file's ordinary lines.
        self.insert(std::iter::once(value).chain(key_lines(value)));
    }

    /// Only the lines that hold key material: a note's prose stays readable.
    pub fn add_key_lines(&mut self, value: &Secret) {
        self.insert(key_lines(value.expose()));
    }

    fn insert<'a>(&mut self, values: impl Iterator<Item = &'a str>) {
        for v in values {
            if v.len() >= MIN && !self.values.iter().any(|known| known.expose() == v) {
                self.values.push(Secret::new(v.into()));
            }
        }
        self.values
            .sort_by_key(|v| std::cmp::Reverse(v.expose().len()));
    }

    pub fn mask(&self, text: &str) -> Masked {
        let mut text = text.to_owned();
        let mut count = 0;
        for value in &self.values {
            let found = text.matches(value.expose()).count();
            if found > 0 {
                count += found;
                text = text.replace(value.expose(), MASK);
            }
        }
        let before = text.matches(MASK).count();
        let text = mask_text(&text);
        count += text.matches(MASK).count().saturating_sub(before);
        Masked { text, count }
    }
}

fn key_lines(value: &str) -> impl Iterator<Item = &str> {
    value.lines().map(str::trim).filter(|l| key_material(l))
}

/// A long run of base64 or random-looking characters, letters and digits mixed: a PEM body line, a token.
fn key_material(line: &str) -> bool {
    line.len() >= 16
        && !line.starts_with("-----")
        && line
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+/=_-.".contains(c))
        && line.chars().any(|c| c.is_ascii_digit())
        && line.chars().any(|c| c.is_ascii_alphabetic())
}
