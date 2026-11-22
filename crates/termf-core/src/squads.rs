//! A project's team (ARCHITECTURE §7): its squads, `[[squads]]` in the project's file, and its staff, `[team]
//! staff`. Without a squad declared, the team is one implicit squad, `main`, that takes every role.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

pub const MAIN: &str = "main";

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Squad {
    pub id: String,
    /// The role that leads it.
    pub lead: String,
    /// Its roles, each with how many of its sessions may run at once. Empty: every role, without limit.
    pub roles: BTreeMap<String, usize>,
    /// The repositories it works on, by their name in the project. Empty: any.
    pub repos: Vec<String>,
    /// Globs relative to a repository, what it may change. Empty: anything.
    pub paths: Vec<String>,
    /// Words that tell the planner what it owns.
    pub domains: Vec<String>,
    /// How many of its tasks may run at once, within the workflow's limit.
    pub max_parallel: Option<usize>,
    /// `[squads.roles.<name>]`: what its roles change in their settings, besides their count.
    #[serde(skip)]
    pub settings: BTreeMap<String, toml::Table>,
}

impl Squad {
    /// Whether `role` belongs to it. `orchestrator` is the former name of `lead`.
    pub fn has(&self, role: &str) -> bool {
        self.roles.is_empty() || self.roles.keys().any(|r| same_role(r, role))
    }

    /// How many sessions of `role` it runs at once, when it says.
    pub fn count(&self, role: &str) -> Option<usize> {
        self.roles
            .iter()
            .find_map(|(r, n)| same_role(r, role).then_some(*n))
    }
}

/// `orchestrator`, the former name of `lead`, still names it everywhere.
pub fn canonical_role(name: &str) -> &str {
    match name {
        "orchestrator" => "lead",
        _ => name,
    }
}

pub fn same_role(a: &str, b: &str) -> bool {
    canonical_role(a) == canonical_role(b)
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    /// Whether the project declares its squads; otherwise `squads` is the implicit `main`.
    pub declared: bool,
    pub squads: Vec<Squad>,
    /// The roles outside the squads, at the service of all.
    pub staff: Vec<String>,
}

pub enum Membership<'a> {
    Squad(&'a Squad),
    Staff,
}

impl Team {
    /// The squad a task names: `""` is the only squad when there is one, else the staff.
    pub fn squad(&self, id: &str) -> Result<Membership<'_>, String> {
        let ids = || {
            self.squads
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        match (id, self.squads.as_slice()) {
            ("", [only]) => Ok(Membership::Squad(only)),
            ("", _) => Ok(Membership::Staff),
            (id, squads) => squads
                .iter()
                .find(|s| s.id == id)
                .map(Membership::Squad)
                .ok_or_else(|| format!("no squad {id} in the project (its squads: {})", ids())),
        }
    }

    /// Where a task of `role` that names `id` works: as [`Team::squad`], but `""` stays with the staff for a
    /// staff role that the only squad does not have.
    pub fn place(&self, id: &str, role: &str) -> Result<Membership<'_>, String> {
        let staff = self.staff.iter().any(|r| same_role(r, role));
        match self.squad(id)? {
            Membership::Squad(squad) if id.is_empty() && staff && !squad.has(role) => {
                Ok(Membership::Staff)
            }
            found => Ok(found),
        }
    }
}

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    squads: Vec<Entry>,
    #[serde(default)]
    team: TeamEntry,
}

#[derive(Deserialize, Default)]
struct TeamEntry {
    #[serde(default)]
    staff: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: String,
    lead: Option<String>,
    #[serde(default)]
    roles: toml::Table,
    #[serde(default)]
    repos: Vec<String>,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    domains: Vec<String>,
    max_parallel: Option<usize>,
}

/// The team of `project`, from its file.
pub fn load_team(config_dir: &Path, project: &str) -> Result<Team> {
    let path = crate::project_file(config_dir, project)?;
    parse(&fs::read_to_string(&path)?).with_context(|| format!("invalid {}", path.display()))
}

/// The team a project's file declares.
pub fn parse(text: &str) -> Result<Team> {
    let file: File = toml::from_str(text)?;
    let declared = !file.squads.is_empty();
    let mut squads: Vec<Squad> = Vec::new();
    for entry in file.squads {
        let id = entry.id.trim().to_owned();
        ensure!(
            !id.is_empty()
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_".contains(c)),
            "squad id {id:?} is not made of letters, digits, - or _"
        );
        ensure!(
            !squads.iter().any(|s| s.id == id),
            "two squads are named {id}"
        );
        for path in &entry.paths {
            let rooted = Path::new(path).is_absolute() || path.starts_with(['/', '\\']);
            ensure!(
                !rooted && !path.split(['/', '\\']).any(|c| c == ".."),
                "squad {id}: {path} is not a path inside a repository"
            );
        }
        ensure!(
            entry.max_parallel != Some(0),
            "squad {id}: max_parallel must be at least 1"
        );
        let mut roles = BTreeMap::new();
        let mut settings = BTreeMap::new();
        for (role, value) in entry.roles {
            // `front = 2`, or `[squads.roles.front]` with `count` and the role's settings in this squad.
            let count = match value {
                toml::Value::Integer(n) => n,
                toml::Value::Table(mut table) => {
                    let count = match table.remove("count") {
                        None => 1,
                        Some(toml::Value::Integer(n)) => n,
                        Some(_) => bail!("squad {id}: roles.{role}.count must be a number"),
                    };
                    settings.insert(role.clone(), table);
                    count
                }
                _ => bail!(
                    "squad {id}: roles.{role} must be a number of sessions, or a table of settings"
                ),
            };
            ensure!(count >= 1, "squad {id}: roles.{role} must be at least 1");
            roles.insert(role, count as usize);
        }
        squads.push(Squad {
            lead: entry
                .lead
                .filter(|l| !l.trim().is_empty())
                .unwrap_or_else(|| "lead".into()),
            id,
            roles,
            repos: entry.repos,
            paths: entry.paths,
            domains: entry.domains,
            max_parallel: entry.max_parallel,
            settings,
        });
    }
    if !declared {
        squads.push(Squad {
            id: MAIN.into(),
            lead: "lead".into(),
            roles: BTreeMap::new(),
            repos: Vec::new(),
            paths: Vec::new(),
            domains: Vec::new(),
            max_parallel: None,
            settings: BTreeMap::new(),
        });
    }
    let mut staff: Vec<String> = Vec::new();
    for role in file.team.staff {
        if !staff.iter().any(|r| same_role(r, &role)) {
            staff.push(role);
        }
    }
    Ok(Team {
        declared,
        squads,
        staff,
    })
}

/// A squad as its editor sets it. The rest of its table (`max_parallel`, its roles' settings) stays as it is.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SquadForm {
    pub id: String,
    /// Empty: the default lead.
    pub lead: String,
    pub roles: BTreeMap<String, usize>,
    pub repos: Vec<String>,
    pub paths: Vec<String>,
    pub domains: Vec<String>,
}

/// The `[[squads]]` tables of a project's file, made if `make` and missing.
fn squad_tables(
    doc: &mut toml_edit::DocumentMut,
    make: bool,
) -> Result<Option<&mut toml_edit::ArrayOfTables>> {
    if make && !doc.contains_key("squads") {
        doc.insert("squads", toml_edit::Item::ArrayOfTables(Default::default()));
    }
    match doc.get_mut("squads") {
        None => Ok(None),
        Some(item) => item.as_array_of_tables_mut().map(Some).context(
            "`squads` in the project's file is not written as [[squads]] tables: edit it by hand",
        ),
    }
}

fn position(squads: &toml_edit::ArrayOfTables, id: &str) -> Option<usize> {
    squads
        .iter()
        .position(|t| t.get("id").and_then(|v| v.as_str()) == Some(id))
}

/// Sets `key` to `value` in place, keeping the comments around the old value; `None` removes it.
fn put(table: &mut dyn toml_edit::TableLike, key: &str, value: Option<toml_edit::Value>) {
    let Some(mut value) = value else {
        table.remove(key);
        return;
    };
    if let Some(old) = table.get(key).and_then(|i| i.as_value()) {
        *value.decor_mut() = old.decor().clone();
    }
    table.insert(key, toml_edit::Item::Value(value));
}

fn strings(list: &[String]) -> Option<toml_edit::Value> {
    (!list.is_empty()).then(|| {
        list.iter()
            .map(String::as_str)
            .collect::<toml_edit::Array>()
            .into()
    })
}

/// The project's file `text` with the squad `from` (a new one when `None`) set as `squad` says: only its table
/// changes, the rest of the file and its comments stay. Read back before it is returned.
pub fn save(text: &str, from: Option<&str>, squad: &SquadForm) -> Result<String> {
    let id = squad.id.trim();
    ensure!(
        !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_".contains(c)),
        "squad id {id:?} is not made of letters, digits, - or _"
    );
    let mut doc: toml_edit::DocumentMut = text.parse().context("invalid project file")?;
    let squads = squad_tables(&mut doc, true)?.expect("just made");
    let at = match from {
        Some(from) => Some(
            position(squads, from).with_context(|| format!("no squad {from} in the project"))?,
        ),
        None => None,
    };
    ensure!(
        position(squads, id).is_none_or(|i| Some(i) == at),
        "two squads are named {id}"
    );
    let table = match at {
        Some(i) => squads.get_mut(i).expect("found"),
        None => {
            squads.push(toml_edit::Table::new());
            squads.get_mut(squads.len() - 1).expect("just pushed")
        }
    };
    put(table, "id", Some(id.into()));
    put(
        table,
        "lead",
        Some(squad.lead.trim())
            .filter(|l| !l.is_empty())
            .map(Into::into),
    );
    // `front = 2`, or `[squads.roles.front]` whose `count` changes and whose settings stay.
    let roles = table.get_mut("roles").and_then(|r| r.as_table_like_mut());
    match roles {
        Some(roles) => {
            let gone: Vec<String> = roles
                .iter()
                .map(|(k, _)| k.to_owned())
                .filter(|k| !squad.roles.contains_key(k))
                .collect();
            for role in gone {
                roles.remove(&role);
            }
            for (role, n) in &squad.roles {
                let count = toml_edit::Value::from(*n as i64);
                match roles.get_mut(role).and_then(|r| r.as_table_like_mut()) {
                    Some(settings) if *n == 1 && !settings.contains_key("count") => {}
                    Some(settings) => put(settings, "count", Some(count)),
                    None => put(roles, role, Some(count)),
                }
            }
            if roles.is_empty() {
                table.remove("roles");
            }
        }
        None if squad.roles.is_empty() => {}
        None => {
            let roles: toml_edit::InlineTable = squad
                .roles
                .iter()
                .map(|(r, n)| (r.as_str(), toml_edit::Value::from(*n as i64)))
                .collect();
            table.insert("roles", toml_edit::value(roles));
        }
    }
    for (key, list) in [
        ("repos", &squad.repos),
        ("paths", &squad.paths),
        ("domains", &squad.domains),
    ] {
        put(table, key, strings(list));
    }
    let after = doc.to_string();
    parse(&after).context("the squads would not read back")?;
    Ok(after)
}

/// The project's file `text` without the squad `id`, its roles' settings included; the rest stays.
pub fn remove(text: &str, id: &str) -> Result<String> {
    let mut doc: toml_edit::DocumentMut = text.parse().context("invalid project file")?;
    let squads =
        squad_tables(&mut doc, false)?.with_context(|| format!("no squad {id} in the project"))?;
    let at = position(squads, id).with_context(|| format!("no squad {id} in the project"))?;
    squads.remove(at);
    if squads.is_empty() {
        doc.remove("squads");
    }
    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_one_squad_and_keeps_the_rest_of_the_file() {
        let text = r#"# Acme, by alex
name = "Acme"

[[squads]]
# The web part
id = "web"
roles = { front = 2 } # two at once
paths = ["web/**"]
max_parallel = 2

[[squads]]
id = "api"
[squads.roles]
devops = 1
[squads.roles.back]
count = 2
effort = "xhigh" # as decided
"#;
        let form = |id: &str, roles: &[(&str, usize)]| SquadForm {
            id: id.into(),
            roles: roles.iter().map(|(r, n)| (r.to_string(), *n)).collect(),
            ..SquadForm::default()
        };
        let mut api = form("backend", &[("back", 3), ("qa", 1)]);
        api.repos = vec!["acme-api".into()];
        let after = save(text, Some("api"), &api).unwrap();
        assert!(after.starts_with("# Acme, by alex\nname = \"Acme\"\n\n[[squads]]\n# The web part\nid = \"web\"\nroles = { front = 2 } # two at once\n"), "{after}");
        assert!(after.contains("max_parallel = 2"));
        assert!(after.contains("effort = \"xhigh\" # as decided"), "{after}");
        let team = parse(&after).unwrap();
        let backend = &team.squads[1];
        assert_eq!(backend.id, "backend", "renamed in place");
        assert_eq!(
            (
                backend.count("back"),
                backend.count("qa"),
                backend.has("devops")
            ),
            (Some(3), Some(1), false)
        );
        assert_eq!(backend.settings["back"]["effort"].as_str(), Some("xhigh"));
        assert_eq!(backend.repos, ["acme-api"]);

        let mut web = form("web", &[("front", 1)]);
        web.lead = "lead".into();
        web.domains = vec!["UI".into()];
        let after = save(&after, Some("web"), &web).unwrap();
        assert!(
            after.contains("roles = { front = 1 } # two at once"),
            "the comment stays: {after}"
        );
        assert!(!after.contains("paths"), "no paths: the whole repository");
        assert_eq!(parse(&after).unwrap().squads[0].domains, ["UI"]);

        assert!(
            format!("{:#}", save(&after, None, &form("web", &[])).unwrap_err())
                .contains("two squads are named web")
        );
        assert!(
            format!(
                "{:#}",
                save(&after, Some("ops"), &form("ops", &[])).unwrap_err()
            )
            .contains("no squad ops")
        );
        assert!(
            format!("{:#}", save(&after, None, &form("a b", &[])).unwrap_err())
                .contains("not made of")
        );
        let mut outside = form("ops", &[]);
        outside.paths = vec!["../x".into()];
        assert!(
            format!("{:#}", save(&after, None, &outside).unwrap_err())
                .contains("not a path inside")
        );

        let after = remove(&after, "backend").unwrap();
        assert!(
            !after.contains("xhigh") && !after.contains("devops"),
            "its settings go with it: {after}"
        );
        assert_eq!(parse(&after).unwrap().squads.len(), 1);
        let none = remove(&after, "web").unwrap();
        assert_eq!(none, "# Acme, by alex\nname = \"Acme\"\n");
        assert!(!parse(&none).unwrap().declared);
        assert!(remove(&none, "web").is_err());

        let created = save(&none, None, &form("web", &[("front", 2), ("qa", 1)])).unwrap();
        assert!(
            created.contains("[[squads]]\nid = \"web\"\nroles = { front = 2, qa = 1 }\n"),
            "{created}"
        );
        assert!(save("squads = 1\n", None, &form("web", &[])).is_err());
    }

    #[test]
    fn reads_squads_or_makes_the_implicit_one() {
        let small = parse("name = \"Acme\"\n[[repos]]\npath = \"/x\"\n").unwrap();
        assert!(!small.declared && small.staff.is_empty());
        assert_eq!(small.squads.len(), 1);
        assert_eq!(small.squads[0].id, "main");
        assert!(small.squads[0].has("anything"));
        assert!(matches!(small.squad(""), Ok(Membership::Squad(s)) if s.id == "main"));
        assert!(matches!(small.squad("main"), Ok(Membership::Squad(_))));

        let team = parse(
            r#"name = "Acme"
[team]
staff = ["po", "architect", "po"]

[[squads]]
id = "web"
roles = { front = 2, qa = 1 }
repos = ["front"]
paths = ["apps/web/**"]
domains = ["UI"]
max_parallel = 2

[[squads]]
id = "api"
lead = "orchestrator"
[squads.roles]
devops = 1
[squads.roles.back]
count = 2
effort = "xhigh"
"#,
        )
        .unwrap();
        assert!(team.declared);
        assert_eq!(team.staff, ["po", "architect"], "once each");
        let web = &team.squads[0];
        assert_eq!((web.lead.as_str(), web.max_parallel), ("lead", Some(2)));
        assert_eq!(web.count("front"), Some(2));
        assert!(web.has("qa") && !web.has("back"));
        let api = &team.squads[1];
        assert_eq!(api.count("back"), Some(2));
        assert_eq!(api.settings["back"]["effort"].as_str(), Some("xhigh"));
        assert!(!api.settings["back"].contains_key("count"));
        assert!(matches!(team.squad(""), Ok(Membership::Staff)));
        assert!(
            team.squad("ops")
                .err()
                .unwrap()
                .contains("no squad ops in the project (its squads: web, api)")
        );
        assert!(same_role(&api.lead, "lead"));
        let one = parse(
            "[team]\nstaff = [\"architect\"]\n[[squads]]\nid = \"web\"\nroles = { front = 1 }\n",
        )
        .unwrap();
        assert!(matches!(one.place("", "front"), Ok(Membership::Squad(s)) if s.id == "web"));
        assert!(
            matches!(one.place("", "architect"), Ok(Membership::Staff)),
            "a staff role the only squad does not have"
        );
        assert!(matches!(
            one.place("web", "architect"),
            Ok(Membership::Squad(_))
        ));

        for (bad, why) in [
            ("[[squads]]\nid = \"a b\"\n", "is not made of"),
            (
                "[[squads]]\nid = \"a\"\n[[squads]]\nid = \"a\"\n",
                "two squads",
            ),
            ("[[squads]]\nid = \"a\"\npaths = [\"../x\"]\n", "not a path"),
            (
                "[[squads]]\nid = \"a\"\nroles = { front = 0 }\n",
                "at least 1",
            ),
            (
                "[[squads]]\nid = \"a\"\nroles = { front = \"x\" }\n",
                "number of sessions",
            ),
            ("[[squads]]\nid = \"a\"\nmax_parallel = 0\n", "at least 1"),
            ("[[squads]]\nid = \"a\"\nleader = \"x\"\n", "unknown field"),
        ] {
            let e = format!("{:#}", parse(bad).unwrap_err());
            assert!(e.contains(why), "{bad}: {e}");
        }
    }
}
