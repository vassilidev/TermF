//! What a run's session may not run: a Claude Code `PreToolUse` hook on Bash calls `termf guard`, which refuses
//! pushes, pull requests, writes to the tracker, the TermF commands that deliver and starting or cancelling CI
//! runs. Defense in depth, not a
//! guarantee: a script the agent writes then runs, a git alias or a tool other than Bash still gets through.

const REASON: &str = "In a TermF run, pushing, pull requests and writes to the tracker wait until the user \
                      delivers the run. Commit your work if you like, then tell the user it is ready.";
const CI_REASON: &str =
    "In a TermF run, the user starts and cancels the CI: tell them what to run, and why.";

/// Why the Bash command `line` is refused in a run's session, or `None` when it may run.
pub fn guard(line: &str) -> Option<String> {
    for words in commands(line) {
        for (i, word) in words.iter().enumerate() {
            let rest = &words[i + 1..];
            if drives_ci(program(word), rest) {
                return Some(format!(
                    "`{}` is refused. {CI_REASON}",
                    words[i..].join(" ")
                ));
            }
            let refused = match program(word) {
                "git" => git_subcommand(rest) == Some("push"),
                "git-push" => true,
                "gh" => gh_writes(rest),
                "termf" => matches!(
                    rest.first().map(String::as_str),
                    Some("deliver" | "close" | "run")
                ),
                "sh" | "bash" | "zsh" | "dash" | "ksh" => {
                    if let Some(reason) = shell_script(rest).and_then(guard) {
                        return Some(reason);
                    }
                    false
                }
                "eval" => {
                    if let Some(reason) = guard(&rest.join(" ")) {
                        return Some(reason);
                    }
                    false
                }
                _ => false,
            };
            if refused {
                return Some(format!("`{}` is refused. {REASON}", words[i..].join(" ")));
            }
            // A command substitution inside double quotes still runs.
            let inner = word
                .find("$(")
                .map(|at| at + 2)
                .or_else(|| word.find('`').map(|at| at + 1));
            if let Some(reason) = inner.and_then(|at| guard(&word[at..])) {
                return Some(reason);
            }
        }
    }
    None
}

/// `/usr/bin/git` and `git.exe` are `git`.
fn program(word: &str) -> &str {
    let name = word.rsplit(['/', '\\']).next().unwrap_or(word);
    name.strip_suffix(".exe").unwrap_or(name)
}

/// The subcommand after git's own options, some of which take a value: `git -C dir -c k=v push`.
fn git_subcommand(args: &[String]) -> Option<&str> {
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-C" | "-c" | "--git-dir" | "--work-tree" | "--namespace" | "--config-env"
            | "--super-prefix" => {
                args.next();
            }
            option if option.starts_with('-') => {}
            subcommand => return Some(subcommand),
        }
    }
    None
}

fn gh_writes(args: &[String]) -> bool {
    match args.first().map(String::as_str) {
        Some("pr") => true,
        Some("issue") => !matches!(
            args.get(1).map(String::as_str),
            None | Some("list" | "view" | "status")
        ),
        Some("api") => api_method(&args[1..]) != "GET",
        _ => false,
    }
}

/// `gh run rerun|cancel|delete…`, `gh workflow run|enable|disable`, `termf ci rerun|cancel|dispatch`: reading
/// the CI stays allowed.
fn drives_ci(program: &str, args: &[String]) -> bool {
    let arg = |i: usize| args.get(i).map(String::as_str);
    match (program, arg(0)) {
        ("gh", Some("run")) => {
            !matches!(arg(1), None | Some("list" | "view" | "watch" | "download"))
        }
        ("gh", Some("workflow")) => !matches!(arg(1), None | Some("list" | "view")),
        ("termf", Some("ci")) => matches!(arg(1), Some("rerun" | "cancel" | "dispatch")),
        _ => false,
    }
}

/// The method `gh api` sends: `-X`, else POST as soon as it has fields or a body, else GET.
fn api_method(args: &[String]) -> String {
    let (mut method, mut body) = (None, false);
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let arg = arg.as_str();
        match arg {
            "-X" | "--method" => method = args.next().cloned(),
            "-f" | "-F" | "--field" | "--raw-field" | "--input" => {
                body = true;
                args.next();
            }
            _ if arg.starts_with("--method=") => method = Some(arg["--method=".len()..].into()),
            _ if arg.starts_with("-X") => method = Some(arg[2..].into()),
            _ if ["--field=", "--raw-field=", "--input=", "-f", "-F"]
                .iter()
                .any(|p| arg.starts_with(p)) =>
            {
                body = true
            }
            _ => {}
        }
    }
    match method {
        Some(method) => method.to_ascii_uppercase(),
        None if body => "POST".into(),
        None => "GET".into(),
    }
}

/// The script of `bash -c '…'`, `sh -lc '…'`.
fn shell_script(args: &[String]) -> Option<&str> {
    let flag = args
        .iter()
        .position(|a| a.starts_with('-') && !a.starts_with("--") && a.contains('c'))?;
    args.get(flag + 1).map(String::as_str)
}

#[derive(Default)]
struct Split {
    commands: Vec<Vec<String>>,
    words: Vec<String>,
    word: String,
    /// The word has begun, even if empty (`''`).
    started: bool,
}

impl Split {
    fn end_word(&mut self) {
        if self.started || !self.word.is_empty() {
            self.words.push(std::mem::take(&mut self.word));
        }
        self.started = false;
    }

    fn end_command(&mut self) {
        self.end_word();
        if !self.words.is_empty() {
            self.commands.push(std::mem::take(&mut self.words));
        }
    }
}

/// The simple commands of a shell line and their words, as sh splits them in common cases: quotes, backslashes,
/// and the operators between commands. Nothing is expanded.
fn commands(line: &str) -> Vec<Vec<String>> {
    let mut split = Split::default();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                split.started = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    split.word.push(c);
                }
            }
            '"' => {
                split.started = true;
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' if chars.peek().is_some_and(|n| "\"\\$`".contains(*n)) => {
                            split.word.extend(chars.next());
                        }
                        c => split.word.push(c),
                    }
                }
            }
            '\\' => match chars.next() {
                Some('\n') | None => {}
                Some(n) => split.word.push(n),
            },
            '$' if chars.peek() == Some(&'(') => {
                chars.next();
                split.end_command();
            }
            ';' | '&' | '|' | '(' | ')' | '`' | '\n' => split.end_command(),
            c if c.is_whitespace() || c == '<' || c == '>' => split.end_word(),
            c => split.word.push(c),
        }
    }
    split.end_command();
    split.commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_shell_lines() {
        assert_eq!(
            commands(r#"cd 'a b' && git "x y"\ z; echo $(date)|wc"#),
            [
                vec!["cd", "a b"],
                vec!["git", "x y z"],
                vec!["echo"],
                vec!["date"],
                vec!["wc"]
            ]
        );
        assert_eq!(commands("echo '' >out"), [vec!["echo", "", "out"]]);
    }

    #[test]
    fn refuses_pushes_and_tracker_writes() {
        for refused in [
            "git push",
            "git push origin main --force",
            "git -C . push",
            "git -c user.name=x -c core.sshCommand='ssh -v' push",
            "/usr/bin/git --no-pager push",
            "git.exe push",
            "GIT_TRACE=1 git push",
            "cd x && git push",
            "git status; git push",
            "sudo git push",
            "timeout 10 git push",
            "bash -lc 'git push'",
            "sh -c \"cd . && git push\"",
            "eval git push",
            "echo \"$(git push)\"",
            "echo `git push`",
            "x=$(git push)",
            "git-push origin",
            "gh pr create --fill",
            "gh pr merge 1",
            "gh api -X POST repos/a/b/issues",
            "gh api -XDELETE repos/a/b",
            "gh api --method=patch repos/a/b/issues/1",
            "gh api repos/a/b/issues/1/comments -f body=hi",
            "gh api repos/a/b/issues/1/comments --raw-field=body=hi",
            "gh api repos/a/b/issues --input body.json",
            "gh issue comment 42 --body hi",
            "gh issue edit 42 --title x",
            "gh issue close 42",
            "gh issue create",
            "termf deliver 3",
            "termf close 3 --force",
            "termf run '#4'",
            "/Applications/TermF.app/Contents/MacOS/termf deliver 1",
            "gh run rerun 5 --failed",
            "gh run cancel 5 -R acme/api",
            "gh run delete 5",
            "gh workflow run deploy.yml -f level=high",
            "gh workflow enable ci.yml",
            "termf ci rerun 5",
            "termf ci cancel 5 --repo .",
            "cd api && termf ci dispatch deploy.yml --input level=high",
            "bash -c 'gh run rerun 5'",
        ] {
            let reason = guard(refused).unwrap_or_else(|| panic!("{refused} let through"));
            assert!(reason.contains("is refused"), "{reason}");
        }
        let ci = guard("gh workflow run ci.yml").unwrap();
        assert!(ci.contains("the user starts and cancels the CI"), "{ci}");
    }

    #[test]
    fn lets_other_commands_run() {
        for allowed in [
            "git status",
            "git log --grep push",
            "git -C push status",
            "git commit -m 'git push later'",
            "git commit -m \"fix: push button\"",
            "git push-notes",
            "gh issue view 42",
            "gh issue list --assignee @me",
            "gh api repos/a/b/issues/1",
            "gh api -X GET search/issues -f q=x",
            "gh repo view",
            "termf ls",
            "termf runs 3",
            "gh run list --branch main",
            "gh run view 5 --log-failed",
            "gh run watch 5",
            "gh workflow list",
            "termf ci runs --pr 7",
            "termf ci logs 9 --errors",
            "cargo test --workspace",
            "echo push",
            "grep -rn 'git push' docs",
            "",
        ] {
            assert_eq!(guard(allowed), None, "{allowed}");
        }
    }
}
