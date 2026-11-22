//! Posting a decided review's images on the run's pull request, when the project asks for it (`[visual]
//! pr_images = true`) and the user confirms what goes out. GitHub takes no image in a comment through its API:
//! the images go in a commit of their own, outside every branch's history, pushed to
//! `termf-visual/<review>-v<n>`, and the comment links to them. The comment holds the screens' paths, sizes,
//! themes and images only: no comment, local path or environment value.

use std::fs;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::{Decision, Version, VersionState, Visual, env, folder};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrPreview {
    pub review: i64,
    pub version: u32,
    pub pr: String,
    /// Where the images are pushed.
    pub branch: String,
    pub files: Vec<String>,
    /// The comment, as it will read.
    pub body: String,
    /// The commit of the images: confirming with it posts exactly this.
    pub fingerprint: String,
}

impl Visual {
    /// What posting the images would send. Makes the images' commit locally, nothing leaves the machine.
    pub fn pr_preview(&self, id: i64) -> Result<PrPreview> {
        let inner = &self.inner;
        let review = self.review(id)?;
        let config = crate::config::load(&inner.config_dir, &review.repo_path)?;
        ensure!(
            config.settings.pr_images,
            "posting the images on the pull request is off for this project: set `pr_images = true` in its [visual]"
        );
        let run = review
            .run
            .context("only a ticket run's review has a pull request")?;
        let pr = termf_tickets::Tickets::new(inner.config_dir.clone(), inner.data_dir.clone())
            .run(run)?
            .pr_url
            .context("the run has no pull request yet: deliver it first")?;
        let slug = pr
            .strip_prefix("https://github.com/")
            .and_then(|p| p.split("/pull/").next())
            .filter(|s| s.split('/').count() == 2)
            .with_context(|| format!("{pr} is not a GitHub pull request"))?
            .to_owned();
        let version = review
            .versions
            .iter()
            .rev()
            .find(|v| v.state == VersionState::Done)
            .context("no version decided yet")?;
        let screens: Vec<_> = version
            .screens
            .iter()
            .filter(|s| s.changed && s.before.is_some() && s.after.is_some())
            .collect();
        ensure!(!screens.is_empty(), "no screen changed: nothing to post");
        let dir = folder(&inner.data_dir, id, version.n);
        let index = termf_core::process::scratch_dir()?.join("index");
        let finished = version.finished.unwrap_or(version.created);
        // Fixed dates: the same images make the same commit, so the preview's fingerprint is the one posted.
        let date = format!("@{finished} +0000");
        let mut git_env = inner.env.clone();
        git_env.push((
            "GIT_INDEX_FILE".into(),
            index.to_string_lossy().into_owned(),
        ));
        git_env.push(("GIT_AUTHOR_DATE".into(), date.clone()));
        git_env.push(("GIT_COMMITTER_DATE".into(), date));
        let mut files = Vec::new();
        for s in &screens {
            for file in [&s.before, &s.after].into_iter().flatten() {
                let path = dir.join(file);
                let blob = env::git(
                    &git_env,
                    &review.repo_path,
                    &["hash-object", "-w", &path.to_string_lossy()],
                )?;
                let name = format!("v{}/{file}", version.n);
                env::git(
                    &git_env,
                    &review.repo_path,
                    &[
                        "update-index",
                        "--add",
                        "--cacheinfo",
                        &format!("100644,{blob},{name}"),
                    ],
                )?;
                files.push(name);
            }
        }
        let tree = env::git(&git_env, &review.repo_path, &["write-tree"])?;
        let message = format!("TermF visual review {id}, v{}", version.n);
        let commit = env::git(
            &git_env,
            &review.repo_path,
            &["commit-tree", &tree, "-m", &message],
        )?;
        let _ = fs::remove_dir_all(index.parent().unwrap_or(&index));
        let url = |file: &str| {
            format!(
                "https://github.com/{slug}/blob/{commit}/v{}/{file}?raw=true",
                version.n
            )
        };
        Ok(PrPreview {
            review: id,
            version: version.n,
            body: body(version, &url),
            pr,
            branch: format!("termf-visual/{id}-v{}", version.n),
            files,
            fingerprint: commit,
        })
    }

    /// Pushes the images and posts the comment the preview showed; refused if anything changed since.
    pub fn pr_comment(&self, id: i64, confirm: &str) -> Result<PrPreview> {
        let preview = self.pr_preview(id)?;
        ensure!(
            preview.fingerprint == confirm,
            "the images changed since you saw them: look at what would be posted again"
        );
        let review = self.review(id)?;
        let refspec = format!("{}:refs/heads/{}", preview.fingerprint, preview.branch);
        env::git(
            &self.inner.env,
            &review.repo_path,
            &["push", "-q", "--force", "origin", &refspec],
        )?;
        let mut gh = Command::new("gh");
        if let Some(path) = termf_core::shell::login_path() {
            gh.env("PATH", path);
        }
        gh.args(["pr", "comment", &preview.pr, "--body-file", "-"])
            .current_dir(&review.repo_path)
            .envs(self.inner.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = gh.spawn().context("cannot run gh, the GitHub CLI")?;
        {
            use std::io::Write;
            child
                .stdin
                .take()
                .context("no stdin")?
                .write_all(preview.body.as_bytes())?;
        }
        let out = child.wait_with_output()?;
        if !out.status.success() {
            let error = String::from_utf8_lossy(&out.stderr).trim().to_owned();
            if error.contains("gh auth login") {
                bail!("gh is not signed in: run `gh auth login`");
            }
            bail!("gh pr comment failed: {error}");
        }
        Ok(preview)
    }
}

fn body(version: &Version, url: &dyn Fn(&str) -> String) -> String {
    let mut text = format!(
        "### Visual review v{} ({})\n\n",
        version.n,
        match version.outcome {
            Some(Decision::Approved) => "approved",
            Some(Decision::Rejected) => "rejected",
            Some(Decision::Changes) => "changes asked",
            _ => "pending",
        }
    );
    for s in version.screens.iter().filter(|s| s.changed) {
        let (Some(before), Some(after)) = (&s.before, &s.after) else {
            continue;
        };
        text.push_str(&format!(
            "**`{}`** · {} {}×{} · {}\n\n| Before | After |\n| --- | --- |\n| ![before]({}) | ![after]({}) |\n\n",
            s.path,
            s.viewport,
            s.width,
            s.height,
            s.theme,
            url(before),
            url(after)
        ));
    }
    text
}
