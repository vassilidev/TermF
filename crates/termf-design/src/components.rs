//! The interface's components: Svelte, Vue and Astro files, and JSX components (a PascalCase file, or any file of
//! a `components` / `ui` folder), with the variants their props or `cva` declare, their uses and their stories.

use std::path::Path;

use crate::jsobj::{self, Node};
use crate::markup;
use crate::{Component, Lang, Read, Variant};

/// Props whose string values are a component's variants.
const PROPS: [&str; 8] = [
    "variant",
    "size",
    "kind",
    "tone",
    "intent",
    "color",
    "appearance",
    "shape",
];
const ROUTES: [&str; 5] = ["pages", "routes", "app", "layouts", "api"];
const FOLDERS: [&str; 4] = ["components", "component", "ui", "widgets"];
const NOT_COMPONENTS: [&str; 6] = ["page", "layout", "error", "loading", "not-found", "index"];

pub(crate) struct Found {
    pub file: usize,
    pub name: String,
    pub framework: &'static str,
    pub variants: Vec<Variant>,
    pub skeleton: Vec<String>,
}

pub(crate) fn found(file: usize, rel: &str, name: &str, kind: &Lang, text: &str) -> Option<Found> {
    let (stem, _) = name.split_once('.')?;
    if stem.starts_with(['+', '_', '[', '(']) {
        return None;
    }
    let dirs: Vec<&str> = rel.split('/').rev().skip(1).collect();
    let routes = dirs.iter().any(|d| ROUTES.contains(d));
    let folder = dirs.iter().any(|d| FOLDERS.contains(d));
    let upper = stem.starts_with(|c: char| c.is_ascii_uppercase());
    // `button/index.tsx` is the button.
    let stem = if stem == "index" && folder {
        *dirs.first()?
    } else {
        stem
    };
    if !upper && (routes || NOT_COMPONENTS.contains(&stem)) {
        return None;
    }
    let (framework, markup_text) = match kind {
        Lang::Component(framework) => (*framework, without_blocks(text)),
        Lang::Script { jsx: true }
            if (upper || folder) && (text.contains("</") || text.contains("/>")) =>
        {
            ("jsx", text.to_owned())
        }
        _ => return None,
    };
    Some(Found {
        file,
        name: markup::pascal(stem),
        framework,
        variants: variants(text),
        skeleton: markup::skeleton(&markup_text),
    })
}

/// The template of a single-file component, its script and style taken out.
fn without_blocks(text: &str) -> String {
    let mut out = String::new();
    let lower = text.to_ascii_lowercase();
    let mut from = 0;
    loop {
        let next = ["<script", "<style"]
            .iter()
            .filter_map(|t| lower[from..].find(t).map(|i| (i + from, *t)))
            .min();
        let Some((at, tag)) = next else {
            out.push_str(&text[from..]);
            return out;
        };
        out.push_str(&text[from..at]);
        let close = format!("</{}", &tag[1..]);
        match lower[at..].find(&close) {
            Some(end) => from = at + end + close.len(),
            None => return out,
        }
    }
}

fn variants(text: &str) -> Vec<Variant> {
    let mut out: Vec<Variant> = Vec::new();
    if let Some((Node::Object(entries), _)) = jsobj::after_key(text, "variants") {
        for (prop, values) in entries {
            if let Node::Object(values) = values {
                let values: Vec<String> = values
                    .into_iter()
                    .map(|(k, _)| k)
                    .filter(|k| k != "...")
                    .collect();
                if values.len() > 1 {
                    out.push(Variant { prop, values });
                }
            }
        }
    }
    for prop in PROPS {
        if out.iter().any(|v| v.prop == prop) {
            continue;
        }
        for (at, _) in text.match_indices(prop) {
            let before = text[..at].chars().next_back();
            if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.') {
                continue;
            }
            let rest = text[at + prop.len()..].trim_start_matches('?').trim_start();
            let Some(rest) = rest.strip_prefix(':') else {
                continue;
            };
            let values = union(rest);
            if values.len() > 1 {
                out.push(Variant {
                    prop: prop.to_owned(),
                    values,
                });
                break;
            }
        }
    }
    out
}

/// The string literals of a TypeScript union: `"primary" | 'ghost'`.
fn union(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text.trim_start().trim_start_matches('|').trim_start();
    while let Some(q) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let Some(end) = rest[1..].find(q) else { break };
        out.push(rest[1..1 + end].to_owned());
        rest = rest[end + 2..].trim_start();
        match rest.strip_prefix('|') {
            Some(r) => rest = r.trim_start(),
            None => break,
        }
    }
    out
}

pub(crate) fn inventory(repo: &Path, read: &Read) -> Vec<Component> {
    let mut list: Vec<Component> = read
        .found
        .iter()
        .map(|f| {
            let rel = &read.files[f.file].rel;
            let (uses, files) = read.tags.get(&f.name).cloned().unwrap_or_default();
            let used_in = files
                .into_iter()
                .filter(|&i| i != f.file)
                .take(10)
                .map(|i| read.files[i].rel.clone())
                .collect();
            let stem = rel
                .rsplit('/')
                .next()
                .and_then(|n| n.split('.').next())
                .unwrap_or("");
            let story: Vec<String> = read
                .stories
                .iter()
                .filter(|s| s.rsplit('/').next().and_then(|n| n.split('.').next()) == Some(stem))
                .cloned()
                .collect();
            let stories = if story.is_empty() {
                Vec::new()
            } else {
                termf_visual::screens::stories(repo, &story).0
            };
            Component {
                name: f.name.clone(),
                file: rel.clone(),
                framework: f.framework,
                variants: f.variants.clone(),
                uses,
                used_in,
                stories,
            }
        })
        .collect();
    list.sort_by(|a, b| b.uses.cmp(&a.uses).then(a.name.cmp(&b.name)));
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_variants_from_unions_and_cva() {
        let svelte = r#"<script lang="ts">
  let { variant = "primary", size }: { variant?: "primary" | "secondary" | 'ghost'; size?:
    | "sm"
    | "lg" } = $props();
</script>
<button class={variant}><slot /></button>"#;
        assert_eq!(
            variants(svelte),
            [
                Variant {
                    prop: "variant".into(),
                    values: vec!["primary".into(), "secondary".into(), "ghost".into()]
                },
                Variant {
                    prop: "size".into(),
                    values: vec!["sm".into(), "lg".into()]
                },
            ]
        );
        let cva = r#"const buttonVariants = cva("inline-flex", { variants: { variant: { default: "bg-primary", destructive: "bg-red" }, size: { sm: "h-8", lg: "h-10" } } });"#;
        let got = variants(cva);
        assert_eq!(got[0].values, ["default", "destructive"]);
        assert_eq!(got[1].prop, "size");
    }

    #[test]
    fn tells_components_from_routes() {
        let jsx = "export function Button() { return <button /> }";
        let script = Lang::Script { jsx: true };
        let svelte = Lang::Component("svelte");
        let name = |rel: &str, kind: &Lang, text: &str| {
            let file = rel.rsplit('/').next().unwrap();
            found(0, rel, file, kind, text).map(|f| f.name)
        };
        assert_eq!(
            name("src/components/Button.tsx", &script, jsx),
            Some("Button".into())
        );
        assert_eq!(
            name("src/components/ui/button.tsx", &script, jsx),
            Some("Button".into())
        );
        assert_eq!(
            name("src/components/date-picker/index.tsx", &script, jsx),
            Some("DatePicker".into())
        );
        assert_eq!(name("src/app/page.tsx", &script, jsx), None);
        assert_eq!(
            name("src/lib/utils.ts", &Lang::Script { jsx: false }, "x"),
            None
        );
        assert_eq!(name("src/routes/+page.svelte", &svelte, ""), None);
        assert_eq!(name("src/routes/settings/form.svelte", &svelte, ""), None);
        assert_eq!(
            name("src/lib/ui/alert-dialog.svelte", &svelte, ""),
            Some("AlertDialog".into())
        );
        let shape = found(0, "a/Card.svelte", "Card.svelte", &svelte, "<script>let a: Array<string> = []</script><div><h2></h2><p></p></div><style>p{}</style>").unwrap();
        assert_eq!(shape.skeleton, ["div", "h2", "p"]);
    }
}
