//! The fonts a repository uses, as files the view can draw them with: its own `@font-face` files, the
//! `@fontsource` packages it imports (their stylesheet and files read in `node_modules`, never run), and the web
//! font services it links, fetched only when the user asks (`fetch`).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, ensure};
use base64ct::Encoding;
use serde::Serialize;

use crate::{Read, css, markup};

const MAX_FILE: u64 = 2 * 1024 * 1024;
const MAX_TOTAL: u64 = 8 * 1024 * 1024;
const MAX_FACES: usize = 32;
/// The stylesheets of the services, and the hosts their font files come from.
const SERVICES: [&str; 2] = ["fonts.googleapis.com/css", "fonts.bunny.net/css"];
const FILE_HOSTS: [&str; 2] = ["https://fonts.gstatic.com/", "https://fonts.bunny.net/"];

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// Files of the repository, named by its `@font-face`.
    Repo,
    /// An installed `@fontsource` package.
    Package,
    /// A web font service (Google Fonts, Bunny Fonts): nothing loaded until the user asks.
    Service,
    /// Declared or imported, but its files are not there (or too large).
    Missing,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Face {
    pub weight: String,
    pub style: String,
    /// The font file as a data URI.
    pub src: String,
    /// Where it was read: a path in the repository, or the service's URL.
    pub file: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Font {
    pub family: String,
    pub origin: Origin,
    pub faces: Vec<Face>,
    /// Where the project declares or loads it.
    pub file: String,
    pub line: u32,
    /// The service's stylesheet, for `fetch`.
    pub url: Option<String>,
}

#[derive(Default)]
struct Budget {
    bytes: u64,
    faces: usize,
}

impl Budget {
    fn take(&mut self, size: u64) -> bool {
        let ok = size <= MAX_FILE && self.bytes + size <= MAX_TOTAL && self.faces < MAX_FACES;
        if ok {
            self.bytes += size;
            self.faces += 1;
        }
        ok
    }
}

fn mime(name: &str) -> Option<&'static str> {
    let ext = name.split(['?', '#']).next()?.rsplit('.').next()?;
    Some(match ext.to_ascii_lowercase().as_str() {
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => return None,
    })
}

fn data_uri(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64ct::Base64::encode_string(bytes)
    )
}

fn line_at(text: &str, at: usize) -> u32 {
    text[..at].matches('\n').count() as u32 + 1
}

/// One face per weight and style: of a family split by script (`@fontsource`, Bunny), the Latin one.
fn pick(faces: &[css::Face]) -> Vec<&css::Face> {
    let latin = |f: &css::Face| {
        f.src
            .iter()
            .any(|s| s.contains("latin") && !s.contains("latin-ext"))
    };
    let key = |f: &css::Face| (f.family.to_lowercase(), f.weight.clone(), f.style.clone());
    let mut chosen: Vec<&css::Face> = Vec::new();
    for face in faces.iter().filter(|f| !f.family.is_empty()) {
        match chosen.iter().position(|c| key(c) == key(face)) {
            Some(i) if latin(face) && !latin(chosen[i]) => chosen[i] = face,
            Some(_) => {}
            None => chosen.push(face),
        }
    }
    chosen
}

/// A local font file a stylesheet in `dir` names, inside the repository: relative to the sheet, or from the site's
/// root (`/fonts/x.woff2`: the root, `public/` or `static/` of the sheet's folder or of one above it).
fn local(root: &Path, dir: &Path, url: &str) -> Option<PathBuf> {
    let url = url.split(['?', '#']).next()?.replace("%20", " ");
    if url.is_empty() || url.contains(':') || url.starts_with("//") {
        return None;
    }
    let candidates: Vec<PathBuf> = match url.strip_prefix('/') {
        Some(path) => dir
            .ancestors()
            .take_while(|d| d.starts_with(root))
            .flat_map(|d| ["", "public", "static"].map(|s| d.join(s).join(path)))
            .collect(),
        None => vec![dir.join(&url)],
    };
    candidates.into_iter().find_map(|p| {
        let p = p.canonicalize().ok()?;
        (p.starts_with(root) && p.is_file() && mime(&p.to_string_lossy()).is_some()).then_some(p)
    })
}

/// The faces of a stylesheet's `@font-face` with their line, and the file found for each, read within the budget.
fn faces(
    root: &Path,
    dir: &Path,
    sheet: &[css::Face],
    budget: &mut Budget,
) -> Vec<(String, u32, Option<Face>)> {
    pick(sheet)
        .into_iter()
        .map(|f| {
            let face = f.src.iter().find_map(|url| {
                let (src, file) =
                    if url.starts_with("data:font/") || url.starts_with("data:application/font") {
                        (url.clone(), String::new())
                    } else {
                        let path = local(root, dir, url)?;
                        let size = fs::metadata(&path).ok()?.len();
                        if !budget.take(size) {
                            return None;
                        }
                        let name = path.to_string_lossy().into_owned();
                        let file = path
                            .strip_prefix(root)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .replace('\\', "/");
                        (data_uri(mime(&name)?, &fs::read(&path).ok()?), file)
                    };
                Some(Face {
                    weight: f.weight.clone().unwrap_or_else(|| "400".into()),
                    style: f.style.clone().unwrap_or_else(|| "normal".into()),
                    src,
                    file,
                })
            });
            (f.family.clone(), f.line, face)
        })
        .collect()
}

/// `@fontsource/inter`, `@fontsource-variable/inter/wght.css`, as a script or stylesheet imports them.
fn fontsource(text: &str) -> Vec<(usize, String)> {
    text.match_indices("@fontsource")
        .filter_map(|(at, _)| {
            let spec: String = text[at..]
                .chars()
                .take_while(|c| !matches!(c, '"' | '\'' | '`' | ')' | ';' | ',') && !c.is_whitespace())
                .collect();
            let mut parts = spec.split('/');
            let scope = parts.next()?;
            let name = parts.next().filter(|n| !n.is_empty())?;
            (["@fontsource", "@fontsource-variable"].contains(&scope)
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            .then_some((at, spec))
        })
        .collect()
}

/// The package's stylesheet, in the nearest `node_modules` from the importing file's folder up to the root.
fn package(root: &Path, dir: &Path, spec: &str) -> Option<PathBuf> {
    let mut parts = spec.splitn(3, '/');
    let (scope, name) = (parts.next()?, parts.next()?);
    let sub = match parts.next() {
        None | Some("") => "index.css".to_owned(),
        Some(s) if s.ends_with(".css") => s.to_owned(),
        Some(s) => format!("{s}.css"),
    };
    if sub.contains("..") {
        return None;
    }
    dir.ancestors()
        .take_while(|d| d.starts_with(root))
        .find_map(|d| {
            let p = d
                .join("node_modules")
                .join(scope)
                .join(name)
                .join(&sub)
                .canonicalize()
                .ok()?;
            (p.starts_with(root) && p.is_file()).then_some(p)
        })
}

/// A web font service's stylesheet: `https://fonts.googleapis.com/css2?…`, `https://fonts.bunny.net/css?…`.
pub fn service(url: &str) -> bool {
    SERVICES.iter().any(|s| {
        url.strip_prefix("https://")
            .is_some_and(|u| u.starts_with(s))
    }) && !url.contains(|c: char| c.is_whitespace() || c.is_control())
}

/// The service stylesheets a file links, written out as `https://` URLs.
fn service_urls(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for host in SERVICES {
        for (at, _) in text.match_indices(host) {
            let end = text[at..]
                .find(['"', '\'', ')', '`', '<', '>'])
                .map_or(text.len(), |e| at + e);
            let url = format!("https://{}", text[at..end].trim().replace("&amp;", "&"));
            if service(&url) {
                out.push((at, url));
            }
        }
    }
    out
}

/// The families a service stylesheet asks for: `family=Inter:wght@400;700&family=Playfair+Display`, `family=A|B`.
pub fn families(url: &str) -> Vec<String> {
    url.split(['?', '&'])
        .filter_map(|p| p.strip_prefix("family="))
        .flat_map(|p| p.split('|'))
        .map(|f| {
            f.split(':')
                .next()
                .unwrap_or("")
                .replace('+', " ")
                .replace("%20", " ")
                .trim()
                .to_owned()
        })
        .filter(|f| !f.is_empty())
        .collect()
}

fn add(
    fonts: &mut Vec<Font>,
    family: String,
    face: Option<Face>,
    origin: Origin,
    file: &str,
    line: u32,
) {
    let font = match fonts
        .iter()
        .position(|f| f.family.eq_ignore_ascii_case(&family))
    {
        Some(i) => &mut fonts[i],
        None => {
            fonts.push(Font {
                family,
                origin: if origin == Origin::Service {
                    Origin::Service
                } else {
                    Origin::Missing
                },
                faces: Vec::new(),
                file: file.to_owned(),
                line,
                url: None,
            });
            fonts.last_mut().expect("pushed above")
        }
    };
    if let Some(face) = face
        && !font
            .faces
            .iter()
            .any(|f| f.weight == face.weight && f.style == face.style)
    {
        font.faces.push(face);
        if matches!(font.origin, Origin::Missing | Origin::Service) {
            font.origin = origin;
        }
    }
}

/// The faces of an installed package's stylesheet; `family` names them when a service asked for it by that name.
fn from_package(
    root: &Path,
    fonts: &mut Vec<Font>,
    budget: &mut Budget,
    sheet: &Path,
    (file, line): (&str, u32),
    family: Option<&str>,
) {
    let text = fs::read_to_string(sheet).unwrap_or_default();
    let parsed = css::parse(&text, false, 1, &mut 0);
    let dir = sheet.parent().unwrap_or(root);
    for (name, _, face) in faces(root, dir, &parsed.faces, budget) {
        let name = family.map_or(name, str::to_owned);
        add(fonts, name, face, Origin::Package, file, line);
    }
}

/// Every font the repository declares or loads, with the files found for it.
pub(crate) fn read(repo: &Path, read: &Read) -> Vec<Font> {
    let Ok(root) = repo.canonicalize() else {
        return Vec::new();
    };
    let mut budget = Budget::default();
    let mut fonts: Vec<Font> = Vec::new();
    let dir_of = |rel: &str| {
        root.join(rel)
            .parent()
            .map_or(root.clone(), Path::to_path_buf)
    };
    // Each stylesheet's faces together, so a family split by script keeps its Latin face.
    for chunk in read.faces.chunk_by(|a, b| a.0 == b.0) {
        let rel = &read.files[chunk[0].0].rel;
        let sheet: Vec<css::Face> = chunk.iter().map(|(_, f)| f.clone()).collect();
        for (family, line, face) in faces(&root, &dir_of(rel), &sheet, &mut budget) {
            add(&mut fonts, family, face, Origin::Repo, rel, line);
        }
    }
    let mut seen = HashSet::new();
    for file in &read.files {
        for (at, spec) in fontsource(&file.text) {
            if !seen.insert(spec.clone()) {
                continue;
            }
            let at = (file.rel.as_str(), line_at(&file.text, at));
            match package(&root, &dir_of(&file.rel), &spec) {
                Some(sheet) => from_package(&root, &mut fonts, &mut budget, &sheet, at, None),
                None => {
                    let name = spec.split('/').nth(1).unwrap_or("").replace('-', " ");
                    let mut family = markup::title(&name);
                    if spec.starts_with("@fontsource-variable/") {
                        family.push_str(" Variable");
                    }
                    add(&mut fonts, family, None, Origin::Missing, at.0, at.1);
                }
            }
        }
    }
    for file in &read.files {
        for (at, url) in service_urls(&file.text) {
            let at = (file.rel.as_str(), line_at(&file.text, at));
            for family in families(&url) {
                if fonts
                    .iter()
                    .any(|f| f.family.eq_ignore_ascii_case(&family) && !f.faces.is_empty())
                {
                    continue;
                }
                // The same family installed as a package: read rather than fetched.
                let spec = format!("@fontsource/{}", family.to_ascii_lowercase().replace(' ', "-"));
                if let Some(sheet) = package(&root, &dir_of(&file.rel), &spec) {
                    from_package(&root, &mut fonts, &mut budget, &sheet, at, Some(&family));
                    continue;
                }
                add(&mut fonts, family.clone(), None, Origin::Service, at.0, at.1);
                if let Some(font) = fonts
                    .iter_mut()
                    .find(|f| f.family.eq_ignore_ascii_case(&family))
                {
                    font.origin = Origin::Service;
                    font.url.get_or_insert(url.clone());
                }
            }
        }
    }
    fonts
}

fn curl(url: &str) -> Result<Vec<u8>> {
    let out = Command::new("curl")
        .args(["-q", "-fsSL", "-g", "--proto", "=https", "--proto-redir", "=https"])
        .args(["--max-redirs", "3", "--max-time", "20"])
        .args(["--max-filesize", &MAX_FILE.to_string(), url])
        .output()
        .context("cannot run curl: is it installed?")?;
    ensure!(
        out.status.success(),
        "cannot fetch {url}: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(out.stdout)
}

/// The fonts of a web font service's stylesheet, fetched by the system's curl on the user's word only: the
/// stylesheet and its files must come from the service itself.
pub fn fetch(url: &str) -> Result<Vec<Font>> {
    ensure!(service(url), "not a web font stylesheet: {url}");
    let text = String::from_utf8(curl(url)?).context("the stylesheet is not text")?;
    let sheet = css::parse(&text, false, 1, &mut 0);
    let mut budget = Budget::default();
    let mut fonts = Vec::new();
    for f in pick(&sheet.faces) {
        let Some(src) = f
            .src
            .iter()
            .find(|u| FILE_HOSTS.iter().any(|h| u.starts_with(h)))
        else {
            continue;
        };
        let Some(mime) = mime(src) else { continue };
        let bytes = curl(src)?;
        if !budget.take(bytes.len() as u64) {
            break;
        }
        let face = Face {
            weight: f.weight.clone().unwrap_or_else(|| "400".into()),
            style: f.style.clone().unwrap_or_else(|| "normal".into()),
            src: data_uri(mime, &bytes),
            file: src.clone(),
        };
        add(&mut fonts, f.family.clone(), Some(face), Origin::Service, url, f.line);
    }
    for font in &mut fonts {
        font.url = Some(url.to_owned());
    }
    Ok(fonts)
}
