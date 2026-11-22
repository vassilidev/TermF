//! The brand guidelines of the shared convention: `BRAND.md` (a short summary), `brand/guidelines.html` (the
//! printable guidelines), `brand/logo/*.svg`, `brand/exports/` and `brand/guidelines.pdf`; and the PDF export of
//! the guidelines, printed by headless Chrome.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use base64ct::Encoding;
use serde::Serialize;

const MAX_LOGOS: usize = 24;
const MAX_LOGO: u64 = 256 * 1024;
const MAX_EXPORTS: usize = 200;
const MAX_SUMMARY: u64 = 64 * 1024;
const MAX_GUIDELINES: u64 = 4 * 1024 * 1024;
const MAX_INLINED: u64 = 2 * 1024 * 1024;
const MAX_INLINED_TOTAL: u64 = 8 * 1024 * 1024;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Brand {
    /// `BRAND.md`.
    pub summary: Option<String>,
    pub logos: Vec<Logo>,
    pub exports: Vec<Export>,
    /// `favicon`, `appleTouch`, `og`, and `appIcon` for an app (Tauri, Electron, iOS, Android).
    pub missing: Vec<&'static str>,
    /// `brand/guidelines.html`, its images and fonts from `brand/` inlined so it shows on its own.
    pub guidelines: Option<String>,
    /// `brand/guidelines.pdf` exists.
    pub pdf: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct Logo {
    pub file: String,
    pub svg: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Export {
    pub file: String,
    pub size: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

fn small(path: &Path, max: u64) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    (meta.is_file() && meta.len() <= max)
        .then(|| fs::read_to_string(path).ok())
        .flatten()
}

fn files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if out.len() >= MAX_EXPORTS {
            return;
        }
        let hidden = path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'));
        if path.is_dir() && depth > 0 && !hidden {
            files(&path, depth - 1, out);
        } else if path.is_file() && !hidden {
            out.push(path);
        }
    }
}

/// A PNG's size, from its header.
fn png_size(path: &Path) -> Option<(u32, u32)> {
    use std::io::Read;
    let mut head = [0u8; 24];
    fs::File::open(path).ok()?.read_exact(&mut head).ok()?;
    (head[..8] == *b"\x89PNG\r\n\x1a\n").then(|| {
        (
            u32::from_be_bytes([head[16], head[17], head[18], head[19]]),
            u32::from_be_bytes([head[20], head[21], head[22], head[23]]),
        )
    })
}

pub fn read(repo: &Path) -> Option<Brand> {
    let dir = repo.join("brand");
    let md = repo.join("BRAND.md");
    if !dir.is_dir() && !md.is_file() {
        return None;
    }
    let rel = |p: &Path| {
        p.strip_prefix(repo)
            .unwrap_or(p)
            .to_string_lossy()
            .replace('\\', "/")
    };
    let mut logo_files = Vec::new();
    files(&dir.join("logo"), 0, &mut logo_files);
    let logos = logo_files
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg")))
        .take(MAX_LOGOS)
        .filter_map(|p| {
            Some(Logo {
                file: rel(p),
                svg: small(p, MAX_LOGO)?,
            })
        })
        .collect();
    let mut export_files = Vec::new();
    files(&dir.join("exports"), 2, &mut export_files);
    let exports: Vec<Export> = export_files
        .iter()
        .map(|p| {
            let size = png_size(p);
            Export {
                file: rel(p),
                size: fs::metadata(p).map_or(0, |m| m.len()),
                width: size.map(|s| s.0),
                height: size.map(|s| s.1),
            }
        })
        .collect();
    Some(Brand {
        summary: small(&md, MAX_SUMMARY),
        logos,
        missing: missing(repo, &export_files),
        exports,
        guidelines: small(&dir.join("guidelines.html"), MAX_GUIDELINES)
            .map(|html| inline(&html, &dir)),
        pdf: dir.join("guidelines.pdf").is_file(),
    })
}

/// The usual exports not found in `brand/exports/`, nor where web frameworks keep them (`public/`, `static/`,
/// Next's `app/`).
fn missing(repo: &Path, exports: &[PathBuf]) -> Vec<&'static str> {
    let mut names: Vec<String> = exports
        .iter()
        .filter_map(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_ascii_lowercase())
        })
        .collect();
    for place in ["public", "static", "app", "src/app"] {
        let mut found = Vec::new();
        files(&repo.join(place), 0, &mut found);
        names.extend(found.iter().filter_map(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_ascii_lowercase())
        }));
    }
    let has = |f: &dyn Fn(&str) -> bool| names.iter().any(|n| f(n));
    let mut out = Vec::new();
    if !has(&|n| n.starts_with("favicon") || n == "icon.svg") {
        out.push("favicon");
    }
    if !has(&|n| n.starts_with("apple-touch-icon") || n.starts_with("apple-icon")) {
        out.push("appleTouch");
    }
    if !has(&|n| {
        n.starts_with("og")
            || n.contains("opengraph")
            || n.contains("og-image")
            || n.starts_with("social")
    }) {
        out.push("og");
    }
    let app = ["src-tauri", "app/src-tauri", "ios", "android", "electron"]
        .iter()
        .any(|d| repo.join(d).is_dir())
        || fs::read_to_string(repo.join("package.json")).is_ok_and(|p| p.contains("\"electron\""));
    if app
        && !has(&|n| {
            n.ends_with(".icns")
                || n.ends_with(".ico") && !n.starts_with("favicon")
                || n.starts_with("app-icon")
                || n.starts_with("icon-512")
                || n.starts_with("icon_512")
        })
    {
        out.push("appIcon");
    }
    out
}

fn mime(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => return None,
    })
}

/// The guidelines with the files they reference from `brand/` as data URIs, so they show in a frame that loads
/// nothing; a reference outside `brand/` stays as it is.
fn inline(html: &str, dir: &Path) -> String {
    let Ok(root) = dir.canonicalize() else {
        return html.to_owned();
    };
    let mut out = String::with_capacity(html.len());
    let mut total = 0u64;
    let mut rest = html;
    loop {
        let next = ["src=", "href=", "url("]
            .iter()
            .filter_map(|m| rest.find(m).map(|i| (i, m.len())))
            .min();
        let Some((at, len)) = next else {
            out.push_str(rest);
            return out;
        };
        let start = at + len;
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'');
        let skip = quote.map_or(0, |_| 1);
        let end = match quote {
            Some(q) => rest[1..].find(q).map(|e| e + 1),
            None => rest.find([')', ' ', '>']),
        };
        let Some(end) = end else { continue };
        let target = rest[skip..end].trim();
        let data = (!target.contains(':') && !target.starts_with(['/', '#']) && !target.is_empty())
            .then(|| root.join(target).canonicalize().ok())
            .flatten()
            .filter(|p| p.starts_with(&root))
            .and_then(|p| {
                let mime = mime(target)?;
                let size = fs::metadata(&p).ok()?.len();
                (size <= MAX_INLINED && total + size <= MAX_INLINED_TOTAL).then_some(())?;
                total += size;
                Some(format!(
                    "data:{mime};base64,{}",
                    base64ct::Base64::encode_string(&fs::read(&p).ok()?)
                ))
            });
        match data {
            Some(d) => {
                out.push_str(&rest[..skip]);
                out.push_str(&d);
                rest = &rest[end..];
            }
            None => {
                out.push_str(&rest[..end]);
                rest = &rest[end..];
            }
        }
    }
}

/// A `file://` URL Chrome can open.
fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let text = text.strip_prefix("//?/").unwrap_or(&text);
    let mut out = String::from(if text.starts_with('/') {
        "file://"
    } else {
        "file:///"
    });
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~:".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `brand/guidelines.html` printed to PDF by headless Chrome, its `@page` size kept; nothing on the page runs and
/// nothing loads from the network.
pub fn pdf(repo: &Path) -> Result<Vec<u8>> {
    let page = repo.join("brand").join("guidelines.html");
    ensure!(
        page.is_file(),
        "this repository has no brand/guidelines.html"
    );
    let page = page
        .canonicalize()
        .context("cannot open brand/guidelines.html")?;
    let program = termf_visual::browser::find()?;
    let mut browser = termf_visual::browser::Browser::launch(&program)?;
    browser.print(&file_url(&page))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_brand_folder_and_inlines_its_images() {
        let repo = std::env::temp_dir().join(format!("termf-design-brand-{}", std::process::id()));
        let _ = fs::remove_dir_all(&repo);
        fs::create_dir_all(repo.join("brand/logo")).unwrap();
        fs::create_dir_all(repo.join("brand/exports/icons")).unwrap();
        fs::create_dir_all(repo.join("src-tauri")).unwrap();
        fs::write(repo.join("BRAND.md"), "# Acme\nSee brand/guidelines.html").unwrap();
        fs::write(repo.join("brand/logo/mark.svg"), "<svg viewBox='0 0 1 1'/>").unwrap();
        fs::write(repo.join("brand/logo/notes.txt"), "x").unwrap();
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(1200u32.to_be_bytes());
        png.extend(630u32.to_be_bytes());
        fs::write(repo.join("brand/exports/og.png"), &png).unwrap();
        fs::write(repo.join("brand/exports/icons/favicon.ico"), "x").unwrap();
        fs::write(repo.join("secret.svg"), "<svg/>").unwrap();
        fs::write(
            repo.join("brand/guidelines.html"),
            "<img src=\"logo/mark.svg\"><img src='../secret.svg'><a href=\"https://acme.test\">x</a><style>.a{background:url(logo/mark.svg)}</style>",
        )
        .unwrap();
        let b = read(&repo).unwrap();
        assert_eq!(
            b.summary.as_deref(),
            Some("# Acme\nSee brand/guidelines.html")
        );
        assert_eq!(b.logos.len(), 1);
        assert_eq!(b.logos[0].file, "brand/logo/mark.svg");
        assert_eq!(
            b.exports[1],
            Export {
                file: "brand/exports/og.png".into(),
                size: png.len() as u64,
                width: Some(1200),
                height: Some(630)
            }
        );
        assert_eq!(b.missing, ["appleTouch", "appIcon"]);
        let html = b.guidelines.unwrap();
        let data = "data:image/svg+xml;base64,";
        assert_eq!(html.matches(data).count(), 2, "{html}");
        assert!(
            html.contains("src='../secret.svg'"),
            "outside brand/ stays a reference: {html}"
        );
        assert!(html.contains("href=\"https://acme.test\""));
        assert!(!b.pdf);
        assert!(read(&repo.join("src-tauri")).is_none());
        assert_eq!(file_url(Path::new("/a b/c.html")), "file:///a%20b/c.html");
        fs::remove_dir_all(&repo).unwrap();
    }

    /// Prints a page with Chrome of this machine: `cargo test -p termf-design -- --ignored real_chrome`.
    #[test]
    #[ignore]
    fn real_chrome_prints_the_guidelines_to_pdf() {
        let repo = std::env::temp_dir().join(format!("termf-design-pdf-{}", std::process::id()));
        let _ = fs::remove_dir_all(&repo);
        fs::create_dir_all(repo.join("brand/logo")).unwrap();
        fs::write(repo.join("brand/logo/mark.svg"), "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10'><rect width='10' height='10' fill='#4f46e5'/></svg>").unwrap();
        fs::write(
            repo.join("brand/guidelines.html"),
            // Were the script to run, the page would turn portrait.
            "<!doctype html><style>@page{size:A4 landscape;margin:0}body{background:#4f46e5;color:#fff}</style><h1>Acme</h1><img src='logo/mark.svg' width=40><script>document.head.insertAdjacentHTML('beforeend','<style>@page{size:A4 portrait}</style>')</script><img src='https://example.com/x.png'>",
        )
        .unwrap();
        let pdf = pdf(&repo).unwrap();
        assert!(pdf.starts_with(b"%PDF-"), "not a PDF");
        let text = String::from_utf8_lossy(&pdf);
        let at = text.find("/MediaBox [").expect("no page") + "/MediaBox [".len();
        let size: Vec<f64> = text[at..]
            .split(']')
            .next()
            .unwrap()
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .collect();
        // Landscape A4 is 842 × 595 points.
        assert!(
            (size[2] - 842.0).abs() < 1.0 && (size[3] - 595.0).abs() < 1.0,
            "{size:?}"
        );
        fs::remove_dir_all(&repo).unwrap();
    }
}
