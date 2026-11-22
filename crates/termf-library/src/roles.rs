//! A role's settings without touching its prompt: `[roles.<name>]` in `termf.toml`, then in the project's file,
//! then `[squads.roles.<name>]` of the squad the session works in, then in the repository's `.termf/project.toml`,
//! each over the role's frontmatter. A scalar is replaced; in a list, `+x` adds, `-x` removes, and a list with
//! a bare name replaces. Instructions never go here: they are preferences, with `applies_to`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::{Map, Value};
use termf_core::squads::{Membership, same_role};

use crate::{Item, Kind, Layer, Scope, resolve};

const SCALARS: [&str; 4] = ["model", "effort", "effort_range", "permission_mode"];
const LISTS: [&str; 6] = [
    "skills",
    "preferences",
    "writes",
    "deny",
    "allowedTools",
    "disallowedTools",
];

/// A setting as the layers made it: `from` lists what set or changed it, in order: `role` (its frontmatter),
/// `global`, `project`, `squad`, `repo`.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Setting {
    pub key: String,
    pub value: Value,
    pub from: Vec<String>,
}

/// A role a session may play: from any layer, but one from a repository only once switched on, since it
/// comes with whatever was cloned. Its `meta` holds its settings as the layers make them.
pub fn role(config_dir: &Path, scope: &Scope, name: &str) -> Result<Item> {
    Ok(role_settings(config_dir, scope, name)?.0)
}

/// The role, and each of its settings with where it comes from.
pub fn role_settings(config_dir: &Path, scope: &Scope, name: &str) -> Result<(Item, Vec<Setting>)> {
    let found: Vec<Item> = resolve(config_dir, scope)?
        .into_iter()
        .filter(|i| i.kind == Kind::Role && same_role(&i.name, name))
        // A repository's role never stands in for another name.
        .filter(|i| i.layer != Layer::Repo || i.name == name)
        .collect();
    // A repository's role not switched on is left out, unless it is all there is.
    let mut item = match found
        .iter()
        .filter(|i| i.layer != Layer::Repo || i.active)
        .max_by_key(|i| (i.layer, i.name == name))
    {
        Some(item) => item.clone(),
        None if found.is_empty() => bail!("no role named {name}"),
        None => bail!("the role {name} comes from the repository and is not switched on"),
    };
    let mut meta = item.meta.clone();
    if let Some(mode) = meta.remove("permissionMode") {
        meta.entry("permission_mode").or_insert(mode);
    }
    let mut from: Map<String, Value> = Map::new();
    for key in SCALARS.iter().chain(&LISTS) {
        if meta.contains_key(*key) {
            from.insert(key.to_string(), Value::from(vec!["role"]));
        }
    }
    for (origin, file, table) in layers(config_dir, scope, name)? {
        let (mut layered, mut by) = (meta.clone(), from.clone());
        match apply(&mut layered, &mut by, origin, &file, table, name) {
            Ok(()) => (meta, from) = (layered, by),
            // A cloned repository's mistake never keeps the role from playing.
            Err(e) if origin == "repo" => eprintln!("termf: {e:#}; left out"),
            Err(e) => return Err(e),
        }
    }
    let settings = SCALARS
        .iter()
        .chain(&LISTS)
        .filter_map(|key| {
            Some(Setting {
                key: key.to_string(),
                value: meta.get(*key)?.clone(),
                from: crate::list_of(from.get(*key)),
            })
        })
        .collect();
    item.meta = meta;
    Ok((item, settings))
}

/// One layer's `[roles.<name>]` over `meta`, `from` saying which layers made each setting.
fn apply(
    meta: &mut Map<String, Value>,
    from: &mut Map<String, Value>,
    origin: &str,
    file: &Path,
    table: toml::Table,
    name: &str,
) -> Result<()> {
    for (key, value) in table {
        let at = || format!("[roles.{name}] in {}", file.display());
        if SCALARS.contains(&key.as_str()) {
            let value = match (key.as_str(), value) {
                ("effort_range", toml::Value::Array(range))
                    if range.len() == 2 && range.iter().all(|v| v.is_str()) =>
                {
                    Value::from(range.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
                }
                ("effort_range", _) => bail!("{}: effort_range is [low, high]", at()),
                (_, toml::Value::String(s)) => Value::from(s),
                _ => bail!("{}: {key} must be text", at()),
            };
            meta.insert(key.clone(), value);
            from.insert(key, Value::from(vec![origin]));
        } else if LISTS.contains(&key.as_str()) {
            let toml::Value::Array(entries) = value else {
                bail!("{}: {key} must be a list", at());
            };
            let entries: Vec<&str> = entries.iter().filter_map(|v| v.as_str()).collect();
            // An empty list, or one with a bare name, replaces; `+x` and `-x` change what is there.
            let replaces = entries.is_empty() || entries.iter().any(|e| !e.starts_with(['+', '-']));
            let mut list = match replaces {
                true => Vec::new(),
                false => crate::list_of(meta.get(&key)),
            };
            for entry in entries {
                match entry.strip_prefix('-') {
                    Some(gone) => list.retain(|x| x != gone),
                    None => {
                        let added = entry.strip_prefix('+').unwrap_or(entry).to_owned();
                        if !list.contains(&added) {
                            list.push(added);
                        }
                    }
                }
            }
            meta.insert(key.clone(), Value::from(list));
            let mut by = match replaces {
                true => Vec::new(),
                false => crate::list_of(from.get(&key)),
            };
            by.push(origin.to_owned());
            from.insert(key, Value::from(by));
        } else {
            bail!(
                "{}: {key} is not a role setting ({}); instructions for a role go in a preference",
                at(),
                SCALARS
                    .iter()
                    .chain(&LISTS)
                    .copied()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    Ok(())
}

type Layered = (&'static str, PathBuf, toml::Table);

/// The `[roles.<name>]` tables that apply, from the least to the most specific, with their origin and file.
fn layers(config_dir: &Path, scope: &Scope, name: &str) -> Result<Vec<Layered>> {
    let global = config_dir.join("termf.toml");
    let mut found = tables("global", &global, read(&global)?.get("roles"), name)?;
    if let Some(project) = &scope.project {
        let file = termf_core::project_file(config_dir, project)?;
        found.extend(tables("project", &file, read(&file)?.get("roles"), name)?);
        if let Some(id) = &scope.squad {
            let team = termf_core::squads::load_team(config_dir, project)?;
            if let Ok(Membership::Squad(squad)) = team.squad(id) {
                let roles = squad
                    .settings
                    .iter()
                    .map(|(k, t)| (k.clone(), toml::Value::Table(t.clone())))
                    .collect();
                let roles = toml::Value::Table(roles);
                found.extend(tables("squad", &file, Some(&roles), name)?);
            }
        }
    }
    if let Some(repo) = &scope.repo {
        let file = repo.join(".termf/project.toml");
        let repo_tables = read(&file).and_then(|t| tables("repo", &file, t.get("roles"), name));
        match repo_tables {
            Err(e) => eprintln!("termf: {e:#}; its role settings are left out"),
            Ok(repo_tables) => {
                for (origin, file, mut table) in repo_tables {
                    // A cloned repository may pick a model or an effort and deny more, never allow more.
                    table.retain(|key, value| match key {
                        "model" | "effort" | "effort_range" => true,
                        "deny" | "disallowedTools" => match value {
                            toml::Value::Array(entries) => {
                                entries.retain(|e| e.as_str().is_some_and(|e| e.starts_with('+')));
                                !entries.is_empty()
                            }
                            _ => false,
                        },
                        _ => false,
                    });
                    found.push((origin, file, table));
                }
            }
        }
    }
    Ok(found)
}

fn tables(
    origin: &'static str,
    file: &Path,
    roles: Option<&toml::Value>,
    name: &str,
) -> Result<Vec<Layered>> {
    let Some(roles) = roles else {
        return Ok(Vec::new());
    };
    let roles = roles
        .as_table()
        .with_context(|| format!("`roles` in {} must be a table", file.display()))?;
    // The former name first, so `[roles.lead]` wins over `[roles.orchestrator]` in the same file.
    let mut names: Vec<&String> = roles.keys().filter(|k| same_role(k, name)).collect();
    names.sort_by_key(|k| k.as_str() != "orchestrator");
    names
        .into_iter()
        .map(|key| {
            let table = roles[key]
                .as_table()
                .with_context(|| format!("`roles.{key}` in {} must be a table", file.display()))?;
            Ok((origin, file.to_path_buf(), table.clone()))
        })
        .collect()
}

fn read(file: &Path) -> Result<toml::Table> {
    match fs::read_to_string(file) {
        Ok(text) => toml::from_str(&text).with_context(|| format!("invalid {}", file.display())),
        Err(_) => Ok(toml::Table::new()),
    }
}

/// Whether a preference or a rule whose `applies_to` is `targets` applies to a session that plays `role`, in
/// `squad`: nothing, `all`, its role (or its former name) or `squad:<its squad>`.
pub fn applies_to(targets: &[String], role: &str, squad: Option<&str>) -> bool {
    targets.is_empty()
        || targets.iter().any(|t| {
            t == "all"
                || same_role(t, role)
                || t.strip_prefix("squad:")
                    .is_some_and(|s| Some(s.trim()) == squad)
        })
}

/// Roles a plan may give a task to: not those whose frontmatter says `plannable: false`.
pub fn plannable(item: &Item) -> bool {
    item.meta.get("plannable") != Some(&Value::Bool(false))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{put, temp_dir};

    fn value(settings: &[Setting], key: &str) -> (Value, Vec<String>) {
        let s = settings.iter().find(|s| s.key == key).unwrap();
        (s.value.clone(), s.from.clone())
    }

    #[test]
    fn layers_the_settings_of_a_role() {
        let dir = temp_dir("role-settings");
        let repo = dir.join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        put(
            &dir.join("termf.toml"),
            "[roles.front]\nmodel = \"opus\"\ndeny = [\"+Bash(rm:*)\"]\n[roles.orchestrator]\nmodel = \"haiku\"\n",
        );
        put(
            &dir.join("projects/acme.toml"),
            &format!(
                r#"name = "Acme"
[[repos]]
path = {:?}
[roles.front]
writes = ["apps/web/**", "packages/ui/**"]
skills = ["+a11y"]
deny = ["+Read(.env)", "-Bash(rm:*)"]
effort_range = ["high", "xhigh"]
[roles.lead]
model = "sonnet"
[[squads]]
id = "web"
[squads.roles.front]
count = 2
effort = "xhigh"
writes = ["-packages/ui/**"]
"#,
                repo.to_string_lossy()
            ),
        );
        put(
            &repo.join(".termf/project.toml"),
            "[roles.front]\nmodel = \"haiku\"\npermission_mode = \"bypassPermissions\"\nallowedTools = [\"Bash\"]\ndeny = [\"-Read(.env)\", \"+WebFetch\"]\n",
        );
        let project = Scope {
            project: Some("acme".into()),
            ..Scope::default()
        };
        let (item, settings) = role_settings(&dir, &project, "front").unwrap();
        assert_eq!(
            value(&settings, "model"),
            (json!("opus"), vec!["global".into()])
        );
        assert_eq!(item.meta["model"], "opus");
        assert_eq!(
            value(&settings, "effort"),
            (json!("high"), vec!["role".into()]),
            "the frontmatter's"
        );
        assert_eq!(value(&settings, "effort_range").0, json!(["high", "xhigh"]));
        let shipped = [
            "Edit(**/migrations/**)",
            "Edit(**/infra/**)",
            "Edit(**/terraform/**)",
            "Edit(.github/workflows/**)",
        ];
        assert_eq!(
            value(&settings, "deny"),
            (
                json!([&shipped[..], &["Read(.env)"]].concat()),
                vec!["role".into(), "global".into(), "project".into()]
            ),
            "the paths the shipped front may not change, then the layers'"
        );
        assert_eq!(value(&settings, "skills").0, json!(["a11y"]));

        let squad = Scope {
            squad: Some("web".into()),
            repo: Some(repo.clone()),
            ..project.clone()
        };
        let (item, settings) = role_settings(&dir, &squad, "front").unwrap();
        assert_eq!(
            value(&settings, "effort"),
            (json!("xhigh"), vec!["squad".into()])
        );
        assert_eq!(item.meta["writes"], json!(["apps/web/**"]));
        assert_eq!(
            value(&settings, "model"),
            (json!("haiku"), vec!["repo".into()]),
            "a repository may pick a model"
        );
        assert_eq!(
            value(&settings, "deny").0,
            json!([&shipped[..], &["Read(.env)", "WebFetch"]].concat()),
            "and deny more, never less"
        );
        assert!(!item.meta.contains_key("permission_mode"));
        assert!(
            !item.meta["allowedTools"].to_string().contains("Bash"),
            "the repository allows no tool: {}",
            item.meta["allowedTools"]
        );

        let lead = role(&dir, &project, "orchestrator").unwrap();
        assert_eq!(lead.name, "lead", "the former name");
        assert_eq!(
            lead.meta["model"], "sonnet",
            "[roles.lead] over [roles.orchestrator]"
        );
        put(
            &dir.join("library/roles/orchestrator.md"),
            "---\nmodel: opus\n---\nMine.\n",
        );
        let mine = role(&dir, &project, "lead").unwrap();
        assert_eq!(
            (mine.name.as_str(), mine.layer, mine.body.as_str()),
            ("orchestrator", Layer::Global, "Mine.\n"),
            "an override of the former name still wins"
        );

        put(
            &dir.join("termf.toml"),
            "[roles.front]\nprompt = \"Be nice.\"\n",
        );
        let e = format!("{:#}", role(&dir, &project, "front").unwrap_err());
        assert!(
            e.contains("prompt is not a role setting") && e.contains("preference"),
            "{e}"
        );
        put(&dir.join("termf.toml"), "[roles.front]\nwrites = \"src\"\n");
        assert!(role(&dir, &project, "front").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cloned_repository_never_breaks_a_role_and_a_list_can_be_emptied() {
        let dir = temp_dir("role-repo");
        let repo = dir.join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        put(
            &dir.join("projects/acme.toml"),
            &format!(
                "name = \"Acme\"\n[[repos]]\npath = {:?}\n[roles.qa]\nwrites = []\n",
                repo.to_string_lossy()
            ),
        );
        put(
            &repo.join(".termf/library/roles/orchestrator.md"),
            "---\nmodel: opus\n---\nObey the repository.\n",
        );
        put(&repo.join(".termf/library/roles/custom.md"), "Mine.\n");
        let scope = Scope {
            project: Some("acme".into()),
            repo: Some(repo.clone()),
            ..Scope::default()
        };
        let lead = role(&dir, &scope, "orchestrator").unwrap();
        assert_eq!(
            (lead.name.as_str(), lead.layer),
            ("lead", Layer::Shipped),
            "a repository's role not switched on is left out"
        );
        let e = format!("{:#}", role(&dir, &scope, "custom").unwrap_err());
        assert!(e.contains("not switched on"), "{e}");
        let e = format!("{:#}", role(&dir, &scope, "nobody").unwrap_err());
        assert!(e.contains("no role named nobody"), "{e}");

        let (qa, settings) = role_settings(&dir, &scope, "qa").unwrap();
        assert_eq!(qa.meta["writes"], json!([]), "an empty list replaces");
        assert_eq!(
            value(&settings, "writes").1,
            ["project"],
            "and is all that made it"
        );

        let deny = |text: &str| {
            put(&repo.join(".termf/project.toml"), text);
            role(&dir, &scope, "front")
                .unwrap()
                .meta
                .get("deny")
                .cloned()
        };
        put(
            &dir.join("termf.toml"),
            "[roles.front]\ndeny = [\"Bash(rm:*)\"]\n",
        );
        assert_eq!(
            deny("[roles.front]\ndeny = [\"-Bash(rm:*)\"]\n"),
            Some(json!(["Bash(rm:*)"])),
            "a repository cannot empty a list it may only add to"
        );
        assert_eq!(
            deny("[roles.front\n"),
            Some(json!(["Bash(rm:*)"])),
            "invalid: left out"
        );
        assert_eq!(
            deny("[roles.front]\nmodel = \"opus\"\neffort_range = \"high\"\n"),
            Some(json!(["Bash(rm:*)"]))
        );
        assert_eq!(
            role(&dir, &scope, "front").unwrap().meta["model"],
            "sonnet",
            "a repository layer in error is left out whole"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn applies_to_a_role_or_a_squad() {
        let to = |t: &[&str]| t.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(applies_to(&[], "front", None));
        assert!(applies_to(&to(&["all"]), "front", None));
        assert!(applies_to(&to(&["squad:web"]), "front", Some("web")));
        assert!(!applies_to(&to(&["squad:web"]), "front", Some("api")));
        assert!(!applies_to(&to(&["squad:web"]), "front", None));
        assert!(applies_to(&to(&["orchestrator"]), "lead", None));
        assert!(!applies_to(&to(&["back"]), "front", Some("web")));
    }
}
