//! What a workflow's YAML file says: the inputs of its manual trigger, and its jobs with what each one needs.

use anyhow::{Context, Result, bail};
use serde::Serialize;
use yaml_rust2::{Yaml, YamlLoader};

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Input {
    pub name: String,
    pub description: Option<String>,
    /// `string`, `boolean`, `choice`, `number` or `environment`.
    pub kind: String,
    pub required: bool,
    pub default: Option<String>,
    /// The values a `choice` takes.
    pub options: Vec<String>,
}

fn scalar(value: &Yaml) -> Option<String> {
    match value {
        Yaml::String(s) | Yaml::Real(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

/// The inputs of the workflow's `workflow_dispatch` trigger; refused if it has none.
pub fn parse_inputs(text: &str) -> Result<Vec<Input>> {
    let docs = YamlLoader::load_from_str(text).context("the workflow is not valid YAML")?;
    let on = docs
        .first()
        .map(|doc| &doc["on"])
        .unwrap_or(&Yaml::BadValue);
    let none = || "the workflow cannot be started by hand: it has no workflow_dispatch trigger";
    let dispatch = match on {
        Yaml::String(event) if event == "workflow_dispatch" => return Ok(Vec::new()),
        Yaml::Array(events)
            if events
                .iter()
                .any(|e| e.as_str() == Some("workflow_dispatch")) =>
        {
            return Ok(Vec::new());
        }
        Yaml::Hash(events) => events
            .get(&Yaml::String("workflow_dispatch".into()))
            .with_context(none)?,
        _ => bail!(none()),
    };
    let Yaml::Hash(inputs) = &dispatch["inputs"] else {
        return Ok(Vec::new());
    };
    Ok(inputs
        .iter()
        .filter_map(|(name, spec)| {
            let options = spec["options"].as_vec().into_iter().flatten();
            Some(Input {
                name: scalar(name)?,
                description: scalar(&spec["description"]),
                kind: scalar(&spec["type"]).unwrap_or_else(|| "string".into()),
                required: spec["required"].as_bool().unwrap_or(false),
                default: scalar(&spec["default"]),
                options: options.filter_map(scalar).collect(),
            })
        })
        .collect())
}

/// A job of a workflow file: its key, the name GitHub shows for it, and the keys of the jobs it waits for.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub key: String,
    /// Its `name` up to the first expression, else its key: a matrix job shows as `name (values)`.
    pub name: String,
    /// The name had an expression: GitHub's names only start with `name`.
    pub partial: bool,
    pub needs: Vec<String>,
}

/// The jobs of a workflow, in the file's order.
pub fn parse_graph(text: &str) -> Result<Vec<Node>> {
    let docs = YamlLoader::load_from_str(text).context("the workflow is not valid YAML")?;
    let Some(Yaml::Hash(jobs)) = docs.first().map(|doc| &doc["jobs"]) else {
        return Ok(Vec::new());
    };
    Ok(jobs
        .iter()
        .filter_map(|(key, job)| {
            let key = scalar(key)?;
            let given = scalar(&job["name"]).unwrap_or_default();
            let start = given.split("${{").next().unwrap_or_default().trim();
            let needs = match &job["needs"] {
                Yaml::Array(list) => list.iter().filter_map(scalar).collect(),
                one => scalar(one).into_iter().collect(),
            };
            Some(Node {
                name: if start.is_empty() {
                    key.clone()
                } else {
                    start.to_owned()
                },
                partial: given.contains("${{"),
                key,
                needs,
            })
        })
        .collect())
}
