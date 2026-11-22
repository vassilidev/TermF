//! A project started from an idea rather than from repositories: its folder, a git repository on `main` with a
//! first commit (a README holding the idea, a `.gitignore`), its GitHub repository if asked, then the project.
//! `plan` says every command before `create` runs them, so the owner sees what will happen first.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

const GITIGNORE: &str = ".DS_Store\nThumbs.db\n.env\n.env.*\n!.env.example\n*.log\n";
const MESSAGE: &str = "Initial commit";

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Private,
    Public,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Spec {
    /// The project's name, also its folder's and its GitHub repository's.
    pub name: String,
    /// The folder the project's folder goes in.
    pub parent: String,
    pub idea: String,
    /// `None`: no GitHub repository.
    pub github: Option<Visibility>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct File {
    pub name: String,
    pub text: String,
}

/// What `create` will do, before it does it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub path: String,
    /// The folder is there already, empty: it is used as it is.
    pub exists: bool,
    pub files: Vec<File>,
    /// Each command as it runs, in the project's folder.
    pub commands: Vec<String>,
    /// The GitHub CLI answers on this machine.
    pub gh: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Created {
    pub project: String,
    pub path: String,
    /// The GitHub repository's URL, as `gh` gives it.
    pub remote: Option<String>,
    /// Why the GitHub repository was not created; the project is, without it.
    pub github_error: Option<String>,
}

/// The programs' environment: `PATH` as the login shell has it, so `gh` is found from an app opened in the Finder.
pub type Env = Vec<(String, OsString)>;

pub fn env() -> Env {
    termf_core::shell::login_path()
        .map(|p| vec![("PATH".to_owned(), p.to_owned())])
        .unwrap_or_default()
}

fn folder(spec: &Spec) -> Result<PathBuf> {
    let name = spec.name.trim();
    ensure!(!name.is_empty(), "the project needs a name");
    // Also a valid GitHub repository name, so the folder and the repository keep the same name.
    ensure!(
        name.len() <= 100
            && name != "."
            && name != ".."
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')),
        "the name \"{name}\" can only hold letters, digits, '-', '_' and '.'"
    );
    let parent = Path::new(spec.parent.trim());
    ensure!(
        parent.is_absolute() && parent.is_dir(),
        "the folder {} does not exist",
        parent.display()
    );
    Ok(parent.join(name))
}

/// `Some(false)`: free; `Some(true)`: an empty folder; an error for anything else there.
fn state(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let empty = path.is_dir() && fs::read_dir(path)?.next().is_none();
    ensure!(
        empty,
        "{} already exists and is not empty: choose another name or folder",
        path.display()
    );
    Ok(true)
}

fn readme(spec: &Spec) -> String {
    format!("# {}\n\n{}\n", spec.name.trim(), spec.idea.trim())
}

fn commands(spec: &Spec) -> Vec<Vec<String>> {
    let mut list: Vec<Vec<&str>> = vec![
        vec!["git", "init", "-q"],
        // Works with every git, `init -b` only since 2.28.
        vec!["git", "symbolic-ref", "HEAD", "refs/heads/main"],
        vec!["git", "add", "README.md", ".gitignore"],
        vec!["git", "commit", "-q", "-m", MESSAGE],
    ];
    let vis = match spec.github {
        Some(Visibility::Public) => "--public",
        _ => "--private",
    };
    let name = spec.name.trim();
    if spec.github.is_some() {
        list.push(vec![
            "gh", "repo", "create", name, vis, "--source", ".", "--remote", "origin", "--push",
        ]);
    }
    list.into_iter()
        .map(|c| c.into_iter().map(str::to_owned).collect())
        .collect()
}

fn gh_answers(env: &Env) -> bool {
    Command::new("gh")
        .arg("--version")
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// The project's folder, and whether it is there already (empty).
fn check(spec: &Spec) -> Result<(PathBuf, bool)> {
    ensure!(!spec.idea.trim().is_empty(), "describe your idea first");
    let path = folder(spec)?;
    let exists = state(&path)?;
    Ok((path, exists))
}

pub fn plan(spec: &Spec, env: &Env) -> Result<Plan> {
    let (path, exists) = check(spec)?;
    Ok(Plan {
        path: path.to_string_lossy().into_owned(),
        exists,
        files: vec![
            File {
                name: "README.md".into(),
                text: readme(spec),
            },
            File {
                name: ".gitignore".into(),
                text: GITIGNORE.into(),
            },
        ],
        commands: commands(spec).iter().map(|c| c.join(" ")).collect(),
        gh: gh_answers(env),
    })
}

fn run(dir: &Path, command: &[String], env: &Env) -> Result<String> {
    let out = Command::new(&command[0])
        .args(&command[1..])
        .current_dir(dir)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("cannot run {}", command[0]))?;
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        bail!("`{}` failed: {said}", command.join(" "));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn local(spec: &Spec, path: &Path, local: &[Vec<String>], env: &Env) -> Result<()> {
    fs::write(path.join("README.md"), readme(spec))?;
    fs::write(path.join(".gitignore"), GITIGNORE)?;
    for command in local {
        run(path, command, env)?;
    }
    Ok(())
}

/// Creates the repository, then the project in `config_dir`. A GitHub repository that cannot be created leaves the
/// local one and the project as they are, and says why.
pub fn create(config_dir: &Path, spec: &Spec, env: &Env) -> Result<Created> {
    let (path, existed) = check(spec)?;
    let mut all = commands(spec);
    let github = spec.github.is_some().then(|| all.pop()).flatten();
    if !existed {
        fs::create_dir(&path).with_context(|| format!("cannot create {}", path.display()))?;
    }
    if let Err(e) = local(spec, &path, &all, env) {
        // Leaves the folder as it was found.
        let _ = if existed {
            [".git", "README.md", ".gitignore"]
                .iter()
                .map(|n| path.join(n))
                .filter(|p| p.exists())
                .try_for_each(|p| {
                    if p.is_dir() {
                        fs::remove_dir_all(p)
                    } else {
                        fs::remove_file(p)
                    }
                })
        } else {
            fs::remove_dir_all(&path)
        };
        return Err(e);
    }
    let (remote, github_error) = match github.map(|c| run(&path, &c, env)) {
        None => (None, None),
        Some(Ok(out)) => (
            out.lines().find(|l| l.contains("://")).map(str::to_owned),
            None,
        ),
        Some(Err(e)) => (None, Some(format!("{e:#}"))),
    };
    let dir = path.to_string_lossy().into_owned();
    let project = termf_core::save_project(
        config_dir,
        None,
        spec.name.trim(),
        std::slice::from_ref(&dir),
    )?;
    Ok(Created {
        project,
        path: dir,
        remote,
        github_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("termf-greenfield-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("home/Projects")).unwrap();
        fs::create_dir_all(dir.join("config")).unwrap();
        dir
    }

    /// A throwaway HOME and git identity: never the real config.
    fn env_in(dir: &Path, path: Option<OsString>) -> Env {
        let mut env: Env = [
            ("HOME", dir.join("home").into_os_string()),
            ("GIT_CONFIG_NOSYSTEM", "1".into()),
            (
                "GIT_CONFIG_GLOBAL",
                dir.join("home/.gitconfig").into_os_string(),
            ),
            ("GIT_AUTHOR_NAME", "alex".into()),
            ("GIT_AUTHOR_EMAIL", "alex@acme.test".into()),
            ("GIT_COMMITTER_NAME", "alex".into()),
            ("GIT_COMMITTER_EMAIL", "alex@acme.test".into()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        env.extend(path.map(|p| ("PATH".to_owned(), p)));
        env
    }

    fn spec(dir: &Path, github: Option<Visibility>) -> Spec {
        Spec {
            name: "plant-care".into(),
            parent: dir.join("home/Projects").to_string_lossy().into_owned(),
            idea: "Track when to water each house plant.".into(),
            github,
        }
    }

    fn git(repo: &Path, args: &[&str], env: &Env) -> String {
        let args: Vec<String> = std::iter::once("git")
            .chain(args.iter().copied())
            .map(str::to_owned)
            .collect();
        run(repo, &args, env).unwrap()
    }

    #[test]
    fn creates_the_folder_the_repository_on_main_its_first_commit_and_the_project() {
        let dir = temp("create");
        let env = env_in(&dir, None);
        let spec = spec(&dir, None);
        let plan = plan(&spec, &env).unwrap();
        assert!(!plan.exists);
        assert!(plan.commands.iter().all(|c| !c.starts_with("gh")));
        let made = create(&dir.join("config"), &spec, &env).unwrap();
        let repo = dir.join("home/Projects/plant-care");
        assert_eq!(made.path, repo.to_string_lossy());
        assert_eq!(made.project, "plant-care");
        assert_eq!((made.remote, made.github_error), (None, None));
        assert_eq!(git(&repo, &["branch", "--show-current"], &env), "main");
        assert_eq!(git(&repo, &["log", "--format=%s"], &env), MESSAGE);
        assert_eq!(
            git(&repo, &["ls-files"], &env).lines().collect::<Vec<_>>(),
            [".gitignore", "README.md"]
        );
        let readme = fs::read_to_string(repo.join("README.md")).unwrap();
        assert!(readme.starts_with("# plant-care\n") && readme.contains("house plant"));
        let projects = termf_core::load_projects(&dir.join("config")).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "plant-care");
        assert_eq!(projects[0].repos[0].path, made.path);
        assert!(
            create(&dir.join("config"), &spec, &env)
                .unwrap_err()
                .to_string()
                .contains("not empty"),
            "the project's folder is never written twice"
        );
    }

    #[test]
    fn refuses_a_folder_that_is_not_empty_and_uses_an_empty_one() {
        let dir = temp("taken");
        let env = env_in(&dir, None);
        let spec = spec(&dir, None);
        let repo = dir.join("home/Projects/plant-care");
        fs::create_dir(&repo).unwrap();
        assert!(plan(&spec, &env).unwrap().exists);
        fs::write(repo.join("notes.txt"), "mine").unwrap();
        let e = create(&dir.join("config"), &spec, &env).unwrap_err();
        assert!(e.to_string().contains("not empty"), "{e}");
        assert_eq!(fs::read_to_string(repo.join("notes.txt")).unwrap(), "mine");
        assert!(!repo.join(".git").exists());
        assert!(
            termf_core::load_projects(&dir.join("config"))
                .unwrap()
                .is_empty()
        );
        for (name, parent) in [
            ("../up", "home/Projects"),
            ("plant care", "home/Projects"),
            ("ok", "nowhere"),
        ] {
            let bad = Spec {
                name: name.into(),
                parent: dir.join(parent).to_string_lossy().into_owned(),
                ..spec.clone()
            };
            assert!(plan(&bad, &env).is_err(), "{name} in {parent}");
        }
        let idle = Spec {
            idea: " ".into(),
            ..spec
        };
        assert!(plan(&idle, &env).is_err(), "an idea is needed");
    }

    #[cfg(unix)]
    fn fake_gh(dir: &Path, script: &str) -> Option<OsString> {
        use std::os::unix::fs::PermissionsExt;
        let bin = dir.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let gh = bin.join("gh");
        fs::write(&gh, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut all = bin.into_os_string();
        all.push(":");
        all.push(path);
        Some(all)
    }

    #[cfg(unix)]
    #[test]
    fn creates_the_github_repository_private_by_default_and_pushes_the_first_commit() {
        let dir = temp("gh");
        let log = dir.join("gh.log");
        let script = format!(
            "[ \"$1\" = --version ] && exit 0\necho \"$@\" > {}\ngit rev-parse --abbrev-ref HEAD >> {}\necho https://github.com/alex/plant-care",
            log.display(),
            log.display()
        );
        let env = env_in(&dir, fake_gh(&dir, &script));
        let spec = spec(&dir, Some(Visibility::Private));
        let plan = plan(&spec, &env).unwrap();
        assert!(plan.gh);
        assert_eq!(
            plan.commands.last().unwrap(),
            "gh repo create plant-care --private --source . --remote origin --push"
        );
        let made = create(&dir.join("config"), &spec, &env).unwrap();
        assert_eq!(
            made.remote.as_deref(),
            Some("https://github.com/alex/plant-care")
        );
        assert_eq!(made.github_error, None);
        assert_eq!(
            fs::read_to_string(&log).unwrap(),
            "repo create plant-care --private --source . --remote origin --push\nmain\n",
            "after the first commit, from the project's folder"
        );
        let public = Spec {
            github: Some(Visibility::Public),
            ..spec
        };
        assert!(
            commands(&public)
                .last()
                .unwrap()
                .contains(&"--public".to_owned())
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_github_repository_that_fails_leaves_the_local_project() {
        let dir = temp("gh-fails");
        let script =
            "[ \"$1\" = --version ] && exit 0\necho 'HTTP 401: Bad credentials' >&2\nexit 1";
        let env = env_in(&dir, fake_gh(&dir, script));
        let made = create(
            &dir.join("config"),
            &spec(&dir, Some(Visibility::Private)),
            &env,
        )
        .unwrap();
        assert_eq!(made.remote, None);
        assert!(made.github_error.unwrap().contains("Bad credentials"));
        let repo = dir.join("home/Projects/plant-care");
        assert_eq!(git(&repo, &["log", "--format=%s"], &env), MESSAGE);
        assert_eq!(
            termf_core::load_projects(&dir.join("config"))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn a_failed_commit_leaves_no_folder_behind() {
        let dir = temp("no-identity");
        // No identity anywhere: git refuses to commit.
        let env: Env = env_in(&dir, None)
            .into_iter()
            .filter(|(k, _)| !k.starts_with("GIT_AUTHOR") && !k.starts_with("GIT_COMMITTER"))
            .chain([("EMAIL".to_owned(), OsString::new())])
            .collect();
        fs::write(
            dir.join("home/.gitconfig"),
            "[user]\n\tuseConfigOnly = true\n",
        )
        .unwrap();
        let e = create(&dir.join("config"), &spec(&dir, None), &env).unwrap_err();
        assert!(e.to_string().contains("git commit"), "{e:#}");
        assert!(!dir.join("home/Projects/plant-care").exists());
        assert!(
            termf_core::load_projects(&dir.join("config"))
                .unwrap()
                .is_empty()
        );
    }
}
