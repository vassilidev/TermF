//! A project's team as the CLI and the UI show it: its squads and its staff, each role with its settings as the
//! layers make them, and where each comes from.

use std::fs;
use std::path::Path;

use anyhow::{Result, ensure};
use serde::Serialize;
use termf_core::squads::{Squad, SquadForm, canonical_role, load_team, same_role};
use termf_library::{Kind, Layer, Scope, Setting};

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct View {
    /// Whether the project declares its squads; otherwise the one squad is the implicit `main`.
    pub declared: bool,
    pub squads: Vec<SquadView>,
    pub staff: Vec<Member>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SquadView {
    #[serde(flatten)]
    pub squad: Squad,
    /// Its lead first, then its roles; every role a task may have when it names none.
    pub members: Vec<Member>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub role: String,
    pub lead: bool,
    /// How many of its sessions the squad runs at once, when it says.
    pub count: Option<usize>,
    pub description: Option<String>,
    pub layer: Option<Layer>,
    pub plannable: bool,
    /// Where its prompt's borrowed passages come from.
    pub credits: Vec<String>,
    pub settings: Vec<Setting>,
    /// Why the role or its settings cannot be read.
    pub error: Option<String>,
}

pub fn view(config_dir: &Path, project: &str) -> Result<View> {
    let team = load_team(config_dir, project)?;
    let scope = Scope {
        project: Some(project.into()),
        ..Scope::default()
    };
    let assignable: Vec<String> = termf_library::resolve(config_dir, &scope)?
        .into_iter()
        .filter(|i| i.kind == Kind::Role && i.layer != Layer::Repo)
        .filter(|i| termf_library::plannable(i) && canonical_role(&i.name) == i.name)
        .map(|i| i.name)
        .collect();
    let member = |role: &str, lead: bool, count: Option<usize>, squad: Option<&str>| {
        let scope = Scope {
            squad: squad.map(Into::into),
            ..scope.clone()
        };
        let mut member = Member {
            role: role.into(),
            lead,
            count,
            description: None,
            layer: None,
            plannable: false,
            credits: Vec::new(),
            settings: Vec::new(),
            error: None,
        };
        match termf_library::role_settings(config_dir, &scope, role) {
            Ok((item, settings)) => {
                member.plannable = termf_library::plannable(&item);
                member.credits = termf_library::credits(&item.meta);
                member.description = item.description;
                member.layer = Some(item.layer);
                member.settings = settings;
            }
            Err(e) => member.error = Some(format!("{e:#}")),
        }
        member
    };
    let squads = team
        .squads
        .iter()
        .map(|squad| {
            let id = Some(squad.id.as_str());
            let mut members = vec![member(&squad.lead, true, None, id)];
            match squad.roles.is_empty() {
                true => members.extend(assignable.iter().map(|r| member(r, false, None, id))),
                false => members.extend(
                    squad
                        .roles
                        .iter()
                        .map(|(r, n)| member(r, false, Some(*n), id)),
                ),
            }
            SquadView {
                squad: squad.clone(),
                members,
            }
        })
        .collect();
    Ok(View {
        declared: team.declared,
        squads,
        staff: team
            .staff
            .iter()
            .map(|r| member(r, false, None, None))
            .collect(),
    })
}

/// Sets the squad `from` of `project` (a new one when `None`) as `form` says, once its roles are the library's and
/// its repositories the project's: only that squad's table of the project's file changes.
pub fn save(
    config_dir: &Path,
    project: &str,
    from: Option<&str>,
    form: &SquadForm,
) -> Result<View> {
    let scope = Scope {
        project: Some(project.into()),
        ..Scope::default()
    };
    let known: Vec<String> = termf_library::resolve(config_dir, &scope)?
        .into_iter()
        .filter(|i| i.kind == Kind::Role && (i.layer != Layer::Repo || i.active))
        .map(|i| i.name)
        .collect();
    let lead = Some(form.lead.trim()).filter(|l| !l.is_empty());
    for role in lead
        .into_iter()
        .chain(form.roles.keys().map(String::as_str))
    {
        ensure!(
            known.iter().any(|k| same_role(k, role)),
            "no role named {role} in the library"
        );
    }
    let repos: Vec<String> = termf_core::load_projects(config_dir)?
        .into_iter()
        .filter(|p| p.id == project)
        .flat_map(|p| p.repos.into_iter().map(|r| r.name))
        .collect();
    for repo in &form.repos {
        ensure!(repos.contains(repo), "no repository {repo} in the project");
    }
    write(config_dir, project, |text| {
        termf_core::squads::save(text, from, form)
    })
}

/// Removes the squad `id` of `project`, its roles' settings included.
pub fn delete(config_dir: &Path, project: &str, id: &str) -> Result<View> {
    write(config_dir, project, |text| {
        termf_core::squads::remove(text, id)
    })
}

fn write(
    config_dir: &Path,
    project: &str,
    edit: impl FnOnce(&str) -> Result<String>,
) -> Result<View> {
    let file = termf_core::project_file(config_dir, project)?;
    let before = fs::read_to_string(&file)?;
    let after = edit(&before)?;
    if after != before {
        termf_core::write_atomic(&file, &after)?;
    }
    view(config_dir, project)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn shows_squads_staff_and_where_each_setting_comes_from() {
        let dir = std::env::temp_dir().join(format!("termf-squads-view-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("projects")).unwrap();
        fs::write(dir.join("termf.toml"), "[roles.back]\nmodel = \"opus\"\n").unwrap();
        fs::write(
            dir.join("projects/acme.toml"),
            "name = \"Acme\"\n[team]\nstaff = [\"architect\", \"wizard\"]\n[[squads]]\nid = \"api\"\nmax_parallel = 2\n[squads.roles.back]\ncount = 2\neffort = \"xhigh\"\n",
        )
        .unwrap();
        fs::write(dir.join("projects/small.toml"), "name = \"Small\"\n").unwrap();

        let acme = view(&dir, "acme").unwrap();
        assert!(acme.declared);
        let api = &acme.squads[0];
        assert_eq!(api.squad.max_parallel, Some(2));
        let roles: Vec<(&str, bool, Option<usize>)> = api
            .members
            .iter()
            .map(|m| (m.role.as_str(), m.lead, m.count))
            .collect();
        assert_eq!(roles, [("lead", true, None), ("back", false, Some(2))]);
        let back = &api.members[1];
        let setting = |key: &str| back.settings.iter().find(|s| s.key == key).unwrap();
        assert_eq!(
            (&setting("model").value, setting("model").from.as_slice()),
            (&json!("opus"), &["global".to_owned()][..])
        );
        assert_eq!(setting("effort").from, ["squad"]);
        assert!(back.plannable && !api.members[0].plannable);
        assert_eq!(acme.staff[0].role, "architect");
        assert!(
            !acme.staff[0].credits.is_empty(),
            "its prompt credits a source"
        );
        assert!(
            acme.staff[1]
                .error
                .as_deref()
                .unwrap()
                .contains("no role named wizard")
        );

        let small = view(&dir, "small").unwrap();
        assert!(!small.declared);
        let names: Vec<&str> = small.squads[0]
            .members
            .iter()
            .map(|m| m.role.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "lead",
                "a11y",
                "ai",
                "analytics",
                "architect",
                "back",
                "brand",
                "competitive-analyst",
                "copy",
                "dba",
                "devops",
                "docs",
                "front",
                "integrations",
                "legal",
                "motion",
                "qa",
                "researcher",
                "reverse-engineer",
                "seo",
                "sre",
                "ux"
            ]
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn saves_a_squad_of_known_roles_and_repositories() {
        let dir = std::env::temp_dir().join(format!("termf-squads-save-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("projects")).unwrap();
        let file = dir.join("projects/acme.toml");
        fs::write(
            &file,
            "name = \"Acme\"\n# kept\n[[repos]]\nname = \"acme-web\"\npath = \"/nowhere/acme-web\"\n",
        )
        .unwrap();
        let form = |roles: &[&str], repos: &[&str]| SquadForm {
            id: "web".into(),
            roles: roles.iter().map(|r| (r.to_string(), 1)).collect(),
            repos: repos.iter().map(|r| r.to_string()).collect(),
            ..SquadForm::default()
        };
        let e = save(&dir, "acme", None, &form(&["wizard"], &[])).unwrap_err();
        assert!(e.to_string().contains("no role named wizard"), "{e}");
        let e = save(&dir, "acme", None, &form(&["front"], &["acme-api"])).unwrap_err();
        assert!(e.to_string().contains("no repository acme-api"), "{e}");
        assert!(!fs::read_to_string(&file).unwrap().contains("squads"));

        let saved = save(&dir, "acme", None, &form(&["front"], &["acme-web"])).unwrap();
        assert!(saved.declared);
        assert_eq!(saved.squads[0].members[1].role, "front");
        assert!(fs::read_to_string(&file).unwrap().contains("# kept"));
        let gone = delete(&dir, "acme", "web").unwrap();
        assert!(!gone.declared);
        let _ = fs::remove_dir_all(&dir);
    }
}
