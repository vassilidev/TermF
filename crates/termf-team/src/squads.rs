//! A project's team as the CLI and the UI show it: its squads and its staff, each role with its settings as the
//! layers make them, and where each comes from.

use std::path::Path;

use anyhow::Result;
use serde::Serialize;
use termf_core::squads::{Squad, canonical_role, load_team};
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
                member.credits = crate::list(item.meta.get("credits"));
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
