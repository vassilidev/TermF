//! What a review captures: the project's routes, the views the agents declared in their reports, and the
//! Storybook stories the change touched; each in its viewports and themes.

use std::fs;
use std::path::Path;

use termf_team::plan::View;

use crate::config::Config;
use crate::{Decision, Screen, Source};

pub struct Viewport {
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
    pub mobile: bool,
}

pub fn viewport(name: &str) -> Viewport {
    let name = name.trim().to_lowercase();
    if name.contains("mobile") || name.contains("phone") {
        Viewport {
            name: "mobile",
            width: 390,
            height: 844,
            mobile: true,
        }
    } else if name.contains("tablet") || name.contains("ipad") {
        Viewport {
            name: "tablet",
            width: 834,
            height: 1112,
            mobile: true,
        }
    } else {
        Viewport {
            name: "desktop",
            width: 1440,
            height: 900,
            mobile: false,
        }
    }
}

/// An agent may name the host of its own dev server: only the path (and query) is the same on TermF's.
fn path_of(url: &str) -> String {
    let url = url.trim();
    let rest = match url.split_once("://") {
        Some((_, rest)) => rest.find('/').map_or("/", |i| &rest[i..]),
        None if host(url) => url.find('/').map_or("/", |i| &url[i..]),
        None => url,
    };
    let rest = rest.split('#').next().unwrap_or_default();
    match rest.starts_with('/') {
        true => rest.to_owned(),
        false => format!("/{rest}"),
    }
}

/// Whether a URL without a scheme starts with a host: `localhost`, a `:port`, an IPv4 address or a domain name
/// followed by a path.
fn host(url: &str) -> bool {
    let first = url.split('/').next().unwrap_or_default();
    if url.starts_with('/') || first.is_empty() {
        return false;
    }
    let (name, port) = first.split_once(':').unwrap_or((first, ""));
    if !port.is_empty() {
        return port.chars().all(|c| c.is_ascii_digit());
    }
    let labels: Vec<&str> = name.split('.').collect();
    let ipv4 = labels.len() == 4 && labels.iter().all(|l| l.parse::<u8>().is_ok());
    let domain = labels.len() > 1
        && labels
            .iter()
            .all(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        && labels
            .last()
            .is_some_and(|l| l.chars().all(|c| c.is_ascii_alphabetic()));
    name == "localhost" || ipv4 || (domain && url.len() > first.len())
}

fn slug(path: &str) -> String {
    let s: String = path
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' => c,
            'A'..='Z' => c.to_ascii_lowercase(),
            _ => '-',
        })
        .collect();
    let s = s
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    match s.is_empty() {
        true => "home".into(),
        false => s.chars().take(40).collect(),
    }
}

/// The screens to capture, and what could not be. `only`: the routes asked for, instead of the project's and the
/// agents'. `stories`: the ids of the stories touched.
pub fn plan(
    config: &Config,
    views: &[View],
    only: &[String],
    stories: &[String],
) -> (Vec<Screen>, Vec<String>) {
    let settings = &config.settings;
    let default_viewports = match settings.viewports.is_empty() {
        true => vec!["desktop".to_owned()],
        false => settings.viewports.clone(),
    };
    let themes = match settings.themes.is_empty() {
        true => vec!["light".to_owned()],
        false => settings.themes.clone(),
    };
    let mut wanted: Vec<(Source, String, Option<String>, Vec<String>)> = Vec::new();
    if only.is_empty() {
        for route in settings.routes.iter().map(|r| r.route()) {
            if route.repo.as_ref().is_none_or(|r| *r == config.repo)
                && !route.path.trim().is_empty()
            {
                wanted.push((
                    Source::Route,
                    path_of(&route.path),
                    route.state,
                    route.viewports,
                ));
            }
        }
        for view in views.iter().filter(|v| !v.url.trim().is_empty()) {
            let state = Some(view.state.trim().to_owned()).filter(|s| !s.is_empty());
            wanted.push((
                Source::Agent,
                path_of(&view.url),
                state,
                view.viewports.clone(),
            ));
        }
    } else {
        for path in only {
            wanted.push((Source::Route, path_of(path), None, Vec::new()));
        }
    }
    let mut notes = Vec::new();
    if !stories.is_empty() && only.is_empty() {
        if config.storybook.is_some() {
            for id in stories {
                let path = format!("/iframe.html?id={id}&viewMode=story");
                wanted.push((Source::Storybook, path, Some(id.clone()), Vec::new()));
            }
        } else {
            notes.push(format!(
                "Storybook stories touched but not captured, no `storybook` command for {} in the project: {}",
                config.repo,
                stories.join(", ")
            ));
        }
    }
    let mut screens: Vec<Screen> = Vec::new();
    for (source, path, state, viewports) in wanted {
        let viewports = match viewports.is_empty() {
            true => default_viewports.clone(),
            false => viewports,
        };
        for vp in viewports.iter().map(|v| viewport(v)) {
            for theme in &themes {
                let theme = match theme.trim().to_lowercase().as_str() {
                    "dark" => "dark",
                    _ => "light",
                };
                let same = |s: &Screen| s.path == path && s.viewport == vp.name && s.theme == theme;
                if screens.iter().any(same) {
                    continue;
                }
                let id = format!(
                    "{:02}-{}-{}-{theme}",
                    screens.len() + 1,
                    slug(&path),
                    vp.name
                );
                screens.push(Screen {
                    id,
                    source,
                    path: path.clone(),
                    state: state.clone(),
                    viewport: vp.name.into(),
                    width: vp.width,
                    height: vp.height,
                    theme: theme.into(),
                    before: None,
                    after: None,
                    heatmap: None,
                    rects: Vec::new(),
                    ratio: 0.0,
                    changed: false,
                    texts: Vec::new(),
                    error: None,
                    decision: Decision::Pending,
                    comment: String::new(),
                    pins: Vec::new(),
                    decided: None,
                });
            }
        }
    }
    (screens, notes)
}

/// The ids of the stories in the changed story files of a repository that has Storybook (`.storybook/`): the
/// `title` of the file's default export and its named exports, as Storybook makes them. A file without a
/// `title` (titles from the file path) is left out.
pub fn stories(repo: &Path, changed: &[String]) -> (Vec<String>, Vec<String>) {
    let mut ids = Vec::new();
    let mut notes = Vec::new();
    if !repo.join(".storybook").is_dir() {
        return (ids, notes);
    }
    let story = |f: &&String| {
        let name = f.rsplit('/').next().unwrap_or(f);
        [".tsx", ".ts", ".jsx", ".js", ".svelte", ".vue"]
            .iter()
            .any(|ext| name.contains(".stories.") && name.ends_with(ext))
    };
    for file in changed.iter().filter(story) {
        let Ok(text) = fs::read_to_string(repo.join(file)) else {
            continue;
        };
        let Some(title) = title(&text) else {
            notes.push(format!("{file}: no `title`, its stories are not captured"));
            continue;
        };
        for line in text.lines() {
            let Some(rest) = line.trim_start().strip_prefix("export const ") else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                .collect();
            if !name.is_empty() {
                ids.push(format!("{}--{}", sanitize(&title), sanitize(&words(&name))));
            }
        }
    }
    (ids, notes)
}

fn title(text: &str) -> Option<String> {
    let at = text.find("title:")? + "title:".len();
    let rest = text[at..].trim_start();
    let quote = rest
        .chars()
        .next()
        .filter(|c| ['\'', '"', '`'].contains(c))?;
    let end = rest[1..].find(quote)?;
    Some(rest[1..1 + end].to_owned())
}

/// The story name Storybook makes of an export (lodash's `startCase`): `WithIcon` → `With Icon`, `ABTest` →
/// `AB Test`, `Step2` → `Step 2`, `big_button` → `big button`; its id comes from that name, not the export.
fn words(export: &str) -> String {
    let chars: Vec<char> = export.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' || c == '$' {
            out.push(' ');
            continue;
        }
        if let Some(&prev) = i.checked_sub(1).and_then(|p| chars.get(p)) {
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            let starts = (c.is_uppercase() && (prev.is_lowercase() || prev.is_ascii_digit()))
                || (c.is_uppercase() && prev.is_uppercase() && next_lower)
                || (c.is_ascii_digit() && prev.is_alphabetic())
                || (c.is_alphabetic() && prev.is_ascii_digit());
            if starts {
                out.push(' ');
            }
        }
        out.push(c);
    }
    out
}

/// Storybook's id part: lowercase, every other character a dash, no dash twice or at the ends.
fn sanitize(text: &str) -> String {
    let s: String = text
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    s.split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{RouteEntry, Settings};

    fn config(settings: Settings) -> Config {
        Config {
            project: "acme".into(),
            repo: "web".into(),
            path: "/nowhere".into(),
            dev: Some("serve".into()),
            storybook: None,
            settings,
        }
    }

    #[test]
    fn plans_routes_and_the_agents_views_in_their_viewports_and_themes() {
        let settings = Settings {
            routes: vec![
                RouteEntry::Path("/".into()),
                RouteEntry::Full(crate::config::Route {
                    path: "/admin".into(),
                    repo: Some("api".into()),
                    ..Default::default()
                }),
            ],
            themes: vec!["light".into(), "dark".into()],
            ..Default::default()
        };
        let views = vec![
            View {
                url: "http://localhost:3000/invoices?tab=2#top".into(),
                state: "non-empty list".into(),
                viewports: vec!["desktop".into(), "Mobile".into()],
            },
            View {
                url: "/".into(),
                state: String::new(),
                viewports: vec![],
            },
        ];
        let (screens, notes) = plan(&config(settings), &views, &[], &[]);
        let ids: Vec<&str> = screens.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "01-home-desktop-light",
                "02-home-desktop-dark",
                "03-invoices-tab-2-desktop-light",
                "04-invoices-tab-2-desktop-dark",
                "05-invoices-tab-2-mobile-light",
                "06-invoices-tab-2-mobile-dark",
            ]
        );
        assert_eq!(screens[2].path, "/invoices?tab=2");
        assert_eq!(screens[2].source, Source::Agent);
        assert_eq!(screens[4].width, 390);
        assert!(notes.is_empty());
        let (only, _) = plan(
            &config(Settings::default()),
            &views,
            &["settings".into()],
            &[],
        );
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].path, "/settings");
    }

    #[test]
    fn keeps_the_path_of_a_url_without_its_host() {
        for (url, path) in [
            ("localhost:3000/invoices", "/invoices"),
            ("localhost", "/"),
            ("127.0.0.1:5173/a?b=1", "/a?b=1"),
            ("10.0.0.2/admin", "/admin"),
            ("app.example.com/settings#x", "/settings"),
            ("https://example.com", "/"),
            ("/invoices", "/invoices"),
            ("invoices", "/invoices"),
            ("invoices.html", "/invoices.html"),
            ("docs/v1.2/intro", "/docs/v1.2/intro"),
            ("v1.2/intro", "/v1.2/intro"),
        ] {
            assert_eq!(path_of(url), path, "{url}");
        }
    }

    #[test]
    fn finds_the_stories_a_change_touched() {
        let dir = std::env::temp_dir().join(format!("termf-visual-stories-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".storybook")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("src/Button.stories.tsx"),
            "export default { title: 'Forms/Big Button', component: Button };\nexport const Primary = {};\nexport const WithIcon: Story = {};\nexport const ABTest2 = {};\nexport const big_button = {};\n",
        )
        .unwrap();
        fs::write(
            dir.join("src/Card.stories.tsx"),
            "export const Plain = {};\n",
        )
        .unwrap();
        let changed = [
            "src/Button.stories.tsx",
            "src/Card.stories.tsx",
            "src/Button.tsx",
        ]
        .map(String::from);
        let (ids, notes) = stories(&dir, &changed);
        // Storybook's names: "Primary", "With Icon", "AB Test 2", "big button".
        assert_eq!(
            ids,
            [
                "forms-big-button--primary",
                "forms-big-button--with-icon",
                "forms-big-button--ab-test-2",
                "forms-big-button--big-button"
            ]
        );
        assert_eq!(notes.len(), 1);
        let (screens, notes) = plan(&config(Settings::default()), &[], &[], &ids);
        assert!(screens.is_empty() && notes[0].contains("forms-big-button--primary"));
        let mut with = config(Settings::default());
        with.storybook = Some("storybook dev -p {{ port }}".into());
        let (screens, _) = plan(&with, &[], &[], &ids);
        assert_eq!(
            screens[0].path,
            "/iframe.html?id=forms-big-button--primary&viewMode=story"
        );
        assert_eq!(screens[0].source, Source::Storybook);
        let _ = fs::remove_dir_all(&dir);
    }
}
