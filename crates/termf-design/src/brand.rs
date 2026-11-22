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
/// The guidelines shown in the view, and printed.
const MAX_PREVIEW: u64 = 4 * 1024 * 1024;
const MAX_PRINTED: u64 = 32 * 1024 * 1024;
/// What a file of `brand/` may weigh to be inlined, and all of them together: for the view, for the PDF.
const PREVIEW: (u64, u64) = (2 * 1024 * 1024, 8 * 1024 * 1024);
const PRINT: (u64, u64) = (16 * 1024 * 1024, 64 * 1024 * 1024);

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Brand {
    /// `BRAND.md`.
    pub summary: Option<String>,
    pub logos: Vec<Logo>,
    pub exports: Vec<Export>,
    /// `favicon`, `appleTouch`, `og`, and `appIcon` for an app (Tauri, Electron, iOS, Android).
    pub missing: Vec<&'static str>,
    /// `brand/guidelines.html` exists: it can be exported to PDF.
    pub printable: bool,
    /// Its preview, up to 4 MB: its images and fonts from `brand/` inlined so it shows on its own.
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
        printable: dir.join("guidelines.html").is_file(),
        guidelines: small(&dir.join("guidelines.html"), MAX_PREVIEW)
            .map(|html| inline(&html, &dir, PREVIEW)),
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
/// nothing; a reference outside `brand/` stays as it is, and leads nowhere there. `(per file, in all)`: sizes.
fn inline(html: &str, dir: &Path, (max_file, max_total): (u64, u64)) -> String {
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
                (size <= max_file && total + size <= max_total).then_some(())?;
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

/// `brand/guidelines.html` printed to PDF by headless Chrome, its `@page` size kept. The page is never opened
/// from disk (a trapped one could reach any file of the machine): its `brand/` files are inlined, and Chrome
/// prints it from a blank page that runs nothing and loads nothing.
pub fn pdf(repo: &Path) -> Result<Vec<u8>> {
    let dir = repo.join("brand");
    let page = dir.join("guidelines.html");
    ensure!(
        page.is_file(),
        "this repository has no brand/guidelines.html"
    );
    let html = small(&page, MAX_PRINTED)
        .context("brand/guidelines.html is over 32 MB or is not UTF-8 text")?;
    let program = termf_visual::browser::find()?;
    let mut browser = termf_visual::browser::Browser::launch(&program)?;
    browser.print_html(&inline(&html, &dir, PRINT))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(b.printable);
        assert!(read(&repo.join("src-tauri")).is_none());
        assert_eq!(file_url(Path::new("/a b/c.html")), "file:///a%20b/c.html");
        // Too big to preview, still there to print.
        let big = format!("<p>{}</p>", "x".repeat(MAX_PREVIEW as usize));
        fs::write(repo.join("brand/guidelines.html"), big).unwrap();
        let b = read(&repo).unwrap();
        assert!(b.printable && b.guidelines.is_none());
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

    /// A PNG of one color, of a size nothing else in a PDF has (37 × 29), stored without compression.
    fn png(rgb: [u8; 3]) -> Vec<u8> {
        let (w, h) = (37u32, 29u32);
        // Each row: no filter, then its pixels.
        let row: Vec<u8> = std::iter::once(0).chain(rgb.repeat(w as usize)).collect();
        let raw = row.repeat(h as usize);
        let crc = |data: &[u8]| {
            let mut c = 0xffff_ffffu32;
            for &b in data {
                c ^= b as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 {
                        0xedb8_8320 ^ (c >> 1)
                    } else {
                        c >> 1
                    };
                }
            }
            !c
        };
        let (mut a, mut b) = (1u32, 0u32);
        for &x in &raw {
            a = (a + x as u32) % 65521;
            b = (b + a) % 65521;
        }
        let len = raw.len() as u16;
        let mut z = vec![0x78, 0x01, 0x01];
        z.extend(len.to_le_bytes());
        z.extend((!len).to_le_bytes());
        z.extend(&raw);
        z.extend(((b << 16) | a).to_be_bytes());
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend(w.to_be_bytes());
        ihdr.extend(h.to_be_bytes());
        ihdr.extend([8, 2, 0, 0, 0]);
        for (kind, data) in [(&b"IHDR"[..], ihdr), (b"IDAT", z), (b"IEND", Vec::new())] {
            out.extend((data.len() as u32).to_be_bytes());
            let mut body = kind.to_vec();
            body.extend(data);
            out.extend(&body);
            out.extend(crc(&body).to_be_bytes());
        }
        out
    }

    /// A trapped `guidelines.html` reaches for files outside `brand/` every way HTML and CSS can: none of them gets
    /// into the PDF, while an image of `brand/` does.
    #[test]
    #[ignore]
    fn real_chrome_prints_nothing_from_outside_brand() {
        let repo = std::env::temp_dir().join(format!("termf-design-trap-{}", std::process::id()));
        let _ = fs::remove_dir_all(&repo);
        fs::create_dir_all(repo.join("brand/logo")).unwrap();
        fs::write(repo.join("secret.png"), png([0, 255, 0])).unwrap();
        fs::write(repo.join("secret.html"), "<img src='secret.png' width=200>").unwrap();
        fs::write(repo.join("secret.css"), "body{background:url(secret.png)}").unwrap();
        let abs = repo
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let file = |name: &str| file_url(Path::new(&format!("{abs}/{name}")));
        let (img, html) = (file("secret.png"), file("secret.html"));
        let ways = [
            "<style>@import url(../secret.css);</style>".to_owned(),
            "<link rel='stylesheet' href='../secret.css'>".to_owned(),
            "<img src='../secret.png' width=100>".to_owned(),
            format!("<img src='{img}' width=100>"),
            "<img srcset='../secret.png 1x' width=100>".to_owned(),
            "<iframe src='../secret.html'></iframe>".to_owned(),
            format!("<iframe src='{html}'></iframe>"),
            "<object data='../secret.png'></object>".to_owned(),
            format!("<embed src='{img}'>"),
            "<svg width=100 height=100><image href='../secret.png' width=100 height=100/></svg>"
                .to_owned(),
            format!("<div style=\"width:100px;height:100px;background:url('{img}')\"></div>"),
        ];
        // A broken image is drawn as an icon, an image too: only one of this size is the file.
        let images = |pdf: &[u8]| String::from_utf8_lossy(pdf).matches("/Width 37").count();
        let mut leaked = Vec::new();
        for way in &ways {
            let page = format!("<!doctype html><base href='{}'>{way}", file("brand/"));
            fs::write(repo.join("brand/guidelines.html"), page).unwrap();
            if images(&pdf(&repo).unwrap()) > 0 {
                leaked.push(way);
            }
        }
        assert!(
            leaked.is_empty(),
            "files outside brand/ in the PDF through {leaked:?}"
        );
        fs::write(repo.join("brand/logo/mark.png"), png([255, 0, 0])).unwrap();
        fs::write(
            repo.join("brand/guidelines.html"),
            "<img src='logo/mark.png' width=100>",
        )
        .unwrap();
        assert!(
            images(&pdf(&repo).unwrap()) > 0,
            "the images of brand/ are printed"
        );
        fs::remove_dir_all(&repo).unwrap();
    }
}
