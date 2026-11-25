//! `~/.ssh/config`: its Host blocks, and edits that keep the rest of the file as it was.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Host {
    pub patterns: Vec<String>,
    pub host_name: Option<String>,
    pub user: Option<String>,
    pub port: Option<String>,
    /// Absolute paths.
    pub identity_files: Vec<String>,
    pub identities_only: Option<bool>,
    pub file: String,
    /// 1-based; 0 for the options set before the first `Host`, which apply to every host.
    pub line: usize,
}

impl Host {
    /// OpenSSH rules: one pattern must match and no negated one may.
    pub fn matches(&self, host: &str) -> bool {
        let host = host.to_lowercase();
        let mut matched = false;
        for pattern in &self.patterns {
            let pattern = pattern.to_lowercase();
            match pattern.strip_prefix('!') {
                Some(negated) if wildcard(negated, &host) => return false,
                Some(_) => {}
                None => matched |= wildcard(&pattern, &host),
            }
        }
        matched
    }

    fn set(&mut self, keyword: &str, args: Vec<String>, home: &Path) {
        let Some(value) = args.into_iter().next() else {
            return;
        };
        // The first value given wins, except for IdentityFile which adds up.
        match keyword {
            "hostname" => _ = self.host_name.get_or_insert(value),
            "user" => _ = self.user.get_or_insert(value),
            "port" => _ = self.port.get_or_insert(value),
            "identityfile" => self
                .identity_files
                .push(expand(&value, home).to_string_lossy().into_owned()),
            "identitiesonly" => {
                _ = self
                    .identities_only
                    .get_or_insert(value.eq_ignore_ascii_case("yes"))
            }
            _ => {}
        }
    }

    fn has_settings(&self) -> bool {
        self.host_name.is_some()
            || self.user.is_some()
            || self.port.is_some()
            || !self.identity_files.is_empty()
            || self.identities_only.is_some()
    }
}

/// What the UI sets on a host; `None` removes the option from the block.
#[derive(Deserialize, Serialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HostEntry {
    pub alias: String,
    pub host_name: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    /// A key inside `~/.ssh`.
    pub identity_file: Option<String>,
    pub identities_only: Option<bool>,
}

// OpenSSH's own limit.
const MAX_INCLUDE_DEPTH: usize = 16;

#[derive(Default)]
struct Parsed {
    /// In the order ssh reads them: an included file's blocks sit where its `Include` is.
    hosts: Vec<Host>,
    /// The config, then each file it includes, once.
    files: Vec<PathBuf>,
}

/// Best effort: an `Include` inside a block is read as if it were global, and `Match` blocks are skipped.
pub fn hosts(ssh_dir: &Path, home: &Path) -> Vec<Host> {
    parse(ssh_dir, home, None).hosts
}

pub fn files(ssh_dir: &Path, home: &Path) -> Vec<PathBuf> {
    parse(ssh_dir, home, None).files
}

/// `config`: the text to read instead of `~/.ssh/config`, for a preview.
fn parse(ssh_dir: &Path, home: &Path, config: Option<&str>) -> Parsed {
    let mut parsed = Parsed::default();
    let file = ssh_dir.join("config");
    let text = config
        .map(str::to_owned)
        .or_else(|| fs::read_to_string(&file).ok());
    if let Some(text) = text {
        read_hosts(&text, &file, ssh_dir, home, 0, &mut parsed);
    }
    parsed
}

fn read_hosts(
    text: &str,
    file: &Path,
    ssh_dir: &Path,
    home: &Path,
    depth: usize,
    out: &mut Parsed,
) {
    if !out.files.iter().any(|f| f == file) {
        out.files.push(file.to_path_buf());
    }
    let file_name = file.to_string_lossy().into_owned();
    let keep = |host: &Host| host.line > 0 || host.has_settings();
    let mut current = Some(Host {
        patterns: vec!["*".into()],
        file: file_name.clone(),
        ..Default::default()
    });
    for (i, line) in text.lines().enumerate() {
        let Some((keyword, args)) = directive(line) else {
            continue;
        };
        match keyword.as_str() {
            "host" | "match" => {
                out.hosts.extend(current.take().filter(keep));
                current = (keyword == "host").then(|| Host {
                    patterns: args,
                    file: file_name.clone(),
                    line: i + 1,
                    ..Default::default()
                });
            }
            "include" if depth < MAX_INCLUDE_DEPTH => {
                for path in args.iter().flat_map(|a| include_paths(a, ssh_dir, home)) {
                    if let Ok(text) = fs::read_to_string(&path) {
                        read_hosts(&text, &path, ssh_dir, home, depth + 1, out);
                    }
                }
            }
            _ => {
                if let Some(host) = &mut current {
                    host.set(&keyword, args, home);
                }
            }
        }
    }
    out.hosts.extend(current.filter(keep));
}

fn include_paths(arg: &str, ssh_dir: &Path, home: &Path) -> Vec<PathBuf> {
    let path = if arg.starts_with('~') {
        expand(arg, home)
    } else {
        ssh_dir.join(arg)
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !name.contains(['*', '?']) {
        return vec![path];
    }
    // shortcut: wildcards are matched in the file name only, not in folder names; enough for `config.d/*`.
    let Some(Ok(entries)) = path.parent().map(fs::read_dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| wildcard(&name, &e.file_name().to_string_lossy()))
        .map(|e| e.path())
        .collect();
    found.sort();
    found
}

/// `~`, `%d` and relative paths resolved against the home folder, as ssh does for IdentityFile.
/// Rebuilt from its components so `~/.ssh/id` gets the platform's separators on Windows.
pub(crate) fn expand(path: &str, home: &Path) -> PathBuf {
    let path = path.replace("%d", &home.to_string_lossy());
    let path = match path.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            home.join(rest.trim_start_matches(['/', '\\']))
        }
        _ => home.join(path),
    };
    path.components().collect()
}

fn wildcard(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

fn directive(line: &str) -> Option<(String, Vec<String>)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let end = line
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(line.len());
    let (keyword, rest) = line.split_at(end);
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('=').unwrap_or(rest);
    Some((keyword.to_ascii_lowercase(), words(rest)))
}

/// Splits arguments like OpenSSH (and, for the common cases, a shell): whitespace separates, quotes
/// group, a backslash escapes only quotes, backslashes and spaces, and `#` starts a comment.
pub(crate) fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word: Option<String> = None;
    let mut quote = None;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let escaped = matches!(chars.peek(), Some('\'' | '"' | '\\'))
            || (quote.is_none() && chars.peek() == Some(&' '));
        match c {
            '\\' if escaped => word.get_or_insert_default().extend(chars.next()),
            c if Some(c) == quote => quote = None,
            '\'' | '"' if quote.is_none() => {
                quote = Some(c);
                word.get_or_insert_default();
            }
            '#' if quote.is_none() && word.is_none() => break,
            c if c.is_whitespace() && quote.is_none() => out.extend(word.take()),
            c => word.get_or_insert_default().push(c),
        }
    }
    out.extend(word);
    out
}

fn indent(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

// Double quotes group words for every OpenSSH version; since 8.7 a single quote opens a quote too.
fn quote(value: &str) -> String {
    if value.contains(|c: char| c.is_whitespace() || c == '#' || c == '\'') {
        format!("\"{value}\"")
    } else {
        value.into()
    }
}

fn check(entry: &HostEntry) -> Result<()> {
    let alias = &entry.alias;
    ensure!(
        !alias.is_empty()
            && !alias.starts_with('-')
            && !alias.contains(|c: char| c.is_whitespace()
                || c.is_control()
                || "*?!\"'#\\=,".contains(c)),
        "invalid host alias {alias:?}"
    );
    for value in [&entry.host_name, &entry.user, &entry.identity_file]
        .into_iter()
        .flatten()
    {
        ensure!(
            !value.trim().is_empty() && !value.contains(|c: char| c.is_control()),
            "invalid value {value:?}"
        );
        // OpenSSH before 8.7 reads no escapes and later versions do: no spelling means the same to both.
        ensure!(
            !value.contains(['"', '\\']),
            "{value:?}: TermF cannot write a \" or \\ in ~/.ssh/config, edit the file by hand"
        );
    }
    ensure!(entry.port != Some(0), "invalid port 0");
    Ok(())
}

fn declares(line: &str, alias: &str) -> Option<Vec<String>> {
    match directive(line) {
        Some((k, patterns))
            if k == "host" && patterns.iter().any(|p| p.eq_ignore_ascii_case(alias)) =>
        {
            Some(patterns)
        }
        _ => None,
    }
}

/// Put above each block TermF adds; deleting the block takes it too.
pub const MARK: &str = "# Written by TermF";

/// `text` with the block of `entry.alias` added or updated. Only the five options of `HostEntry` change:
/// other options, comments and blocks are kept as they were. A new block goes before `Host *`, since the
/// first value found wins and a block after it could not override its defaults.
pub fn upsert(text: &str, entry: &HostEntry) -> Result<String> {
    check(entry)?;
    let options: [(&str, Option<String>); 5] = [
        ("HostName", entry.host_name.clone()),
        ("User", entry.user.clone()),
        ("Port", entry.port.map(|p| p.to_string())),
        ("IdentityFile", entry.identity_file.clone()),
        (
            "IdentitiesOnly",
            entry
                .identities_only
                .map(|b| if b { "yes" } else { "no" }.into()),
        ),
    ];
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let starts_block =
        |l: &str| matches!(directive(l), Some((k, _)) if k == "host" || k == "match");
    let start = lines
        .iter()
        .position(|l| declares(l, &entry.alias).is_some());
    // A second block for the alias would be ignored where they disagree, and editing a shared one would
    // change the other hosts too.
    if let Some(start) = start {
        ensure!(
            declares(&lines[start], &entry.alias).is_some_and(|p| p.len() == 1),
            "{} shares its block with other hosts (line {}: `{}`): edit ~/.ssh/config by hand",
            entry.alias,
            start + 1,
            lines[start].trim()
        );
    }
    match start {
        Some(start) => {
            let end = lines[start + 1..]
                .iter()
                .position(|l| starts_block(l))
                .map_or(lines.len(), |i| start + 1 + i);
            let mut block = Vec::new();
            let mut seen = Vec::new();
            for line in &lines[start + 1..end] {
                let managed = directive(line).and_then(|(keyword, args)| {
                    options
                        .iter()
                        .find(|(name, _)| name.eq_ignore_ascii_case(&keyword))
                        .map(|(name, value)| (*name, value, args))
                });
                let Some((name, value, args)) = managed else {
                    block.push(line.clone());
                    continue;
                };
                // A repeated option is dropped: the form sets a single value.
                if seen.contains(&name) {
                    continue;
                }
                seen.push(name);
                match value {
                    Some(v) if args == [v.as_str()] => block.push(line.clone()),
                    Some(v) => block.push(format!("{}{name} {}", indent(line), quote(v))),
                    None => {}
                }
            }
            let pad = block
                .iter()
                .find(|l| directive(l).is_some())
                .map_or("  ", |l| indent(l))
                .to_owned();
            let missing: Vec<String> = options
                .iter()
                .filter(|(name, _)| !seen.contains(name))
                .filter_map(|(name, value)| Some(format!("{pad}{name} {}", quote(value.as_ref()?))))
                .collect();
            let at = block
                .iter()
                .rposition(|l| directive(l).is_some())
                .map_or(0, |i| i + 1);
            block.splice(at..at, missing);
            lines.splice(start + 1..end, block);
        }
        None => {
            let mut block = vec![MARK.to_owned(), format!("Host {}", entry.alias)];
            block.extend(
                options.iter().filter_map(|(name, value)| {
                    Some(format!("  {name} {}", quote(value.as_ref()?)))
                }),
            );
            let catch_all = lines
                .iter()
                .position(|l| matches!(directive(l), Some((k, a)) if k == "host" && a == ["*"]));
            match catch_all {
                Some(mut at) => {
                    // Comments right above `Host *` describe it and stay with it.
                    while at > 0 && lines[at - 1].trim_start().starts_with('#') {
                        at -= 1;
                    }
                    block.push(String::new());
                    lines.splice(at..at, block);
                }
                None => {
                    if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                        lines.push(String::new());
                    }
                    lines.extend(block);
                }
            }
        }
    }
    Ok(join(text, &lines))
}

/// `text` without the block that declares `alias` alone, its TermF mark with it, as the Hosts tab deletes it.
pub fn remove(text: &str, alias: &str) -> Result<String> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|l| declares(l, alias).is_some())
        .with_context(|| format!("~/.ssh/config has no Host {alias}"))?;
    ensure!(
        declares(lines[start], alias).is_some_and(|p| p.len() == 1),
        "{alias} shares its block with other hosts (line {}: `{}`): edit ~/.ssh/config by hand",
        start + 1,
        lines[start].trim()
    );
    let first = if start > 0 && lines[start - 1].trim() == MARK {
        start - 1
    } else {
        start
    };
    Ok(cut(text, &lines, first, start))
}

/// `text` without lines `first..` to the end of the block that starts at `start`, and the blank lines after it.
fn cut(text: &str, lines: &[&str], first: usize, start: usize) -> String {
    let mut end = lines[start + 1..]
        .iter()
        .position(|l| matches!(directive(l), Some((k, _)) if k == "host" || k == "match"))
        .map_or(lines.len(), |i| start + 1 + i);
    // Comments right above the next block (its mark among them) belong to it.
    while end > start + 1 && lines[end - 1].trim_start().starts_with('#') {
        end -= 1;
    }
    while end < lines.len() && lines[end].trim().is_empty() {
        end += 1;
    }
    let mut kept: Vec<String> = lines[..first]
        .iter()
        .chain(&lines[end..])
        .map(|l| l.to_string())
        .collect();
    while kept.last().is_some_and(|l| l.trim().is_empty()) {
        kept.pop();
    }
    if kept.is_empty() {
        return String::new();
    }
    join(text, &kept)
}

/// Why the block of `entry` may not fully apply once `config` is `~/.ssh/config`: ssh keeps the first
/// value it reads, so an earlier block matching the alias wins. A block for it in an included file is
/// reported too.
pub fn warnings(config: &str, entry: &HostEntry, ssh_dir: &Path, home: &Path) -> Vec<String> {
    let hosts = parse(ssh_dir, home, Some(config)).hosts;
    let main = ssh_dir.join("config").to_string_lossy().into_owned();
    let alias = &entry.alias;
    let Some(at) = hosts.iter().position(|h| {
        h.file == main && h.patterns.len() == 1 && h.patterns[0].eq_ignore_ascii_case(alias)
    }) else {
        return Vec::new();
    };
    let place = |h: &Host| match h.line {
        0 => format!("the top of {}, before any Host", h.file),
        line => format!("`Host {}` ({}:{line})", h.patterns.join(" "), h.file),
    };
    let mut warnings: Vec<String> = hosts
        .iter()
        .filter(|h| h.file != main && h.patterns.iter().any(|p| p.eq_ignore_ascii_case(alias)))
        .map(|h| format!("{alias} is also declared in {}", place(h)))
        .collect();
    for host in hosts[..at].iter().filter(|h| h.matches(alias)) {
        let options: Vec<&str> = [
            (
                "HostName",
                entry.host_name.is_some() && host.host_name.is_some(),
            ),
            ("User", entry.user.is_some() && host.user.is_some()),
            ("Port", entry.port.is_some() && host.port.is_some()),
            (
                "IdentityFile",
                entry.identity_file.is_some() && !host.identity_files.is_empty(),
            ),
            (
                "IdentitiesOnly",
                entry.identities_only.is_some() && host.identities_only.is_some(),
            ),
        ]
        .into_iter()
        .filter_map(|(name, both)| both.then_some(name))
        .collect();
        if !options.is_empty() {
            warnings.push(format!(
                "{alias} already gets {} from {}, which ssh reads first",
                options.join(", "),
                place(host)
            ));
        }
    }
    warnings
}

/// Returns the text and the hosts whose IdentityFile changed.
pub fn replace_identity(
    text: &str,
    home: &Path,
    old: &[PathBuf],
    new: &str,
) -> (String, Vec<String>) {
    let mut hosts = Vec::new();
    let mut current = "*".to_owned();
    let lines: Vec<String> = text
        .lines()
        .map(|line| match directive(line) {
            Some((k, args)) if k == "host" => {
                current = args.join(" ");
                line.to_owned()
            }
            Some((k, args))
                if k == "identityfile"
                    && args.first().is_some_and(|f| {
                        old.iter().any(|o| crate::same_file(&expand(f, home), o))
                    }) =>
            {
                hosts.push(current.clone());
                format!("{}IdentityFile {}", indent(line), quote(new))
            }
            _ => line.to_owned(),
        })
        .collect();
    (join(text, &lines), hosts)
}

fn join(original: &str, lines: &[String]) -> String {
    let eol = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut text = lines.join(eol);
    text.push_str(eol);
    text
}

pub use termf_core::diff;

/// Replaces `file` atomically, readable by the user only. A symlinked config (dotfiles) is written
/// through, so the link stays.
pub fn write(file: &Path, text: impl AsRef<[u8]>) -> Result<()> {
    let file = fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = file.with_file_name(format!(".{name}.termf-tmp"));
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut out = options
        .open(&tmp)
        .with_context(|| format!("cannot write {}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        out.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    out.write_all(text.as_ref())?;
    out.sync_all()?;
    fs::rename(&tmp, &file).with_context(|| format!("cannot write {}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_words_like_openssh() {
        assert_eq!(words("a  b"), ["a", "b"]);
        assert_eq!(words(r#""My Key" x"#), ["My Key", "x"]);
        assert_eq!(words("'it'\\''s'"), ["it's"]);
        assert_eq!(words(r"C:\Users\me\.ssh\id"), [r"C:\Users\me\.ssh\id"]);
        assert_eq!(words(r"a\ b"), ["a b"]);
        assert_eq!(words("host # comment"), ["host"]);
        assert_eq!(words("\"\""), [""]);
        assert_eq!(
            directive("  IdentityFile=~/.ssh/id"),
            Some(("identityfile".into(), vec!["~/.ssh/id".into()]))
        );
        assert_eq!(
            directive("Port = 22"),
            Some(("port".into(), vec!["22".into()]))
        );
        assert_eq!(directive("  # Host x"), None);
    }

    #[test]
    fn matches_host_patterns() {
        let host = |p: &[&str]| Host {
            patterns: p.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        assert!(host(&["*"]).matches("github.com"));
        assert!(host(&["*.example.com"]).matches("Git.Example.com"));
        assert!(!host(&["*.example.com"]).matches("example.com"));
        assert!(host(&["gh?"]).matches("gh1"));
        assert!(host(&["*", "!bastion"]).matches("web"));
        assert!(!host(&["*", "!bastion"]).matches("bastion"));
        assert!(
            !host(&["!bastion"]).matches("web"),
            "negation alone matches nothing"
        );
    }

    #[test]
    fn diff_shows_only_the_change() {
        assert_eq!(diff("a\nb\n", "a\nb\n"), "");
        let before = "1\n2\n3\n4\n5\n6\n7\n8\n";
        let after = "1\n2\n3\n4\nfive\n6\n7\n8\n";
        assert_eq!(diff(before, after), " 2\n 3\n 4\n-5\n+five\n 6\n 7\n 8\n");
        assert_eq!(diff("", "Host x\n"), "+Host x\n");
    }
}
