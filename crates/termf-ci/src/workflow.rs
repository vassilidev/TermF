//! The inputs of a workflow's manual trigger, read from its YAML file.

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
