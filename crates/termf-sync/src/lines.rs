//! `~/.ssh/config` and `~/.gitconfig` merged as what they are: blocks (`Host`, `Match`, `[section]`, and what
//! comes before the first one), each a list of options. Comments and blank lines go with the line below them,
//! so they stay where they were; a file is only rewritten where something changed.

use std::collections::BTreeSet;

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
const GIT_LISTS: [&str; 6] = [
    "path",
    "fetch",
    "push",
    "insteadof",
    "pushinsteadof",
    "directory",
];

type Items<T> = Vec<(String, T)>;

#[derive(Clone, PartialEq)]
struct Block {
    head: String,
    options: Items<String>,
}

fn words(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
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

    /// The setting of an option's text, below its comments.
    fn directive(self, text: &str) -> &str {
        text.lines()
            .map(str::trim)
            .find(|l| !self.comment(l))
            .unwrap_or("")
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
                match name.as_str() {
                    "host" => Some(format!("Host {}", words(value))),
                    "match" => Some(format!("Match {}", words(value))),
                    _ => None,
                }
            }
            Syntax::Git => line.starts_with('[').then(|| words(line)),
        }
    }

    /// An option's name, or its name and value when it adds up ([`read_all`] gives the others their value too).
    fn key(self, line: &str) -> String {
        let (name, value) = self.split(line);
        let lists: &[&str] = match self {
            Syntax::Ssh => &SSH_LISTS,
            Syntax::Git => &GIT_LISTS,
        };
        if lists.contains(&name.as_str()) {
            format!("{name} {}", words(value))
        } else {
            name
        }
    }

    /// An option that only makes sense on this machine: a path out of the home folder, a system's keychain.
    fn machine(self, block: &str, key: &str, text: &str) -> bool {
        let (name, value) = self.split(self.directive(text));
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
            let key = syntax.key(trimmed);
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

/// The versions of a file, read together: an option given more than once in a block of any of them is known by
/// its value too, in all of them, so the same line is the same option on every side whatever its rank.
fn read_all<const N: usize>(syntax: Syntax, texts: [Option<&str>; N]) -> [Option<Items<Block>>; N] {
    let mut docs = texts.map(|t| t.map(|t| parse(syntax, t)));
    let mut repeated = BTreeSet::new();
    for (block, b) in docs.iter().flatten().flatten() {
        let mut seen = BTreeSet::new();
        for (key, _) in &b.options {
            if !seen.insert(key) {
                repeated.insert((block.clone(), key.clone()));
            }
        }
    }
    for (block, b) in docs.iter_mut().flatten().flatten() {
        let mut taken: Items<()> = Vec::new();
        for (key, text) in &mut b.options {
            // A list's key has its value already.
            if !key.contains(' ') && repeated.contains(&(block.clone(), key.clone())) {
                let value = syntax.split(syntax.directive(text)).1;
                *key = format!("{key} {}", words(value));
            }
            *key = unique(&taken, std::mem::take(key));
            taken.push((key.clone(), ()));
        }
    }
    docs
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

/// Same setting: the same lines, whatever their indentation, the blank lines and the comments around them.
fn same_text(a: &str, b: &str) -> bool {
    fn meaning(s: &str) -> Vec<&str> {
        s.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with(['#', ';']))
            .collect()
    }
    meaning(a) == meaning(b)
}

/// The comments and blank lines above a line, and the rest.
fn cut(syntax: Syntax, text: &str) -> (&str, &str) {
    let at: usize = text
        .split_inclusive('\n')
        .take_while(|l| syntax.comment(l.trim()))
        .map(str::len)
        .sum();
    text.split_at(at)
}

/// Three-way merge of a line and the comments above it. The line as any setting: `label` names the conflict when
/// both sides set it differently (`None`: a header, its block's key, whose spelling never conflicts). The
/// comments: the side that changed them, both when both did.
fn piece(
    syntax: Syntax,
    label: Option<&str>,
    base: Option<&str>,
    local: &str,
    remote: &str,
    keep: Option<bool>,
    conflicts: &mut Vec<String>,
) -> String {
    let ((lc, ll), (rc, rl)) = (cut(syntax, local), cut(syntax, remote));
    let (bc, bl) = base.map(|b| cut(syntax, b)).unzip();
    let line = if same_text(ll, rl) || bl.is_some_and(|b| same_text(rl, b)) {
        ll
    } else if bl.is_some_and(|b| same_text(ll, b)) {
        rl
    } else {
        match label {
            None => ll,
            Some(label) => {
                settle(keep, conflicts, label.to_owned(), Some(&ll), Some(&rl)).expect("both sides")
            }
        }
    };
    let comments = if lc == rc || bc == Some(rc) {
        lc.to_owned()
    } else if bc == Some(lc) {
        rc.to_owned()
    } else {
        let mut both = lc.to_owned();
        for line in rc.split_inclusive('\n') {
            if !line.trim().is_empty() && !lc.lines().any(|l| l.trim() == line.trim()) {
                both.push_str(line);
            }
        }
        both
    };
    comments + line
}

fn find<'a, T>(items: &'a [(String, T)], key: &str) -> Option<&'a T> {
    items.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// The keys of `items` that all three versions have, in `items`' order.
fn order<'a, T>(items: &'a [(String, T)], all: [&[(String, T)]; 3]) -> Vec<&'a str> {
    items
        .iter()
        .map(|(k, _)| k.as_str())
        .filter(|k| all.iter().all(|items| find(items, k).is_some()))
        .collect()
}

/// Three-way merge of keyed items, as TOML keys are merged: `both` settles a key changed on both sides. The
/// order is this machine's, or the other's when only that one changed it; what only one side has comes after
/// the rest, before what `last` keeps at the end (`Host *`: ssh takes the first value it finds).
fn keyed<T: Clone + PartialEq>(
    base: Option<&[(String, T)]>,
    local: &[(String, T)],
    remote: &[(String, T)],
    last: impl Fn(&str) -> bool,
    mut both: impl FnMut(&str, Option<&T>, Option<&T>, Option<&T>) -> Option<T>,
) -> Items<T> {
    let mut pick = |key: &str| {
        let (b, l, r) = (
            base.and_then(|b| find(b, key)),
            find(local, key),
            find(remote, key),
        );
        if l == r || r == b {
            l.cloned()
        } else if l == b {
            r.cloned()
        } else {
            both(key, b, l, r)
        }
    };
    let (first, then) = match base {
        Some(b)
            if order(local, [b, local, remote]) == order(b, [b, local, remote])
                && order(remote, [b, local, remote]) != order(b, [b, local, remote]) =>
        {
            (remote, local)
        }
        _ => (local, remote),
    };
    let mut out: Items<T> = Vec::new();
    for (key, _) in first {
        if let Some(value) = pick(key) {
            out.push((key.clone(), value));
        }
    }
    for (key, _) in then {
        if find(first, key).is_some() {
            continue;
        }
        if let Some(value) = pick(key) {
            let at = out.iter().rposition(|(k, _)| !last(k)).map_or(0, |i| i + 1);
            out.insert(at, (key.clone(), value));
        }
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
    let [local, remote, base] = read_all(syntax, [Some(local), Some(remote), base]);
    let (local, remote) = (local.expect("given"), remote.expect("given"));
    let catch_all = |key: &str| key == "Host *" || key == "Match all";
    let blocks = keyed(
        base.as_deref(),
        &local,
        &remote,
        catch_all,
        |key, b, l, r| {
            let (Some(l), Some(r)) = (l, r) else {
                // Deleted on one side, changed on the other.
                return settle(keep, conflicts, key.to_owned(), l, r);
            };
            let base_head = b.map(|b| b.head.as_str());
            let head = piece(syntax, None, base_head, &l.head, &r.head, keep, conflicts);
            let options = keyed(
                b.map(|b| b.options.as_slice()),
                &l.options,
                &r.options,
                |_| false,
                |name, bo, lo, ro| {
                    let label = if key.is_empty() {
                        name.to_owned()
                    } else {
                        format!("{key} › {name}")
                    };
                    match (lo, ro) {
                        (Some(lo), Some(ro)) => {
                            let bo = bo.map(String::as_str);
                            Some(piece(syntax, Some(&label), bo, lo, ro, keep, conflicts))
                        }
                        _ => settle(keep, conflicts, label, lo, ro),
                    }
                },
            );
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
    let [blocks, base] = read_all(syntax, [Some(current), base]);
    let mut blocks = blocks.expect("given");
    let mut changed = false;
    for (key, block) in &mut blocks {
        let synced = base.as_ref().and_then(|b| find(b, key));
        block.options.retain_mut(|(name, text)| {
            if !syntax.machine(key, name, text) {
                return true;
            }
            changed = true;
            match synced.and_then(|b| find(&b.options, name)) {
                Some(theirs) => {
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
        merged_as(Syntax::Ssh, base, local, remote)
    }

    fn merged_as(
        syntax: Syntax,
        base: Option<&str>,
        local: &str,
        remote: &str,
    ) -> Result<String, Vec<String>> {
        let mut conflicts = Vec::new();
        let text = merge(syntax, base, local, remote, None, &mut conflicts);
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
        // The other's new host after this machine's, before the catch-all.
        assert_eq!(
            merged(None, b, a).unwrap(),
            "Include ~/.orbstack/ssh/config\nHost box\n  HostName box.acme.test\n\n# Work\nHost acme-prod\n  HostName prod.acme.test\n  User deploy\n\nHost *\n  ServerAliveInterval 60\n"
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
        // Identity files add up, this machine's first.
        assert_eq!(
            merged(
                None,
                "Host a\n  IdentityFile ~/.ssh/x\n",
                "Host a\n  IdentityFile ~/.ssh/y\n"
            )
            .unwrap(),
            "Host a\n  IdentityFile ~/.ssh/x\n  IdentityFile ~/.ssh/y\n"
        );
    }

    #[test]
    fn the_order_is_kept_and_changed_only_by_who_changed_it() {
        let base = "Host a\n  User a\nHost b\n  User b\nHost *\n  User all\n";
        // Only the other reordered, this one changed an option: the other's order, this one's option.
        assert_eq!(
            merged(
                Some(base),
                "Host a\n  User a2\nHost b\n  User b\nHost *\n  User all\n",
                "Host b\n  User b\nHost a\n  User a\nHost *\n  User all\n"
            )
            .unwrap(),
            "Host b\n  User b\nHost a\n  User a2\nHost *\n  User all\n"
        );
        // Both reordered: this machine's order.
        assert_eq!(
            merged(
                Some(base),
                "Host *\n  User all\nHost a\n  User a\nHost b\n  User b\n",
                "Host b\n  User b\nHost a\n  User a\nHost *\n  User all\nHost c\n  User c\n"
            )
            .unwrap(),
            "Host *\n  User all\nHost a\n  User a\nHost b\n  User b\nHost c\n  User c\n"
        );
    }

    #[test]
    fn comments_stay_whoever_changed_the_setting() {
        let base = "Host prod\n  HostName a.test\n  User deploy\n";
        assert_eq!(
            merged(
                Some(base),
                "# careful: production\nHost prod\n  HostName a.test\n  # the deploy user\n  User deploy\n",
                "Host prod\n  HostName b.test\n  User ops\n"
            )
            .unwrap(),
            "# careful: production\nHost prod\n  HostName b.test\n  # the deploy user\n  User ops\n"
        );
        // Without a common history, the comments of both stay.
        assert_eq!(
            merged(
                None,
                "# mine\nHost prod\n  User deploy\n",
                "# theirs\nHost prod\n  User deploy\n"
            )
            .unwrap(),
            "# mine\n# theirs\nHost prod\n  User deploy\n"
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
    fn an_option_given_twice_is_known_by_its_value() {
        let both = "[tool]\n\tplugin = /opt/acme/a.so\n\tplugin = ~/b.so\n";
        assert_eq!(
            pin(Syntax::Git, both, None).unwrap(),
            "[tool]\n\tplugin = ~/b.so\n"
        );
        // Read with a version that has it twice, the single one is the same option, whatever its rank.
        assert_eq!(
            pin(Syntax::Git, both, Some("[tool]\n\tplugin = ~/b.so\n")).unwrap(),
            "[tool]\n\tplugin = ~/b.so\n"
        );
        assert_eq!(
            merged_as(
                Syntax::Git,
                Some("[tool]\n\tplugin = ~/b.so\n"),
                "[tool]\n\tplugin = ~/b.so\n",
                "[tool]\n\tplugin = ~/b.so\n\tplugin = ~/c.so\n"
            )
            .unwrap(),
            "[tool]\n\tplugin = ~/b.so\n\tplugin = ~/c.so\n"
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
            for syntax in [Syntax::Git, Syntax::Ssh] {
                let [doc] = read_all(syntax, [Some(text)]);
                assert_eq!(render(&doc.unwrap()).trim_end(), text.trim_end());
            }
        }
    }
}
