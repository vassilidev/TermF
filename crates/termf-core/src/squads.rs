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
    Ok(Team {
        declared,
        squads,
        staff: file.team.staff,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
staff = ["po", "architect"]

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
        assert_eq!(team.staff, ["po", "architect"]);
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
