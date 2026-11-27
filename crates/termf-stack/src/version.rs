//! Versions as the ecosystems write them (semver, Composer's `v1.2.3`, Go's `v1.2.3+incompatible` and
//! pseudo-versions, PEP 440's `2.0rc1` and `.post1`, RubyGems' `7.1.0.rc1`), compared, and how far one update goes.

use std::cmp::Ordering;

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    nums: Vec<u64>,
    /// Empty for a release.
    pre: String,
}

/// How far an update goes: a patch or a minor stays within what the version's range accepts (`^`, `~>`, `~=`,
/// Cargo's default); a major breaks. Under `0.x`, a minor is a major, and under `0.0.x` so is a patch.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Bump {
    Patch,
    Minor,
    Major,
}

impl Version {
    pub fn parse(text: &str) -> Option<Version> {
        let text = text.trim();
        let text = text.strip_prefix(['v', 'V']).unwrap_or(text);
        // A PEP 440 epoch, and build metadata (semver's `+build`, Go's `+incompatible`, PEP 440's local part).
        let text = text.split_once('!').map_or(text, |(_, v)| v);
        let text = text.split('+').next().unwrap_or_default();
        let end = text
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(text.len());
        let (release, rest) = text.split_at(end);
        let release = release.trim_end_matches('.');
        if release.is_empty() || release.split('.').any(str::is_empty) {
            return None;
        }
        let mut nums: Vec<u64> = release
            .split('.')
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?;
        let rest = rest
            .trim_start_matches(['-', '.', '_'])
            .to_ascii_lowercase();
        let mut pre = String::new();
        match rest.strip_prefix("post").or_else(|| rest.strip_prefix("p")) {
            // A post-release comes after its release.
            Some(n) if !rest.is_empty() && n.chars().all(|c| c.is_ascii_digit() || c == '.') => {
                nums.push(n.trim_matches('.').parse().unwrap_or(0));
            }
            _ => pre = rest,
        }
        while nums.len() > 1 && nums.last() == Some(&0) {
            nums.pop();
        }
        Some(Version { nums, pre })
    }

    pub fn stable(&self) -> bool {
        self.pre.is_empty()
    }

    fn at(&self, i: usize) -> u64 {
        self.nums.get(i).copied().unwrap_or(0)
    }
}

/// A pre-release's identifiers, numbers before words as semver orders them: `rc.10` after `rc.2`, `b1` after `a2`.
fn pre_order(a: &str, b: &str) -> Ordering {
    fn parts(s: &str) -> Vec<(bool, String)> {
        let mut out: Vec<(bool, String)> = Vec::new();
        for c in s.chars().filter(|c| c.is_ascii_alphanumeric()) {
            match out.last_mut() {
                Some((digit, part)) if *digit == c.is_ascii_digit() => part.push(c),
                _ => out.push((c.is_ascii_digit(), c.to_string())),
            }
        }
        out
    }
    let (a, b) = (parts(a), parts(b));
    for (x, y) in a.iter().zip(&b) {
        let order = match (x.0, y.0) {
            (true, true) => {
                let (n, m) = (x.1.parse::<u64>(), y.1.parse::<u64>());
                n.unwrap_or(u64::MAX).cmp(&m.unwrap_or(u64::MAX))
            }
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => x.1.cmp(&y.1),
        };
        if order.is_ne() {
            return order;
        }
    }
    a.len().cmp(&b.len())
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let n = self.nums.len().max(other.nums.len());
        (0..n)
            .map(|i| self.at(i).cmp(&other.at(i)))
            .find(|o| o.is_ne())
            .unwrap_or_else(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => pre_order(&self.pre, &other.pre),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// `a` against `b`, by the rules above; `None` when one of them is not a version.
pub fn compare(a: &str, b: &str) -> Option<Ordering> {
    Some(Version::parse(a)?.cmp(&Version::parse(b)?))
}

/// How far going from `from` to `to` goes; `None` when `to` is not newer.
pub fn bump(from: &str, to: &str) -> Option<Bump> {
    let (from, to) = (Version::parse(from)?, Version::parse(to)?);
    if to <= from {
        return None;
    }
    let differ = |i: usize| from.at(i) != to.at(i);
    // The first number that is not 0 is the major: `0.3` → `0.4` breaks, and so does `0.0.3` → `0.0.4`.
    let major = (0..2).find(|i| from.at(*i) != 0).unwrap_or(2);
    Some(if (0..=major).any(differ) {
        Bump::Major
    } else if differ(1) {
        Bump::Minor
    } else {
        Bump::Patch
    })
}

/// The newest release among `versions`, and the newest one an update from `used` reaches without a major: the
/// safe one, `None` when `used` is already there.
pub fn best<'a>(
    versions: impl IntoIterator<Item = &'a str>,
    used: Option<&str>,
) -> (Option<String>, Option<String>) {
    let mut releases: Vec<(Version, &str)> = versions
        .into_iter()
        .filter_map(|v| Some((Version::parse(v)?, v)))
        .filter(|(v, _)| v.stable())
        .collect();
    releases.sort_by(|a, b| a.0.cmp(&b.0));
    let latest = releases.last().map(|(_, v)| v.to_string());
    let safe = used.and_then(|used| {
        releases
            .iter()
            .rev()
            .find(|(_, v)| matches!(bump(used, v), Some(Bump::Patch | Bump::Minor)))
            .map(|(_, v)| v.to_string())
    });
    (latest, safe)
}
