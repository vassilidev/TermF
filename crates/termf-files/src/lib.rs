use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, RwLock};
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result, anyhow, ensure};
use grep_regex::RegexMatcherBuilder;
use grep_searcher::sinks::Lossy;
use grep_searcher::{BinaryDetection, SearcherBuilder};
use ignore::WalkBuilder;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};
use serde::Serialize;

/// Larger files are neither opened in the editor nor searched.
pub const MAX_EDIT_BYTES: u64 = 5 * 1024 * 1024;
pub const MAX_HITS: usize = 1_000;
const MAX_LINE_CHARS: usize = 300;
const MAX_INDEX_FILES: usize = 200_000;
const FIND_LIMIT: usize = 50;

#[derive(Serialize, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    /// Relative to the repository, `/`-separated.
    pub path: String,
    pub dir: bool,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Content {
    Text { text: String },
    Binary,
    TooLarge { size: u64 },
}

#[derive(Serialize, Debug)]
pub struct Opened {
    #[serde(flatten)]
    pub content: Content,
    /// Milliseconds since the Unix epoch, handed back on save to detect changes made on disk meanwhile.
    pub mtime: u64,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Saved {
    Saved { mtime: u64 },
    Conflict,
}

#[derive(Serialize, Debug)]
pub struct Hit {
    pub path: String,
    pub line: u64,
    pub text: String,
}

#[derive(Serialize, Debug)]
pub struct SearchResult {
    pub hits: Vec<Hit>,
    pub truncated: bool,
}

/// File access for the UI, limited to the known repositories.
#[derive(Default)]
pub struct Files {
    roots: RwLock<Vec<PathBuf>>,
    index: Mutex<HashMap<PathBuf, Arc<Vec<String>>>>,
}

impl Files {
    /// Replaces the repositories the UI may reach; paths that don't exist are dropped.
    pub fn set_roots<P: AsRef<Path>>(&self, repos: impl IntoIterator<Item = P>) {
        *self.roots.write().unwrap() = repos
            .into_iter()
            .filter_map(|p| p.as_ref().canonicalize().ok())
            .collect();
        self.index.lock().unwrap().clear();
    }

    fn root(&self, repo: &str) -> Result<PathBuf> {
        let root = Path::new(repo)
            .canonicalize()
            .with_context(|| format!("cannot open {repo}"))?;
        ensure!(
            self.roots.read().unwrap().contains(&root),
            "{repo} is not a known repository"
        );
        Ok(root)
    }

    /// Canonicalizes `rel` inside `repo`, so `..`, absolute paths and symlinks cannot leave the repository.
    /// `.git` is refused too: writing its hooks or config would run code.
    pub fn resolve(&self, repo: &str, rel: &str) -> Result<(PathBuf, PathBuf)> {
        let root = self.root(repo)?;
        let path = root
            .join(rel)
            .canonicalize()
            .with_context(|| format!("cannot open {rel}"))?;
        let inside = path
            .strip_prefix(&root)
            .map_err(|_| anyhow!("{rel} is outside the repository"))?;
        ensure!(
            !inside.components().any(|c| c.as_os_str() == ".git"),
            "{rel} is inside .git"
        );
        Ok((root, path))
    }

    /// Direct children of `dir`, folders first, without git-ignored entries.
    pub fn list(&self, repo: &str, dir: &str) -> Result<Vec<Entry>> {
        let (_, path) = self.resolve(repo, dir)?;
        let mut entries: Vec<_> = walker(&path)
            .max_depth(Some(1))
            .build()
            .filter_map(Result::ok)
            .filter(|e| e.depth() == 1)
            .map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                Entry {
                    path: if dir.is_empty() {
                        name.clone()
                    } else {
                        format!("{dir}/{name}")
                    },
                    dir: e.path().is_dir(),
                    name,
                }
            })
            .collect();
        entries.sort_by_cached_key(|e| (!e.dir, e.name.to_lowercase()));
        Ok(entries)
    }

    /// Changed files: `M` modified, `A` added, `D` deleted, `?` untracked.
    // shortcut: git reports paths from the top level, so a project repo that is a subfolder of a repository
    // gets no markers; strip `git rev-parse --show-prefix` if that case shows up.
    pub fn status(&self, repo: &str) -> Result<HashMap<String, char>> {
        self.root(repo)?;
        let out = git(
            Path::new(repo),
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        )?;
        Ok(parse_status(&String::from_utf8_lossy(&out)))
    }

    /// Best fuzzy matches for `query` among the repository files. The file list is cached per repository;
    /// `fresh` walks the repository again (the UI asks for it each time the finder opens).
    pub fn find(&self, repo: &str, query: &str, fresh: bool) -> Result<Vec<String>> {
        let root = self.root(repo)?;
        let cached = self.index.lock().unwrap().get(&root).cloned();
        let files = match cached {
            Some(files) if !fresh => files,
            _ => {
                let files = Arc::new(index(&root));
                self.index.lock().unwrap().insert(root, files.clone());
                files
            }
        };
        Ok(rank(&files, query))
    }

    /// Lines matching `query` (literal unless `regex`, smart case), at most `MAX_HITS`.
    pub fn search(&self, repo: &str, query: &str, regex: bool) -> Result<SearchResult> {
        let root = self.root(repo)?;
        let mut hits = Vec::new();
        if !query.is_empty() {
            let matcher = RegexMatcherBuilder::new()
                .case_smart(true)
                .fixed_strings(!regex)
                .build(query)?;
            let mut searcher = SearcherBuilder::new()
                .binary_detection(BinaryDetection::quit(0))
                .build();
            let files = walker(&root).max_filesize(Some(MAX_EDIT_BYTES)).build();
            for entry in files.filter_map(Result::ok) {
                if hits.len() > MAX_HITS {
                    break;
                }
                if !entry.file_type().is_some_and(|t| t.is_file()) {
                    continue;
                }
                let Some(path) = rel_path(&root, entry.path()) else {
                    continue;
                };
                // Unreadable files are skipped, as ripgrep does.
                let _ = searcher.search_path(
                    &matcher,
                    entry.path(),
                    Lossy(|line, text| {
                        hits.push(Hit {
                            path: path.clone(),
                            line,
                            text: text.trim().chars().take(MAX_LINE_CHARS).collect(),
                        });
                        Ok(hits.len() <= MAX_HITS)
                    }),
                );
            }
        }
        let truncated = hits.len() > MAX_HITS;
        hits.truncate(MAX_HITS);
        Ok(SearchResult { hits, truncated })
    }

    pub fn read(&self, repo: &str, rel: &str) -> Result<Opened> {
        let (_, path) = self.resolve(repo, rel)?;
        let meta = fs::metadata(&path)?;
        ensure!(meta.is_file(), "{rel} is not a file");
        let content = if meta.len() > MAX_EDIT_BYTES {
            Content::TooLarge { size: meta.len() }
        } else {
            decode(fs::read(&path)?)
        };
        Ok(Opened {
            content,
            mtime: mtime(&meta),
        })
    }

    /// Overwrites an existing file, unless it changed on disk since `expected_mtime` (an agent edited it, typically).
    pub fn write(
        &self,
        repo: &str,
        rel: &str,
        text: &str,
        expected_mtime: Option<u64>,
    ) -> Result<Saved> {
        let (_, path) = self.resolve(repo, rel)?;
        let meta = fs::metadata(&path)?;
        ensure!(meta.is_file(), "{rel} is not a file");
        if expected_mtime.is_some_and(|m| m != mtime(&meta)) {
            return Ok(Saved::Conflict);
        }
        fs::write(&path, text)?;
        Ok(Saved::Saved {
            mtime: mtime(&fs::metadata(&path)?),
        })
    }

    /// The file as committed in `HEAD`; `None` when it isn't there (new file, no commit yet).
    pub fn head(&self, repo: &str, rel: &str) -> Result<Option<Content>> {
        let (root, path) = self.resolve(repo, rel)?;
        let rel = rel_path(&root, &path).context("invalid path")?;
        Ok(git(Path::new(repo), &["show", &format!("HEAD:./{rel}")])
            .ok()
            .map(decode))
    }

    /// Opens the file with `editor` (a command from the settings) or the system's default app.
    pub fn open_external(&self, repo: &str, rel: &str, editor: Option<&str>) -> Result<()> {
        self.resolve(repo, rel)?;
        // The path as the user knows it: canonical Windows paths (`\\?\C:\…`) confuse some apps.
        let path = Path::new(repo).join(rel);
        let editor = editor.map(str::trim).filter(|e| !e.is_empty());
        let mut cmd = match editor {
            Some(editor) if cfg!(windows) => {
                let mut cmd = command("cmd");
                cmd.args(["/d", "/c", editor]);
                cmd
            }
            // A login shell so GUI launches find the editor on the user's PATH; the path is `$1`, never interpolated.
            Some(editor) => {
                let mut cmd = command(&std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()));
                cmd.args(["-l", "-i", "-c", &format!("{editor} \"$1\""), "termf"]);
                cmd
            }
            None if cfg!(target_os = "macos") => command("open"),
            None if cfg!(windows) => command("explorer"),
            None => command("xdg-open"),
        };
        let out = cmd.arg(&path).stdin(Stdio::null()).output()?;
        // explorer.exe exits with 1 even when it opened the file.
        ensure!(
            out.status.success() || (cfg!(windows) && editor.is_none()),
            "cannot open {}: {}",
            path.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
        Ok(())
    }
}

fn walker(path: &Path) -> WalkBuilder {
    let mut builder = WalkBuilder::new(path);
    builder
        .hidden(false)
        .filter_entry(|e| e.file_name() != ".git");
    builder
}

fn index(root: &Path) -> Vec<String> {
    let mut files: Vec<_> = walker(root)
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .filter_map(|e| rel_path(root, e.path()))
        .take(MAX_INDEX_FILES)
        .collect();
    // Shorter paths first, so they win score ties (the ranking sort is stable).
    files.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    files
}

fn rank(files: &[String], query: &str) -> Vec<String> {
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    pattern
        .match_list(files, &mut matcher)
        .into_iter()
        .take(FIND_LIMIT)
        .map(|(path, _)| path.clone())
        .collect()
}

fn rel_path(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?.to_string_lossy();
    Some(if cfg!(windows) {
        rel.replace('\\', "/")
    } else {
        rel.into_owned()
    })
}

fn decode(bytes: Vec<u8>) -> Content {
    if bytes.len() as u64 > MAX_EDIT_BYTES {
        return Content::TooLarge {
            size: bytes.len() as u64,
        };
    }
    // Same heuristic as git: a NUL byte near the start means binary.
    if bytes[..bytes.len().min(8000)].contains(&0) {
        return Content::Binary;
    }
    String::from_utf8(bytes).map_or(Content::Binary, |text| Content::Text { text })
}

fn mtime(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as u64)
}

fn parse_status(out: &str) -> HashMap<String, char> {
    let mut map = HashMap::new();
    let mut tokens = out.split('\0');
    while let Some(entry) = tokens.next() {
        let (Some(code), Some(path)) = (entry.get(..2), entry.get(3..)) else {
            continue;
        };
        // Renames and copies are followed by their source path.
        if code.starts_with(['R', 'C']) {
            tokens.next();
        }
        let mark = match code {
            "??" => '?',
            c if c.starts_with('A') => 'A',
            c if c.contains('D') => 'D',
            _ => 'M',
        };
        map.insert(path.to_owned(), mark);
    }
    map
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    // No optional locks: a status refresh must never make an agent's concurrent git command fail on index.lock.
    let out = command("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(root)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .context("cannot run git")?;
    ensure!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(out.stdout)
}

fn command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_repo(name: &str) -> (PathBuf, Files) {
        let dir = std::env::temp_dir().join(format!("termf-files-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let repo = dir.join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::write(repo.join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(dir.join("secret.txt"), "outside").unwrap();
        let files = Files::default();
        files.set_roots([&repo]);
        (repo, files)
    }

    fn s(path: &Path) -> &str {
        path.to_str().unwrap()
    }

    #[test]
    fn guard_keeps_paths_inside_known_repositories() {
        let (repo, files) = temp_repo("guard");
        let repo_s = s(&repo);
        assert!(files.resolve(repo_s, "src/main.rs").is_ok());
        assert!(files.resolve(repo_s, "").is_ok());
        assert!(files.resolve(repo_s, "src/../src/main.rs").is_ok());

        assert!(files.resolve(repo_s, "../secret.txt").is_err());
        assert!(files.resolve(repo_s, "src/../../secret.txt").is_err());
        let outside = repo.parent().unwrap().join("secret.txt");
        assert!(files.resolve(repo_s, s(&outside)).is_err(), "absolute path");
        assert!(files.read(repo_s, "../secret.txt").is_err());
        assert!(files.write(repo_s, "../secret.txt", "x", None).is_err());
        assert_eq!(fs::read_to_string(&outside).unwrap(), "outside");

        fs::write(repo.join(".git/config"), "").unwrap();
        assert!(files.resolve(repo_s, ".git/config").is_err());
        assert!(files.resolve(repo_s, "src/missing.rs").is_err());

        let parent = repo.parent().unwrap();
        assert!(
            files.resolve(s(parent), "secret.txt").is_err(),
            "unknown repository"
        );
    }

    #[cfg(unix)]
    #[test]
    fn guard_rejects_symlinks_escaping_the_repository() {
        use std::os::unix::fs::symlink;
        let (repo, files) = temp_repo("symlink");
        let outside = repo.parent().unwrap();
        symlink(outside.join("secret.txt"), repo.join("leak.txt")).unwrap();
        symlink(outside, repo.join("up")).unwrap();
        symlink(repo.join("src/main.rs"), repo.join("alias.rs")).unwrap();

        assert!(files.read(s(&repo), "leak.txt").is_err());
        assert!(files.write(s(&repo), "leak.txt", "x", None).is_err());
        assert!(files.list(s(&repo), "up").is_err());
        assert!(files.read(s(&repo), "up/secret.txt").is_err());
        assert!(
            files.read(s(&repo), "alias.rs").is_ok(),
            "symlink within the repo"
        );
        assert_eq!(
            fs::read_to_string(outside.join("secret.txt")).unwrap(),
            "outside"
        );
    }

    #[test]
    fn lists_without_ignored_entries() {
        let (repo, files) = temp_repo("list");
        fs::write(repo.join(".gitignore"), "target/\n*.log\n").unwrap();
        fs::create_dir_all(repo.join("target")).unwrap();
        fs::write(repo.join("debug.log"), "").unwrap();
        fs::write(repo.join("README.md"), "").unwrap();
        let names: Vec<_> = files
            .list(s(&repo), "")
            .unwrap()
            .into_iter()
            .map(|e| (e.name, e.dir))
            .collect();
        assert_eq!(
            names,
            [
                ("src".into(), true),
                (".gitignore".into(), false),
                ("README.md".into(), false)
            ]
        );
        assert_eq!(files.list(s(&repo), "src").unwrap()[0].path, "src/main.rs");
    }

    #[test]
    fn ranks_fuzzy_matches() {
        let files: Vec<String> = [
            "main.rs",
            "src/main.rs",
            "docs/remains.txt",
            "app/src/lib/Terminal.svelte",
        ]
        .map(String::from)
        .into();
        assert_eq!(rank(&files, "main")[..2], ["main.rs", "src/main.rs"]);
        assert_eq!(rank(&files, "srcmain"), ["src/main.rs"]);
        assert_eq!(rank(&files, "term")[0], "app/src/lib/Terminal.svelte");
        assert_eq!(rank(&files, "").len(), 4);
        assert!(rank(&files, "zzz").is_empty());
    }

    #[test]
    fn finder_caches_until_refreshed() {
        let (repo, files) = temp_repo("find");
        assert_eq!(
            files.find(s(&repo), "main", false).unwrap(),
            ["src/main.rs"]
        );
        fs::write(repo.join("src/domain.rs"), "").unwrap();
        assert!(files.find(s(&repo), "domain", false).unwrap().is_empty());
        assert_eq!(
            files.find(s(&repo), "domain", true).unwrap(),
            ["src/domain.rs"]
        );
    }

    #[test]
    fn search_is_capped_and_skips_binary_and_ignored_files() {
        let (repo, files) = temp_repo("search");
        fs::write(repo.join(".gitignore"), "ignored.txt\n").unwrap();
        fs::write(repo.join("ignored.txt"), "needle\n").unwrap();
        fs::write(repo.join("bin.dat"), b"needle\0\x01").unwrap();
        fs::write(repo.join("few.txt"), "a\n  Needle here  \nb\n").unwrap();

        let result = files.search(s(&repo), "needle", false).unwrap();
        assert!(!result.truncated);
        assert_eq!(result.hits.len(), 1);
        assert_eq!(
            (result.hits[0].path.as_str(), result.hits[0].line),
            ("few.txt", 2)
        );
        assert_eq!(result.hits[0].text, "Needle here");
        assert!(
            files.search(s(&repo), "Needle", false).unwrap().hits.len() == 1,
            "smart case"
        );
        assert!(files.search(s(&repo), "main(", true).is_err());
        assert_eq!(
            files.search(s(&repo), "main(", false).unwrap().hits.len(),
            1
        );

        fs::write(repo.join("many.txt"), "needle\n".repeat(MAX_HITS + 10)).unwrap();
        let result = files.search(s(&repo), "nee+dle", true).unwrap();
        assert!(result.truncated);
        assert_eq!(result.hits.len(), MAX_HITS);
    }

    #[test]
    fn reads_only_reasonable_text_files() {
        let (repo, files) = temp_repo("read");
        let opened = files.read(s(&repo), "src/main.rs").unwrap();
        assert_eq!(
            opened.content,
            Content::Text {
                text: "fn main() {}\n".into()
            }
        );
        fs::write(repo.join("nul.bin"), b"abc\0def").unwrap();
        assert_eq!(
            files.read(s(&repo), "nul.bin").unwrap().content,
            Content::Binary
        );
        fs::write(repo.join("latin1.txt"), b"caf\xe9").unwrap();
        assert_eq!(
            files.read(s(&repo), "latin1.txt").unwrap().content,
            Content::Binary
        );
        let big = fs::File::create(repo.join("big.txt")).unwrap();
        big.set_len(MAX_EDIT_BYTES + 1).unwrap();
        assert_eq!(
            files.read(s(&repo), "big.txt").unwrap().content,
            Content::TooLarge {
                size: MAX_EDIT_BYTES + 1
            }
        );
        assert!(files.read(s(&repo), "src").is_err(), "folder");
    }

    #[test]
    fn write_refuses_to_clobber_changes_made_on_disk() {
        let (repo, files) = temp_repo("write");
        let mtime = files.read(s(&repo), "src/main.rs").unwrap().mtime;
        assert_eq!(
            files
                .write(s(&repo), "src/main.rs", "a", Some(mtime + 1))
                .unwrap(),
            Saved::Conflict
        );
        assert_eq!(
            fs::read_to_string(repo.join("src/main.rs")).unwrap(),
            "fn main() {}\n"
        );
        assert!(matches!(
            files
                .write(s(&repo), "src/main.rs", "b", Some(mtime))
                .unwrap(),
            Saved::Saved { .. }
        ));
        assert_eq!(fs::read_to_string(repo.join("src/main.rs")).unwrap(), "b");
    }

    #[test]
    fn reads_status_and_head_from_git() {
        let (repo, files) = temp_repo("git");
        fs::remove_dir_all(repo.join(".git")).unwrap();
        let run = |args: &[&str]| git(&repo, args).unwrap();
        run(&["init", "-q"]);
        run(&["add", "."]);
        run(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "init",
        ]);
        fs::write(repo.join("src/main.rs"), "changed\n").unwrap();
        fs::write(repo.join("new.txt"), "").unwrap();

        let status = files.status(s(&repo)).unwrap();
        assert_eq!(status["src/main.rs"], 'M');
        assert_eq!(status["new.txt"], '?');
        assert_eq!(
            files.head(s(&repo), "src/main.rs").unwrap(),
            Some(Content::Text {
                text: "fn main() {}\n".into()
            })
        );
        assert_eq!(files.head(s(&repo), "new.txt").unwrap(), None);
    }

    #[test]
    fn parses_git_status() {
        let status =
            parse_status(" M src/a.rs\0A  new.rs\0R  moved.rs\0old.rs\0?? notes.md\0 D gone.rs\0");
        assert_eq!(status["src/a.rs"], 'M');
        assert_eq!(status["new.rs"], 'A');
        assert_eq!(status["moved.rs"], 'M');
        assert_eq!(status["notes.md"], '?');
        assert_eq!(status["gone.rs"], 'D');
        assert!(!status.contains_key("old.rs"));
    }
}
