//! `~/.ssh/config` and `~/.gitconfig` merged as what they are: blocks (`Host`, `Match`, `[section]`, and what
//! comes before the first one), each a list of options. Comments and blank lines go with the line below them,
//! so they stay where they were; a file is only rewritten where something changed.

#[derive(Clone, Copy, PartialEq)]
pub enum Syntax {
    Ssh,
    Git,
}

/// Options that can be given more than once, each line on its own: they add up instead of replacing each other.
const SSH_LISTS: [&str; 8] = [
    "include",
    "identityfile",
    "certificatefile",
    "localforward",
    "remoteforward",
    "dynamicforward",
    "sendenv",
    "setenv",
];
const GIT_LISTS: [&str; 5] = ["path", "fetch", "push", "insteadof", "pushinsteadof"];

type Items<T> = Vec<(String, T)>;

#[derive(Clone)]
struct Block {
    head: String,
    options: Items<String>,
}

impl Syntax {
    pub fn of(path: &str) -> Option<Syntax> {
        match path {
            "ssh/config" => Some(Syntax::Ssh),
            "dev/gitconfig" => Some(Syntax::Git),
            _ => None,
        }
    }

    fn comment(self, line: &str) -> bool {
        line.is_empty() || line.starts_with('#') || self == Syntax::Git && line.starts_with(';')
    }

    /// The option's name and value.
    fn split(self, line: &str) -> (String, &str) {
        let (name, value) = match self {
            Syntax::Ssh => line
                .split_once(|c: char| c.is_whitespace() || c == '=')
                .map(|(n, v)| {
                    (
                        n,
                        v.trim_start_matches(|c: char| c.is_whitespace() || c == '='),
                    )
                }),
            Syntax::Git => line.split_once('='),
        }
        .unwrap_or((line, ""));
        (name.trim().to_lowercase(), value.trim())
    }

    fn header(self, line: &str) -> Option<String> {
        match self {
            Syntax::Ssh => {
                let (name, value) = self.split(line);
                let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
                match name.as_str() {
                    "host" => Some(format!("Host {value}")),
                    "match" => Some(format!("Match {value}")),
                    _ => None,
                }
            }
            Syntax::Git => line
                .starts_with('[')
                .then(|| line.split_whitespace().collect::<Vec<_>>().join(" ")),
        }
    }

    fn key(self, line: &str) -> String {
        let (name, value) = self.split(line);
        let lists: &[&str] = match self {
            Syntax::Ssh => &SSH_LISTS,
            Syntax::Git => &GIT_LISTS,
        };
        if lists.contains(&name.as_str()) {
            format!(
                "{name} {}",
                value.split_whitespace().collect::<Vec<_>>().join(" ")
            )
        } else {
            name
        }
    }

    /// An option that only makes sense on this machine: a path out of the home folder, a system's keychain.
    fn machine(self, block: &str, key: &str, text: &str) -> bool {
        let line = text.lines().map(str::trim).find(|l| !self.comment(l));
        let (name, value) = line.map_or((String::new(), ""), |l| self.split(l));
        crate::merge::machine_path(value)
            || match self {
                Syntax::Ssh => name == "usekeychain",
                Syntax::Git => block.starts_with("[credential") && key.starts_with("helper"),
            }
    }
}

fn parse(syntax: Syntax, text: &str) -> Items<Block> {
    let mut blocks = vec![(
        String::new(),
        Block {
            head: String::new(),
            options: Vec::new(),
        },
    )];
    let mut pending = String::new();
    for line in text.split_inclusive('\n') {
        let line = if line.ends_with('\n') {
            line.to_owned()
        } else {
            format!("{line}\n")
        };
        let trimmed = line.trim();
        let block = &mut blocks.last_mut().expect("one block at least").1;
        // git's trailing backslash: the next line is the same value.
        if pending.is_empty()
            && let Some((_, last)) = block.options.last_mut()
            && last.trim_end().ends_with('\\')
        {
            last.push_str(&line);
        } else if syntax.comment(trimmed) {
            pending.push_str(&line);
        } else if let Some(key) = syntax.header(trimmed) {
            let head = std::mem::take(&mut pending) + &line;
            let key = unique(&blocks, key);
            blocks.push((
                key,
                Block {
                    head,
                    options: Vec::new(),
                },
            ));
        } else {
            let key = unique(&block.options, syntax.key(trimmed));
            block
                .options
                .push((key, std::mem::take(&mut pending) + &line));
        }
    }
    let block = &mut blocks.last_mut().expect("one block at least").1;
    match block.options.last_mut() {
        Some((_, last)) => last.push_str(&pending),
        None => block.head.push_str(&pending),
    }
    blocks
}

/// A key seen again in the same list gets its rank: the second `Host x` is `Host x#2`.
fn unique<T>(items: &Items<T>, key: String) -> String {
    let n = items
        .iter()
        .filter(|(k, _)| *k == key || k.strip_prefix(&key).is_some_and(|r| r.starts_with('#')))
        .count();
    if n == 0 {
        key
    } else {
        format!("{key}#{}", n + 1)
    }
}

fn render(blocks: &Items<Block>) -> String {
    let mut out = String::new();
    for (_, block) in blocks {
        out.push_str(&block.head);
        for (_, text) in &block.options {
            out.push_str(text);
        }
    }
    out
}

/// Same meaning: the same settings, whatever their indentation, the blank lines and the comments around them. A
/// comment changed on one side alone still travels: the file differs, and is taken whole.
fn same_text(a: &str, b: &str) -> bool {
    fn meaning(s: &str) -> Vec<&str> {
        s.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with(['#', ';']))
            .collect()
    }
    meaning(a) == meaning(b)
}

fn same_block(a: &Block, b: &Block) -> bool {
    same_text(&a.head, &b.head)
        && a.options.len() == b.options.len()
        && a.options
            .iter()
            .zip(&b.options)
            .all(|((ka, a), (kb, b))| ka == kb && same_text(a, b))
}

/// Three-way merge of keyed items, as TOML keys are merged: `both` settles a key changed on both sides. This
/// machine's items keep their order; the other's new ones come right after the item they follow there, so a
/// `Host *` written last stays last.
fn keyed<T: Clone>(
    base: Option<&[(String, T)]>,
    local: &[(String, T)],
    remote: &[(String, T)],
    same: impl Fn(&T, &T) -> bool,
    mut both: impl FnMut(&str, Option<&T>, Option<&T>, Option<&T>) -> Option<T>,
) -> Items<T> {
    fn find<'a, T>(items: &'a [(String, T)], key: &str) -> Option<&'a T> {
        items.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    let eq = |a: Option<&T>, b: Option<&T>| match (a, b) {
        (Some(a), Some(b)) => same(a, b),
        (a, b) => a.is_none() && b.is_none(),
    };
    let mut pick = |key: &str| {
        let (b, l, r) = (
            base.and_then(|b| find(b, key)),
            find(local, key),
            find(remote, key),
        );
        if eq(l, r) || eq(r, b) {
            l.cloned()
        } else if eq(l, b) {
            r.cloned()
        } else {
            both(key, b, l, r)
        }
    };
    let mut out: Items<T> = Vec::new();
    for (key, _) in local {
        if let Some(value) = pick(key) {
            out.push((key.clone(), value));
        }
    }
    for (i, (key, _)) in remote.iter().enumerate() {
        if find(local, key).is_some() {
            continue;
        }
        let Some(value) = pick(key) else {
            continue;
        };
        let at = remote[..i]
            .iter()
            .rev()
            .find_map(|(p, _)| out.iter().position(|(k, _)| k == p))
            .map_or(0, |n| n + 1);
        out.insert(at, (key.clone(), value));
    }
    out
}

/// Blocks merged by their header, options by their name. A conflict is named by its block and option
/// (`Host acme › user`); `keep` settles it on one side instead.
pub fn merge(
    syntax: Syntax,
    base: Option<&str>,
    local: &str,
    remote: &str,
    keep: Option<bool>,
    conflicts: &mut Vec<String>,
) -> String {
    let base = base.map(|b| parse(syntax, b));
    let (local, remote) = (parse(syntax, local), parse(syntax, remote));
    let blocks = keyed(
        base.as_deref(),
        &local,
        &remote,
        same_block,
        |key, b, l, r| {
            let (Some(l), Some(r)) = (l, r) else {
                // Deleted on one side, changed on the other.
                return settle(keep, conflicts, key.to_owned(), l, r);
            };
            let options = keyed(
                b.map(|b| b.options.as_slice()),
                &l.options,
                &r.options,
                |a, b| same_text(a, b),
                |name, _, lo, ro| {
                    let label = if key.is_empty() {
                        name.to_owned()
                    } else {
                        format!("{key} › {name}")
                    };
                    settle(keep, conflicts, label, lo, ro)
                },
            );
            // The header's comments: the side that changed them, else the one that has more.
            let head = match b {
                Some(b) if l.head == b.head => r.head.clone(),
                None if r.head.len() > l.head.len() => r.head.clone(),
                _ => l.head.clone(),
            };
            Some(Block { head, options })
        },
    );
    render(&blocks)
}

fn settle<T: Clone>(
    keep: Option<bool>,
    conflicts: &mut Vec<String>,
    name: String,
    l: Option<&T>,
    r: Option<&T>,
) -> Option<T> {
    if keep.is_none() {
        conflicts.push(name);
    }
    if keep == Some(true) { r } else { l }.cloned()
}

/// [`crate::merge::pin`] for these files: each option only this machine may have becomes the synced one, or
/// leaves.
pub fn pin(syntax: Syntax, current: &str, base: Option<&str>) -> Option<String> {
    let mut blocks = parse(syntax, current);
    let base = base.map(|b| parse(syntax, b));
    let mut changed = false;
    for (key, block) in &mut blocks {
        let synced = base
            .as_ref()
            .and_then(|b| b.iter().find(|(k, _)| k == key))
            .map(|(_, b)| b);
        block.options.retain_mut(|(name, text)| {
            if !syntax.machine(key, name, text) {
                return true;
            }
            changed = true;
            match synced.and_then(|b| b.options.iter().find(|(n, _)| n == name)) {
                Some((_, theirs)) => {
                    text.clone_from(theirs);
                    true
                }
                None => false,
            }
        });
    }
    changed.then(|| render(&blocks))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merged(base: Option<&str>, local: &str, remote: &str) -> Result<String, Vec<String>> {
        let mut conflicts = Vec::new();
        let text = merge(Syntax::Ssh, base, local, remote, None, &mut conflicts);
        if conflicts.is_empty() {
            Ok(text)
        } else {
            Err(conflicts)
        }
    }

    #[test]
    fn hosts_of_both_machines_come_together() {
        let a = "Include ~/.orbstack/ssh/config\n\n# Work\nHost acme-prod\n  HostName prod.acme.test\n  User deploy\n\nHost *\n  ServerAliveInterval 60\n";
        let b = "Host box\n  HostName box.acme.test\n\nHost *\n  ServerAliveInterval 60\n";
        let text = merged(None, b, a).unwrap();
        assert_eq!(
            text,
            "Include ~/.orbstack/ssh/config\n\n# Work\nHost acme-prod\n  HostName prod.acme.test\n  User deploy\nHost box\n  HostName box.acme.test\n\nHost *\n  ServerAliveInterval 60\n"
        );
        // Each side changed another option of the same host: both kept.
        let base = "Host a\n  HostName a.test\n  User x\n";
        assert_eq!(
            merged(
                Some(base),
                "Host a\n  HostName a.test\n  User y\n",
                "Host a\n  HostName b.test\n  User x\n  Port 2222\n"
            )
            .unwrap(),
            "Host a\n  HostName b.test\n  User y\n  Port 2222\n"
        );
        // The same option set differently: a conflict named by its host.
        assert_eq!(
            merged(None, "Host a\n  User y\n", "host  a\n  User z\n"),
            Err(vec!["Host a › user".into()])
        );
        // Deleted here, changed there.
        assert_eq!(
            merged(Some(base), "", "Host a\n  HostName c.test\n  User x\n"),
            Err(vec!["Host a".into()])
        );
        // Identity files add up.
        assert_eq!(
            merged(
                None,
                "Host a\n  IdentityFile ~/.ssh/x\n",
                "Host a\n  IdentityFile ~/.ssh/y\n"
            )
            .unwrap(),
            "Host a\n  IdentityFile ~/.ssh/y\n  IdentityFile ~/.ssh/x\n"
        );
    }

    #[test]
    fn keeping_a_side_settles_only_the_conflict() {
        let mut conflicts = Vec::new();
        let text = merge(
            Syntax::Git,
            None,
            "[user]\n\tname = Alex\n\temail = a@b\n",
            "[user]\n\tname = Alex\n\temail = a@c\n[pull]\n\trebase = true\n",
            Some(true),
            &mut conflicts,
        );
        assert!(conflicts.is_empty());
        assert_eq!(
            text,
            "[user]\n\tname = Alex\n\temail = a@c\n[pull]\n\trebase = true\n"
        );
    }

    #[test]
    fn options_of_one_machine_stay_there() {
        let config = "Host *\n  UseKeychain yes\n  IdentityFile ~/.ssh/id\n  ProxyCommand /usr/local/bin/cloudflared %h\n";
        assert_eq!(
            pin(Syntax::Ssh, config, None).unwrap(),
            "Host *\n  IdentityFile ~/.ssh/id\n"
        );
        let git = "[credential]\n\thelper = osxkeychain\n[gpg]\n\tprogram = /opt/homebrew/bin/gpg\n[user]\n\tname = Alex\n";
        assert_eq!(
            pin(Syntax::Git, git, Some("[gpg]\n\tprogram = gpg\n")).unwrap(),
            "[credential]\n[gpg]\n\tprogram = gpg\n[user]\n\tname = Alex\n"
        );
        assert!(pin(Syntax::Git, "[user]\n\tname = Alex\n", None).is_none());
    }

    #[test]
    fn a_file_reads_back_as_it_was() {
        for text in [
            "",
            "# only a comment",
            "Host a\n  User x\n# trailing\n",
            "[alias]\n\tlg = log \\\n\t  --graph\n[core]\n\teditor = vim\n",
            "Host a\nHost a\n",
        ] {
            assert_eq!(
                render(&parse(Syntax::Git, text)).trim_end(),
                text.trim_end()
            );
            assert_eq!(
                render(&parse(Syntax::Ssh, text)).trim_end(),
                text.trim_end()
            );
        }
    }
}
