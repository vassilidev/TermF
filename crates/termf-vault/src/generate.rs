//! Passwords and passphrases from the system's randomness.

use anyhow::{Result, ensure};
use serde::Deserialize;
use termf_sync::crypto;

use crate::Secret;

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
/// Accepted by most sites, and harmless in a shell or a URL query once quoted: no quote, backslash nor space.
const SYMBOLS: &str = "!#$%&*+-.:;=?@^_~";
const AMBIGUOUS: &str = "0Oo1lI|";
/// The EFF's short word list 1 (CC BY 3.0 US, https://www.eff.org/dice): 1,296 short, distinct English words.
const WORDS: &str = include_str!("words.txt");

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Recipe {
    #[serde(rename_all = "camelCase")]
    Password {
        length: usize,
        lower: bool,
        upper: bool,
        digits: bool,
        symbols: bool,
        /// Without characters easily mistaken for others (0 O o 1 l I |).
        unambiguous: bool,
    },
    Passphrase {
        words: usize,
        separator: String,
    },
}

pub fn generate(recipe: &Recipe) -> Result<Secret> {
    match recipe {
        Recipe::Password {
            length,
            lower,
            upper,
            digits,
            symbols,
            unambiguous,
        } => {
            ensure!(
                (4..=128).contains(length),
                "a password has 4 to 128 characters"
            );
            let classes: Vec<Vec<char>> = [
                (lower, LOWER),
                (upper, UPPER),
                (digits, DIGITS),
                (symbols, SYMBOLS),
            ]
            .into_iter()
            .filter(|(on, _)| **on)
            .map(|(_, set)| {
                set.chars()
                    .filter(|c| !*unambiguous || !AMBIGUOUS.contains(*c))
                    .collect()
            })
            .collect();
            ensure!(!classes.is_empty(), "pick at least one kind of character");
            let all: Vec<char> = classes.concat();
            // Drawn again until every kind picked is in it: still uniform among such passwords.
            loop {
                let password = Secret::new((0..*length).map(|_| all[index(all.len())]).collect());
                if classes
                    .iter()
                    .all(|class| password.expose().chars().any(|c| class.contains(&c)))
                {
                    return Ok(password);
                }
            }
        }
        Recipe::Passphrase { words, separator } => {
            ensure!((3..=20).contains(words), "a passphrase has 3 to 20 words");
            ensure!(
                separator.chars().count() <= 3,
                "a separator has 3 characters at most"
            );
            let list: Vec<&str> = WORDS.lines().collect();
            let picked: Vec<&str> = (0..*words).map(|_| list[index(list.len())]).collect();
            Ok(Secret::new(picked.join(separator)))
        }
    }
}

/// Uniform in `0..n`, by rejection rather than a biased modulo.
fn index(n: usize) -> usize {
    let n = n as u32;
    let zone = u32::MAX - u32::MAX % n;
    loop {
        let draw = u32::from_le_bytes(crypto::random::<4>());
        if draw < zone {
            return (draw % n) as usize;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn password(length: usize, classes: [bool; 4], unambiguous: bool) -> Recipe {
        let [lower, upper, digits, symbols] = classes;
        Recipe::Password {
            length,
            lower,
            upper,
            digits,
            symbols,
            unambiguous,
        }
    }

    #[test]
    fn makes_passwords_of_the_kinds_asked() {
        for _ in 0..200 {
            let p = generate(&password(12, [true, true, true, true], true)).unwrap();
            let p = p.expose();
            assert_eq!(p.chars().count(), 12);
            assert!(p.chars().any(|c| c.is_ascii_lowercase()));
            assert!(p.chars().any(|c| c.is_ascii_uppercase()));
            assert!(p.chars().any(|c| c.is_ascii_digit()));
            assert!(p.chars().any(|c| SYMBOLS.contains(c)));
            assert!(!p.chars().any(|c| AMBIGUOUS.contains(c)), "{p}");
        }
        let digits = generate(&password(6, [false, false, true, false], false)).unwrap();
        assert!(digits.expose().chars().all(|c| c.is_ascii_digit()));
        assert_ne!(
            generate(&password(32, [true; 4], false)).unwrap(),
            generate(&password(32, [true; 4], false)).unwrap()
        );
        assert!(generate(&password(3, [true; 4], false)).is_err());
        assert!(generate(&password(12, [false; 4], false)).is_err());
    }

    #[test]
    fn makes_passphrases_from_the_word_list() {
        let list: Vec<&str> = WORDS.lines().collect();
        assert_eq!(list.len(), 1296);
        let phrase = generate(&Recipe::Passphrase {
            words: 5,
            separator: " ".into(),
        })
        .unwrap();
        let words: Vec<&str> = phrase.expose().split(' ').collect();
        assert_eq!(words.len(), 5);
        assert!(words.iter().all(|w| list.contains(w)));
        assert!(
            generate(&Recipe::Passphrase {
                words: 2,
                separator: "-".into()
            })
            .is_err()
        );
    }

    #[test]
    fn draws_every_index_evenly() {
        let mut seen = [0u32; 6];
        for _ in 0..6000 {
            seen[index(6)] += 1;
        }
        assert!(seen.iter().all(|n| (800..1200).contains(n)), "{seen:?}");
    }
}
