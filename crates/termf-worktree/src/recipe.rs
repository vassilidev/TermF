//! The project's recipe, in its library, and what saving it writes next to it: the `worktree-setup` skill,
//! role (the setup agent) and command. A proposal from Claude Code waits in a draft until the user saves it.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::database::{self, DbView};
use crate::{Dep, Recipe, RepoRecipe, Worktree, detect, files, ports};

/// The skill, the role and the command share it.
pub const NAME: &str = "worktree-setup";
const MAX_LIST: usize = 40;
const MAX_TEXT: usize = 2000;

/// What the recipe view of a project shows.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RecipeView {
    /// What applies to new worktrees: the saved recipe, else what detection found.
    pub recipe: Recipe,
    pub saved: bool,
    pub detected: Recipe,
    pub draft: Option<Draft>,
    pub repos: Vec<RepoView>,
    /// `[conventions] model` and `effort`, which a proposal uses.
    pub model: String,
    pub effort: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub recipe: Recipe,
    pub cost: Option<f64>,
    /// What was dropped from Claude's answer, and why.
    pub dropped: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RepoView {
    pub name: String,
    pub path: String,
    /// The database each source named by the saved, detected or proposed recipe points at.
    pub databases: BTreeMap<String, DbView>,
}

fn dir(config_dir: &Path, project: &str) -> Result<PathBuf> {
    Ok(termf_library::library_dir(config_dir, Some(project))?.join("recipes"))
}

pub fn load(config_dir: &Path, project: &str) -> Result<Option<Recipe>> {
    let file = dir(config_dir, project)?.join(format!("{NAME}.toml"));
    if !file.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&file)?;
    Ok(Some(
        toml::from_str(&text).with_context(|| format!("invalid {}", file.display()))?,
    ))
}

fn load_draft(config_dir: &Path, project: &str) -> Result<Option<Draft>> {
    let file = dir(config_dir, project)?.join(format!("{NAME}.draft.toml"));
    match fs::read_to_string(&file) {
        Ok(text) => Ok(toml::from_str(&text).ok()),
        Err(_) => Ok(None),
    }
}

/// The recipe that applies to `wt`, its repository's part, and whether the user saved it.
pub(crate) fn effective(
    config_dir: &Path,
    wt: &Worktree,
    env: &[(String, String)],
) -> Result<(Recipe, RepoRecipe, bool)> {
    Ok(match load(config_dir, &wt.project)? {
        Some(recipe) => {
            let repo = match recipe.repos.get(&wt.repo) {
                Some(repo) => repo.clone(),
                None => detect(&wt.source, env),
            };
            (recipe, repo, true)
        }
        None => (Recipe::default(), detect(&wt.source, env), false),
    })
}

fn project(config_dir: &Path, id: &str) -> Result<termf_core::Project> {
    termf_core::project_file(config_dir, id)?;
    termf_core::load_projects(config_dir)?
        .into_iter()
        .find(|p| p.id == id)
        .with_context(|| format!("unknown project {id}"))
}

fn detected(project: &termf_core::Project) -> Recipe {
    Recipe {
        repos: project
            .repos
            .iter()
            .map(|r| (r.name.clone(), detect(Path::new(&r.path), &[])))
            .collect(),
        ..Recipe::default()
    }
}

pub fn view(config_dir: &Path, id: &str) -> Result<RecipeView> {
    let project = project(config_dir, id)?;
    let saved = load(config_dir, id)?;
    let detected = detected(&project);
    let draft = load_draft(config_dir, id)?;
    let repos = project
        .repos
        .iter()
        .map(|r| {
            let mut databases = BTreeMap::new();
            let recipes = [
                saved.as_ref(),
                Some(&detected),
                draft.as_ref().map(|d| &d.recipe),
            ];
            for recipe in recipes.into_iter().flatten() {
                if let Some(from) = recipe.repos.get(&r.name).and_then(|s| s.database.clone())
                    && !databases.contains_key(&from)
                {
                    let view = database::view(Path::new(&r.path), &from);
                    databases.insert(from, view);
                }
            }
            RepoView {
                name: r.name.clone(),
                path: r.path.clone(),
                databases,
            }
        })
        .collect();
    let (model, effort) = termf_library::conventions::settings(config_dir);
    Ok(RecipeView {
        recipe: saved.clone().unwrap_or_else(|| detected.clone()),
        saved: saved.is_some(),
        detected,
        draft,
        repos,
        model,
        effort,
    })
}

/// Saves the recipe the user checked, writes the skill, the role and the command it describes, and drops the
/// draft it came from.
pub fn save(config_dir: &Path, id: &str, recipe: Recipe) -> Result<RecipeView> {
    let project = project(config_dir, id)?;
    let names: Vec<&str> = project.repos.iter().map(|r| r.name.as_str()).collect();
    let mut checked = Recipe {
        agent: recipe.agent,
        notes: text(&recipe.notes, "notes")?,
        repos: BTreeMap::new(),
    };
    for (name, repo) in recipe.repos {
        ensure!(
            names.contains(&name.as_str()),
            "no repository {name} in project {id}"
        );
        checked.repos.insert(
            name.clone(),
            check(&repo).with_context(|| format!("repository {name}"))?,
        );
    }
    let dir = dir(config_dir, id)?;
    fs::create_dir_all(&dir)?;
    let body = toml::to_string(&checked)?;
    termf_core::write_atomic(
        &dir.join(format!("{NAME}.toml")),
        &format!(
            "# How TermF prepares a worktree of this project (ARCHITECTURE §9): `termf worktree setup` runs it again.\n{body}"
        ),
    )?;
    write_library(config_dir, &project, &checked)?;
    let _ = fs::remove_file(dir.join(format!("{NAME}.draft.toml")));
    view(config_dir, id)
}

pub fn discard(config_dir: &Path, id: &str) -> Result<RecipeView> {
    let _ = fs::remove_file(dir(config_dir, id)?.join(format!("{NAME}.draft.toml")));
    view(config_dir, id)
}

/// Asks Claude Code, read-only in the project's repositories, for a recipe; it becomes the draft.
pub fn propose(config_dir: &Path, id: &str, claude: &[String]) -> Result<RecipeView> {
    let project = project(config_dir, id)?;
    ensure!(!project.repos.is_empty(), "project {id} has no repository");
    let repos: Vec<(String, PathBuf)> = project
        .repos
        .iter()
        .map(|r| (r.name.clone(), PathBuf::from(&r.path)))
        .collect();
    let current = load(config_dir, id)?.unwrap_or_else(|| detected(&project));
    let (model, effort) = termf_library::conventions::settings(config_dir);
    let prompt = prompt(&project, &current)?;
    let (answer, cost) =
        termf_library::conventions::ask(claude, &repos, &model, &effort, &prompt, &schema())?;
    let mut draft = Draft {
        recipe: Recipe {
            agent: answer["agent"].as_bool().unwrap_or(false),
            notes: answer["notes"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_owned(),
            repos: BTreeMap::new(),
        },
        cost,
        dropped: Vec::new(),
    };
    if draft.recipe.notes.len() > MAX_TEXT {
        draft.recipe.notes.clear();
        draft.dropped.push("notes: too long".into());
    }
    for raw in answer["repos"].as_array().into_iter().flatten() {
        let name = raw["name"].as_str().unwrap_or_default().to_owned();
        if !project.repos.iter().any(|r| r.name == name) {
            draft
                .dropped
                .push(format!("{name}: no such repository in the project"));
            continue;
        }
        let parsed = serde_json::from_value::<RepoRecipe>(raw.clone())
            .map_err(anyhow::Error::from)
            .and_then(|mut repo| {
                // The schema gives "" for no database.
                repo.database = repo.database.filter(|d| !d.trim().is_empty());
                check(&repo)
            });
        match parsed {
            Ok(repo) => {
                draft.recipe.repos.insert(name, repo);
            }
            Err(e) => draft.dropped.push(format!("{name}: {e:#}")),
        }
    }
    ensure!(
        !draft.recipe.repos.is_empty() || !draft.dropped.is_empty(),
        "Claude Code proposed nothing"
    );
    let dir = dir(config_dir, id)?;
    fs::create_dir_all(&dir)?;
    termf_core::write_atomic(
        &dir.join(format!("{NAME}.draft.toml")),
        &toml::to_string(&draft)?,
    )?;
    view(config_dir, id)
}

/// Trims the recipe of a repository and refuses what TermF could not run safely: paths out of the repository,
/// raw copies of dependency folders, oversized lists.
fn check(repo: &RepoRecipe) -> Result<RepoRecipe> {
    let list = |items: &[String], what: &str| -> Result<Vec<String>> {
        ensure!(
            items.len() <= MAX_LIST,
            "{what}: at most {MAX_LIST} entries"
        );
        let mut kept: Vec<String> = Vec::new();
        for item in items {
            let item = text(item, what)?;
            if !item.is_empty() && !kept.contains(&item) {
                kept.push(item);
            }
        }
        Ok(kept)
    };
    let include = list(&repo.include, "include")?;
    for pattern in &include {
        let bare = pattern.trim_start_matches('!').trim_matches('/');
        ensure!(
            !bare.split('/').any(|part| part == ".." || part == ".git"),
            "include: {pattern} leaves the repository"
        );
        ensure!(
            !files::DEPENDENCY_DIRS.contains(&bare),
            "include: {bare} is a dependency folder, never copied raw (list it under dependencies)"
        );
    }
    ensure!(repo.deps.len() <= MAX_LIST, "deps: at most {MAX_LIST}");
    let mut deps = Vec::new();
    for dep in &repo.deps {
        let dir = text(&dep.dir, "deps")?.trim_matches('/').to_owned();
        ensure!(
            dir.is_empty() || files::relative(&dir).is_some(),
            "deps: {dir} is not a folder of the repository"
        );
        let install = list(&dep.install, "install")?;
        if dir.is_empty() && install.is_empty() {
            continue;
        }
        deps.push(Dep {
            dir,
            clone: dep.clone,
            install,
        });
    }
    let database = match repo.database.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(from) => {
            ensure!(
                files::relative(from).is_some(),
                "database: {from} is not a file of the repository"
            );
            Some(from.to_owned())
        }
    };
    let ports = list(&repo.ports, "ports")?
        .iter()
        .map(|p| ports::name(p))
        .filter(|p| !p.is_empty() && p != "port")
        .collect();
    Ok(RepoRecipe {
        stack: list(&repo.stack, "stack")?,
        include,
        deps,
        database,
        compose: repo.compose,
        ports,
        commands: list(&repo.commands, "commands")?,
    })
}

fn text(value: &str, what: &str) -> Result<String> {
    ensure!(value.len() <= MAX_TEXT, "{what}: too long");
    Ok(value.trim().to_owned())
}

fn prompt(project: &termf_core::Project, current: &Recipe) -> Result<String> {
    let mut repos = String::new();
    for r in &project.repos {
        let _ = writeln!(repos, "- {}: {}", r.name, r.path);
    }
    let known = toml::to_string(current)?;
    Ok(format!(
        "TermF runs the recipe below in every new git worktree of the project \"{name}\", so an agent can work there \
         right away. Read the repositories (you cannot change anything) and propose the recipe of each one.\n\n\
         Repositories (name: path):\n{repos}\n\
         The current recipe (what TermF detected from the files alone, or what the user saved), in TOML:\n\
         ```toml\n{known}```\n\n\
         For each repository:\n\
         - `stack`: what it is made of, as short lowercase words (`node:pnpm`, `laravel`, `postgres`…).\n\
         - `include`: .worktreeinclude patterns (gitignore syntax) of the files git ignores that a worktree needs to \
         run, such as `.env` or a local config. Never a dependency folder.\n\
         - `deps`: each dependency folder (`dir`, relative; empty when the tool keeps packages in a shared cache). \
         Dependency folders are never copied raw: with `clone`, TermF clones the repository's folder copy-on-write \
         when the file system can (false for a folder that holds absolute paths, such as a Python virtualenv), \
         then runs `install`, commands tried in order until one works: the package manager's offline or cache \
         mode first, then the online one.\n\
         - `database`: the file that gives the local database's connection (an env file with DATABASE_URL or DB_*, \
         or config/database.yml), or \"\" for none. TermF copies that database for each worktree, and never a \
         remote one.\n\
         - `compose`: true when it runs Docker Compose; `ports`: the services that need a port of their own (TermF \
         sets PORT and TERMF_PORT_<NAME>).\n\
         - `commands`: shell commands to run in the worktree afterwards, deterministic and safe to run twice. \
         Migrations and seeds are not commands: they are for the setup agent.\n\
         Then `agent`: true when a worktree needs judgment to be ready (migrations, seeds, generated files), and \
         `notes`: what an agent preparing a worktree of this project should know, in a few lines.\n\
         Keep what is right in the current recipe. Answer in English.",
        name = project.name
    ))
}

fn schema() -> Value {
    let text = json!({ "type": "string" });
    let texts = json!({ "type": "array", "items": text });
    let dep = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["dir", "clone", "install"],
        "properties": { "dir": text, "clone": { "type": "boolean" }, "install": texts },
    });
    let repo = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["name", "stack", "include", "deps", "database", "compose", "ports", "commands"],
        "properties": {
            "name": text, "stack": texts, "include": texts,
            "deps": { "type": "array", "items": dep },
            "database": text, "compose": { "type": "boolean" }, "ports": texts, "commands": texts,
        },
    });
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["agent", "notes", "repos"],
        "properties": {
            "agent": { "type": "boolean" },
            "notes": text,
            "repos": { "type": "array", "items": repo },
        },
    })
}

/// The skill (how a worktree of the project is prepared), the role (the agent that does what needs judgment)
/// and the command (`/worktree-setup`, which runs the setup again), in the project's library.
fn write_library(config_dir: &Path, project: &termf_core::Project, recipe: &Recipe) -> Result<()> {
    let library = termf_library::library_dir(config_dir, Some(&project.id))?;
    let name = &project.name;
    let mut skill = format!(
        "---\nname: {NAME}\ndescription: {}\n---\n# Preparing a worktree of {name}\n\n\
         TermF prepares every new worktree of {name} with the recipe below, kept in the project's library. \
         `termf worktree setup` runs it again in the current worktree, `termf worktree teardown` drops its database \
         copy and gives its ports back.\n",
        json!(format!(
            "How a git worktree of {name} is prepared: dependencies, local files, its own database copy, Docker \
             Compose and ports. Use it when a worktree lacks dependencies, configuration or data."
        ))
    );
    for (repo, r) in &recipe.repos {
        let _ = write!(skill, "\n## {repo}");
        if !r.stack.is_empty() {
            let _ = write!(skill, " ({})", r.stack.join(", "));
        }
        skill.push_str("\n\n");
        for dep in &r.deps {
            let what = match dep.dir.as_str() {
                "" => "Packages in the tool's shared cache".to_owned(),
                dir => format!("`{dir}`"),
            };
            let mut how = Vec::new();
            if dep.clone && !dep.dir.is_empty() {
                how.push(
                    "cloned copy-on-write from the repository when the file system can".to_owned(),
                );
            }
            if !dep.install.is_empty() {
                let commands: Vec<String> = dep.install.iter().map(|c| format!("`{c}`")).collect();
                how.push(format!("installed with {}", commands.join(", else ")));
            }
            let _ = writeln!(skill, "- {what}: {}.", how.join(", then "));
        }
        if !r.include.is_empty() {
            let _ = writeln!(
                skill,
                "- Local files copied from the repository: {}.",
                r.include
                    .iter()
                    .map(|i| format!("`{i}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if let Some(from) = &r.database {
            let _ = writeln!(
                skill,
                "- Database: each worktree has its own copy of the local database named in `{from}`, which points at \
                 it, dropped when the run closes. Never connect to another database, and never to a remote one."
            );
        }
        let mut ports = vec!["`PORT`".to_owned()];
        ports.extend(
            r.ports
                .iter()
                .map(|p| format!("`TERMF_PORT_{}`", p.to_ascii_uppercase())),
        );
        let _ = writeln!(
            skill,
            "- Ports of its own, in the environment: {}.",
            ports.join(", ")
        );
        if r.compose {
            let _ = writeln!(
                skill,
                "- Docker Compose: `COMPOSE_PROJECT_NAME` is set per run; refer to the ports above in the Compose \
                 file to keep runs apart."
            );
        }
        for c in &r.commands {
            let _ = writeln!(skill, "- Then: `{c}`.");
        }
    }
    skill.push_str(
        "\n## Rules\n\n- Work in the worktree only, and never push.\n- Never copy a dependency folder (`node_modules`, \
         `vendor`, `.venv`, `target`…): install it from the package manager's cache.\n- Use only this worktree's \
         database copy.\n",
    );
    if !recipe.notes.trim().is_empty() {
        let _ = write!(skill, "\n## Notes\n\n{}\n", recipe.notes.trim());
    }
    let role = format!(
        "---\nname: {NAME}\ndescription: {}\ntools: Bash, Read, Edit, Write, Glob, Grep\nmodel: sonnet\n---\n\
         You prepare a git worktree of {name} so that an agent can work in it right away. TermF has already run the \
         recipe's steps; you do what is left: a step that failed, then migrations, seeds or generated files the \
         project needs. Follow the `{NAME}` skill.\n\n\
         - Stay in the worktree. Never push, never open a pull request.\n\
         - Never copy a dependency folder: install from the package manager's cache.\n\
         - Touch no database but this worktree's own copy, and never a remote one.\n\
         - End with a short summary: what you did, and what still needs a person.\n",
        json!(format!(
            "Prepares a worktree of {name} when a step needs judgment: migrations, seeds, an install that fails."
        ))
    );
    let command = "---\ndescription: Prepare this worktree again (dependencies, local files, database copy, ports)\n---\n\
        Run `\"${TERMF_BIN:-termf}\" worktree setup` in this worktree and report each step. If a step failed, fix \
        it following the worktree-setup skill, then run the setup again.\n";
    let write = |path: PathBuf, text: &str| -> Result<()> {
        fs::create_dir_all(path.parent().context("no parent folder")?)?;
        termf_core::write_atomic(&path, text)
    };
    write(library.join("skills").join(NAME).join("SKILL.md"), &skill)?;
    write(library.join("roles").join(format!("{NAME}.md")), &role)?;
    write(library.join("commands").join(format!("{NAME}.md")), command)
}
