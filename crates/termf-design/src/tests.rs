use std::path::PathBuf;

use super::*;

/// A throwaway repository with these files.
fn repo(name: &str, files: &[(&str, &str)]) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("termf-design-{name}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    for (file, text) in files {
        let path = dir.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    dir
}

fn token<'a>(d: &'a Design, name: &str) -> &'a Token {
    d.tokens.iter().find(|t| t.name == name).unwrap_or_else(|| {
        panic!(
            "no {name} in {:?}",
            d.tokens.iter().map(|t| &t.name).collect::<Vec<_>>()
        )
    })
}

fn of(d: &Design, kind: Kind) -> Vec<&Finding> {
    d.findings.iter().filter(|f| f.kind == kind).collect()
}

fn find<'a>(d: &'a Design, kind: Kind, subject: &str) -> &'a Finding {
    d.findings
        .iter()
        .find(|f| f.kind == kind && f.subject == subject)
        .unwrap_or_else(|| panic!("no {kind:?} {subject} in {:#?}", of(d, kind)))
}

fn has(d: &Design, kind: Kind, subject: &str) -> bool {
    d.findings
        .iter()
        .any(|f| f.kind == kind && f.subject == subject)
}

const ACME_CSS: &str = r#"@import url("https://fonts.googleapis.com/css2?family=Inter:wght@400;600&family=Playfair+Display&display=swap");
:root {
  --bg: #ffffff;
  --surface: #f8fafc;
  --text: #0f172a;
  --muted: #94a3b8;
  --border: #e2e8f0;
  --gray-100: #f3f4f6;
  --gray-200: #e5e7eb;
  --gray-700: #374151;
  --slate-700: #383f50;
  --accent: #4f46e5;
  --accent-foreground: #ffffff;
  --link: var(--accent);
  --brand: #4f46e5;
  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-5: 24px;
  --space-6: 18px;
  --radius-sm: 4px;
  --radius: 8px;
  --text-sm: 14px;
  --text-base: 16px;
  --text-lg: 20px;
  --font-sans: "Inter", system-ui, sans-serif;
  --shadow-1: 0 1px 2px rgb(0 0 0 / 0.06);
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #0b1120;
    --surface: #111827;
    --text: #e2e8f0;
    --muted: #64748b;
    --accent: #818cf8;
    --accent-foreground: #0b1120;
  }
}
body { background: var(--bg); color: var(--text); font-family: var(--font-sans); }
h1 { font-family: "Playfair Display", serif; }
a { color: var(--link); }
"#;

const BUTTON: &str = r#"<script lang="ts">
  let { variant = "primary", size = "md" }: { variant?: "primary" | "ghost" | "danger"; size?: "sm" | "md" } = $props();
</script>
<button class="btn {variant} {size}"><slot /></button>
<style>
  .btn { padding: 8px 13px; border-radius: var(--radius); background: var(--accent); color: var(--accent-foreground); }
  .ghost { background: transparent; color: #4f46e6; }
  .danger { background: #e11d48; color: #fff; }
</style>
"#;

const CARD: &str = r#"<div class="card"><h2><slot name="title" /></h2><p class="meta"></p><div class="body"><slot /></div><footer><a href="/">x</a></footer></div>
<style>
  .card { padding: 16px; background: var(--surface); border: 1px solid var(--border); border-radius: 6px; box-shadow: var(--shadow-1); }
  .meta { color: var(--muted); font-size: 13px; }
</style>
"#;

const PANEL: &str = r#"<div class="panel"><h2><slot name="title" /></h2><p></p><div><slot /></div><footer><a href="/">y</a></footer></div>"#;

const PAGE: &str = r#"<script>
  import Button from "$lib/components/Button.svelte";
</script>
<Button variant="primary">Go</Button>
<Card /><Panel />
<p style="color: #0f172a">Hi</p>
<a style="color: var(--accent-hover)">x</a>
"#;

/// A SvelteKit app with an imperfect design system, the files a scan must leave alone around it.
fn acme() -> PathBuf {
    repo(
        "acme",
        &[
            ("package.json", r#"{"name": "acme-web", "private": true}"#),
            (".gitignore", "src/ignored.css\n"),
            ("src/app.css", ACME_CSS),
            ("src/lib/components/Button.svelte", BUTTON),
            ("src/lib/components/Card.svelte", CARD),
            ("src/lib/components/Panel.svelte", PANEL),
            ("src/lib/legacy/Button.svelte", "<button><slot /></button>"),
            ("src/routes/+page.svelte", PAGE),
            // Never the project's design: ignored, installed, built, tested or generated.
            ("src/ignored.css", ".x { color: #abcdef; padding: 7px; }"),
            (
                "node_modules/lib/style.css",
                ".y { color: #abcdef; } :root { --lib: 1px }",
            ),
            ("dist/app.css", ".z { color: #abcdef; }"),
            (
                "src/lib/components/Button.test.ts",
                "const s = 'color: #abcdef';",
            ),
            ("tests/fixtures/x.css", ".t { color: #abcdef }"),
            (
                "src/lib/theme.gen.css",
                ".g { color: #abcdef; padding: 7px; }",
            ),
        ],
    )
}

#[test]
fn reads_css_tokens_with_their_themes_uses_and_sources() {
    let d = scan(&acme());
    let bg = token(&d, "--bg");
    assert_eq!((bg.family, bg.format), (Family::Color, Format::Css));
    assert_eq!((bg.file.as_str(), bg.line), ("src/app.css", 3));
    assert_eq!(bg.color.as_deref(), Some("#ffffff"));
    assert_eq!(bg.dark_color.as_deref(), Some("#0b1120"));
    assert_eq!(bg.dark.as_ref().unwrap().line, 32);
    assert!(d.dark);
    let link = token(&d, "--link");
    assert!(link.alias);
    assert_eq!(
        link.dark_color.as_deref(),
        Some("#818cf8"),
        "an alias follows its token's dark value"
    );
    assert_eq!(token(&d, "--space-4").px, Some(16.0));
    assert_eq!(token(&d, "--space-4").family, Family::Spacing);
    assert_eq!(token(&d, "--text-sm").family, Family::Text);
    assert_eq!(token(&d, "--radius").family, Family::Radius);
    assert_eq!(token(&d, "--font-sans").family, Family::Font);
    assert_eq!(token(&d, "--shadow-1").family, Family::Shadow);
    assert_eq!(token(&d, "--accent").uses, 2, "var() in Button, and --link");
    assert_eq!(token(&d, "--text").uses, 1);
    assert!(
        d.tokens.iter().all(|t| t.name != "--lib"),
        "node_modules is not read"
    );
    assert_eq!(d.sources, ["src/app.css"]);
    // The generated file is read for what it defines, never checked.
    assert_eq!(d.files, 7);
}

#[test]
fn finds_hardcoded_and_off_scale_values_with_the_token_to_use() {
    let d = scan(&acme());
    // Within ΔE 0.5 of a token: that token, and where.
    let near = find(&d, Kind::Hardcoded, "#4f46e6");
    assert_eq!(
        (near.level, near.other.as_slice()),
        (Level::Warn, ["--accent".to_owned()].as_slice())
    );
    assert_eq!(near.at[0].file, "src/lib/components/Button.svelte");
    assert_eq!(near.at[0].line, 7);
    assert_eq!(
        near.at[0].text,
        ".ghost { background: transparent; color: #4f46e6; }"
    );
    // White as a text color is the text-on-accent token, not the background one of the same value.
    assert_eq!(
        find(&d, Kind::Hardcoded, "#ffffff").other,
        ["--accent-foreground"]
    );
    assert_eq!(
        find(&d, Kind::Hardcoded, "#0f172a").at[0].file,
        "src/routes/+page.svelte"
    );
    // Far from every token, in a real palette: noted, nothing to suggest.
    let lone = find(&d, Kind::Hardcoded, "#e11d48");
    assert_eq!((lone.level, lone.other.len()), (Level::Info, 0));
    assert_eq!(find(&d, Kind::Hardcoded, "16px").other, ["--space-4"]);
    assert_eq!(find(&d, Kind::Hardcoded, "8px").other, ["--space-2"]);
    let off = d
        .findings
        .iter()
        .find(|f| {
            f.kind == Kind::OffScale && f.subject == "13px" && f.family == Some(Family::Spacing)
        })
        .expect("a padding off the 4 px grid");
    assert_eq!(off.at.len(), 1);
    assert_eq!(find(&d, Kind::OffScale, "6px").family, Some(Family::Radius));
    let text = d
        .findings
        .iter()
        .find(|f| f.kind == Kind::OffScale && f.subject == "13px" && f.family == Some(Family::Text))
        .expect("a font size out of the type scale");
    assert_eq!(text.other, ["14px", "16px", "20px"]);
    // A token off the 4 px grid the others are on.
    assert_eq!(find(&d, Kind::OffScale, "--space-6").other, ["18px", "4px"]);
    // Nothing from ignored, installed, built, test or generated files.
    assert!(!has(&d, Kind::Hardcoded, "#abcdef"));
    assert!(!has(&d, Kind::OffScale, "7px") && !has(&d, Kind::Hardcoded, "7px"));
}

#[test]
fn finds_near_and_duplicate_tokens_but_not_scale_steps_or_aliases() {
    let d = scan(&acme());
    let near = find(&d, Kind::Near, "--gray-700");
    assert_eq!(near.other, ["--slate-700"]);
    assert_eq!(near.level, Level::Warn);
    assert!(near.measure.unwrap() < 1.0);
    assert_eq!(near.at.len(), 2);
    assert!(
        !of(&d, Kind::Near).iter().any(|f| f.subject == "--gray-100"),
        "steps of one scale are close on purpose"
    );
    let dup = find(&d, Kind::Duplicate, "#4f46e5");
    assert_eq!(
        dup.other,
        ["--accent", "--brand"],
        "the alias --link is not a duplicate"
    );
}

#[test]
fn finds_unused_undefined_and_incomplete_dark_tokens() {
    let d = scan(&acme());
    let unused = find(&d, Kind::Unused, "color");
    for name in ["--gray-100", "--slate-700", "--brand"] {
        assert!(unused.other.contains(&name.to_owned()), "{name}");
    }
    assert!(!unused.other.contains(&"--accent".to_owned()));
    let undefined = find(&d, Kind::Undefined, "--accent-hover");
    assert_eq!(undefined.at[0].file, "src/routes/+page.svelte");
    assert_eq!(undefined.at[0].line, 7);
    assert_eq!(of(&d, Kind::Undefined).len(), 1);
    // Used, semantic, light only.
    let border = find(&d, Kind::Dark, "--border");
    assert_eq!(border.level, Level::Warn);
    assert!(!has(&d, Kind::Dark, "--link"), "an alias follows its token");
    assert!(!has(&d, Kind::Dark, "--brand"), "unused");
}

#[test]
fn measures_contrast_of_text_on_backgrounds_in_both_themes() {
    let d = scan(&acme());
    let light = d
        .findings
        .iter()
        .find(|f| {
            f.kind == Kind::Contrast
                && f.subject == "--muted"
                && f.other == ["--bg"]
                && f.theme == Some(Theme::Light)
        })
        .expect("muted text on the background");
    assert_eq!(light.level, Level::Error);
    assert!(
        (light.measure.unwrap() - 2.56).abs() < 0.05,
        "{:?}",
        light.measure
    );
    let dark = d
        .findings
        .iter()
        .find(|f| {
            f.kind == Kind::Contrast
                && f.subject == "--muted"
                && f.other == ["--bg"]
                && f.theme == Some(Theme::Dark)
        })
        .expect("and in the dark theme");
    assert_eq!(dark.level, Level::Warn);
    let pair = |fg: &str, bg: &str| {
        d.pairs
            .iter()
            .find(|p| p.fg == fg && p.bg == bg && p.theme == Theme::Light)
    };
    let button = pair("--accent-foreground", "--accent").expect("X-foreground on X");
    assert!(button.ratio > 4.5);
    assert!(
        button
            .at
            .as_ref()
            .is_some_and(|a| a.file.ends_with("Button.svelte"))
            || pair("--accent-foreground", "--accent").is_some()
    );
    assert!(pair("--text", "--bg").unwrap().ratio > 15.0);
    assert!(!has(&d, Kind::Contrast, "--text"));
}

#[test]
fn finds_loaded_fonts_missing_from_the_tokens_and_duplicate_components() {
    let d = scan(&acme());
    let font = find(&d, Kind::Font, "Playfair Display");
    assert_eq!(font.at[0].line, 1);
    assert!(!has(&d, Kind::Font, "Inter"));
    let button = d
        .components
        .iter()
        .find(|c| c.file == "src/lib/components/Button.svelte")
        .unwrap();
    assert_eq!(button.framework, "svelte");
    assert_eq!(button.uses, 1);
    assert_eq!(button.used_in, ["src/routes/+page.svelte"]);
    assert_eq!(button.variants[0].values, ["primary", "ghost", "danger"]);
    assert!(d.components.iter().all(|c| !c.file.contains("routes")));
    let same_name = find(&d, Kind::Component, "Button");
    assert_eq!(same_name.level, Level::Warn);
    assert_eq!(same_name.other, ["src/lib/legacy/Button.svelte"]);
    let same_shape = find(&d, Kind::Component, "Card");
    assert_eq!(
        (same_shape.level, same_shape.other.as_slice()),
        (Level::Info, ["Panel".to_owned()].as_slice())
    );
}

#[test]
fn reads_tailwind_4_theme_and_its_utilities() {
    let d = scan(&repo(
        "tw4",
        &[
            ("package.json", r#"{"private": true}"#),
            (
                "src/app.css",
                r#"@import "tailwindcss";
@theme {
  --color-brand: #4f46e5;
  --color-unused: #000000;
  --font-display: "Satoshi", sans-serif;
  --spacing-gutter: 18px;
  --text-lg--line-height: 1.5;
}
@theme inline {
  --color-secondary: var(--secondary);
  --color-secondary-foreground: var(--secondary-foreground);
}
:root {
  --background: #ffffff;
  --foreground: #0a0a0a;
  --secondary: #f5f5f5;
  --secondary-foreground: #a3a3a3;
}
.dark { --background: #0a0a0a; --foreground: #fafafa; --secondary: #262626; --secondary-foreground: #fafafa; }
"#,
            ),
            (
                "src/App.tsx",
                r#"export function App() {
  return <div className="bg-brand font-display mt-gutter bg-secondary text-secondary-foreground">
    <p className="text-[#333333] p-[13px]" style={{ color: "var(--color-red-500)" }}>x</p>
  </div>;
}"#,
            ),
        ],
    ));
    assert_eq!(token(&d, "--color-brand").format, Format::Tailwind);
    assert!(token(&d, "--color-brand").uses > 0, "bg-brand");
    assert!(token(&d, "--font-display").uses > 0, "font-display");
    assert_eq!(token(&d, "--font-display").family, Family::Font);
    assert!(token(&d, "--spacing-gutter").uses > 0, "mt-gutter");
    assert_eq!(
        token(&d, "--color-secondary").dark_color.as_deref(),
        Some("#262626")
    );
    assert!(d.tokens.iter().all(|t| t.name != "--text-lg--line-height"));
    let unused = &find(&d, Kind::Unused, "color").other;
    assert!(unused.contains(&"--color-unused".to_owned()));
    for used in ["--color-brand", "--color-secondary", "--secondary"] {
        assert!(!unused.contains(&used.to_owned()), "{used}");
    }
    let arbitrary = find(&d, Kind::Hardcoded, "#333333");
    assert_eq!(
        (arbitrary.at[0].file.as_str(), arbitrary.at[0].line),
        ("src/App.tsx", 3)
    );
    assert!(
        !has(&d, Kind::OffScale, "13px"),
        "one spacing token is not a scale"
    );
    assert!(
        of(&d, Kind::Undefined).is_empty(),
        "Tailwind defines its default palette"
    );
    let fail = d
        .findings
        .iter()
        .find(|f| {
            f.kind == Kind::Contrast
                && f.subject == "--secondary-foreground"
                && f.other == ["--secondary"]
        })
        .expect("X-foreground on X");
    assert_eq!(fail.theme, Some(Theme::Light));
}

#[test]
fn reads_a_tailwind_3_config_without_running_it() {
    let d = scan(&repo(
        "tw3",
        &[
            (
                "tailwind.config.js",
                r##"const colors = require("tailwindcss/colors");
module.exports = {
  content: ["./src/**/*.jsx"],
  theme: {
    extend: {
      colors: { brand: { DEFAULT: "#4f46e5", 600: "#4338ca" }, gray: colors.slate },
      spacing: { 18: "4.5rem" },
    },
  },
};"##,
            ),
            (
                "src/Header.jsx",
                r#"export function Header() { return <h1 className="text-brand hover:bg-brand-600">x</h1>; }"#,
            ),
        ],
    ));
    let brand = token(&d, "colors.brand");
    assert_eq!(
        (brand.format, brand.line, brand.uses),
        (Format::Config, 6, 1)
    );
    assert_eq!(token(&d, "colors.brand.600").uses, 1);
    assert_eq!(token(&d, "spacing.18").px, Some(72.0));
    assert_eq!(find(&d, Kind::Unused, "spacing").other, ["spacing.18"]);
    assert_eq!(
        d.notes,
        [Note {
            kind: NoteKind::Unread,
            file: Some("tailwind.config.js".into()),
            detail: Some("theme.extend.colors.gray".into())
        }]
    );
}

#[test]
fn reads_scss_variables_and_their_uses() {
    let d = scan(&repo(
        "scss",
        &[
            (
                "src/styles/_tokens.scss",
                "$primary: #3b82f6;\n$primary-alt: #3b82f7;\n$space: 8px;\n$unused-gap: 4px;\n",
            ),
            (
                "src/styles/main.scss",
                "@use \"tokens\";\n.a { color: $primary; padding: $space; border: 1px solid #3b82f6; }\n.b { color: $primary-alt; }\n",
            ),
        ],
    ));
    assert_eq!(token(&d, "$primary").format, Format::Scss);
    assert_eq!(token(&d, "$primary").uses, 1);
    assert_eq!(find(&d, Kind::Unused, "spacing").other, ["$unused-gap"]);
    assert_eq!(find(&d, Kind::Near, "$primary").other, ["$primary-alt"]);
    assert_eq!(find(&d, Kind::Hardcoded, "#3b82f6").other, ["$primary"]);
    assert!(
        d.notes.is_empty(),
        "a bare @use is a partial, not a package: {:?}",
        d.notes
    );
}

#[test]
fn reads_w3c_design_tokens_merged_with_their_generated_css() {
    let d = scan(&repo(
        "dtcg",
        &[
            (
                "design/tokens.json",
                r##"{
  "color": { "$type": "color", "brand": { "500": { "$value": "#4f46e5" } }, "text": { "$value": "{color.brand.500}" } },
  "space": { "$type": "dimension", "md": { "$value": { "value": 16, "unit": "px" } } }
}"##,
            ),
            (
                "design/dark.tokens.json",
                r##"{ "color": { "$type": "color", "text": { "$value": "#e0e7ff" } } }"##,
            ),
            (
                "src/tokens.css",
                "/* Generated by Style Dictionary. Do not edit. */\n:root { --color-brand-500: #4f46e5; --color-text: var(--color-brand-500); --space-md: 16px; }\n.x { color: #123123; }\n",
            ),
            (
                "src/a.css",
                ".y { color: var(--color-text); padding: 16px; }\n",
            ),
        ],
    ));
    let text = token(&d, "--color-text");
    assert_eq!(text.path.as_deref(), Some("color.text"));
    assert_eq!(
        (text.format, text.file.as_str(), text.line),
        (Format::Dtcg, "design/tokens.json", 2)
    );
    assert_eq!(text.color.as_deref(), Some("#4f46e5"));
    assert_eq!(text.dark_color.as_deref(), Some("#e0e7ff"));
    assert_eq!(token(&d, "--space-md").px, Some(16.0));
    assert!(
        token(&d, "--color-brand-500").uses >= 2,
        "an alias and a var()"
    );
    assert_eq!(find(&d, Kind::Hardcoded, "16px").other, ["--space-md"]);
    assert!(
        !has(&d, Kind::Hardcoded, "#123123"),
        "generated files are not checked"
    );
}

/// What the `brand` role writes (`brand-identity` skill): `design/tokens.json`, `design/tokens.dark.json` and the
/// `design/tokens.css` its `tokens-to-css.mjs` makes of them.
#[test]
fn reads_the_tokens_the_brand_role_writes() {
    let d = scan(&repo(
        "brand-role",
        &[
            (
                "design/tokens.json",
                r##"{
  "color": {
    "$type": "color",
    "accent": { "$root": { "$value": { "colorSpace": "oklch", "components": [0.55, 0.2, 275], "hex": "#5145e5" } }, "soft": { "$value": { "colorSpace": "srgb", "components": [0.32, 0.27, 0.9], "alpha": 0.1, "hex": "#5145e5" } } }
  },
  "font": { "sizeBody": { "$type": "dimension", "$value": { "value": 1, "unit": "rem" } } }
}"##,
            ),
            (
                "design/tokens.dark.json",
                r##"{ "color": { "$type": "color", "accent": { "$root": { "$value": { "colorSpace": "srgb", "components": [0.5, 0.55, 1], "hex": "#808cff" } } } } }"##,
            ),
            (
                "design/tokens.css",
                "/* Generated from design/tokens.json by tokens-to-css.mjs: edit the tokens, then run it again. */\n:root {\n  --color-accent: #5145e5;\n  --color-accent-soft: rgb(82 69 230 / 0.1);\n  --font-size-body: 1rem;\n}\n@media (prefers-color-scheme: dark) {\n  :root {\n    --color-accent: #808cff;\n  }\n}\n.x { color: #123123; }\n",
            ),
            (
                "src/a.css",
                ".b { color: var(--color-accent); background: var(--color-accent-soft); font-size: var(--font-size-body); }\n",
            ),
        ],
    ));
    let accent = token(&d, "--color-accent");
    assert_eq!(
        (accent.path.as_deref(), accent.format, accent.file.as_str()),
        (Some("color.accent"), Format::Dtcg, "design/tokens.json")
    );
    assert_eq!(
        accent.dark.as_ref().unwrap().file,
        "design/tokens.dark.json"
    );
    assert_eq!(accent.dark_color.as_deref(), Some("#808cff"));
    assert_eq!(
        token(&d, "--color-accent-soft").color.as_deref(),
        Some("#5245e61a")
    );
    assert_eq!(token(&d, "--font-size-body").px, Some(16.0));
    assert!(
        of(&d, Kind::Unused).is_empty(),
        "{:?}",
        of(&d, Kind::Unused)
    );
    assert!(
        !has(&d, Kind::Hardcoded, "#123123"),
        "the generated CSS is not checked"
    );
}

#[test]
fn reads_components_of_react_and_vue_with_their_stories() {
    let d = scan(&repo(
        "components",
        &[
            (".storybook/main.js", "export default {};"),
            (
                "src/components/Button.tsx",
                r#"export function Button({ variant }: { variant?: "primary" | "secondary" }) { return <button className={variant} />; }"#,
            ),
            (
                "src/components/Button.stories.tsx",
                "export default { title: \"UI/Button\" };\nexport const Primary = {};\nexport const WithIcon = {};\n",
            ),
            (
                "src/components/ui/button.tsx",
                "export const Button = () => <button />;",
            ),
            (
                "src/pages/index.tsx",
                "export default function Home() { return <><Button variant=\"primary\" /><Button /><Card /></>; }",
            ),
            (
                "src/components/Card.vue",
                "<template><div><h2 /><p /><span /><a /><i /></div></template>",
            ),
            (
                "src/views/Home.vue",
                "<template><Card /><my-card /></template>",
            ),
        ],
    ));
    let button = d
        .components
        .iter()
        .find(|c| c.file == "src/components/Button.tsx")
        .unwrap();
    assert_eq!((button.framework, button.uses), ("jsx", 2));
    assert_eq!(
        button.stories,
        ["ui-button--primary", "ui-button--with-icon"]
    );
    assert_eq!(
        button.variants,
        [Variant {
            prop: "variant".into(),
            values: vec!["primary".into(), "secondary".into()]
        }]
    );
    assert!(d.storybook);
    let card = d.components.iter().find(|c| c.name == "Card").unwrap();
    assert_eq!((card.framework, card.uses), ("vue", 2));
    assert!(
        d.components
            .iter()
            .all(|c| !c.file.starts_with("src/pages"))
    );
    assert_eq!(
        find(&d, Kind::Component, "Button").other,
        ["src/components/ui/button.tsx"]
    );
}

#[test]
fn leaves_alone_what_a_package_or_the_code_defines() {
    let d = scan(&repo(
        "defined",
        &[
            (
                "package.json",
                r#"{"name": "@acme/tokens", "exports": "./tokens.css"}"#,
            ),
            (
                "tokens.css",
                ":root { --acme-blue: #2563eb; --acme-gray: #6b7280; --acme-red: #dc2626; }",
            ),
            (
                "src/app.css",
                ".a { color: var(--set-by-js); margin: var(--inline, 4px); background: var(--from-style); border-color: var(--from-svelte); }",
            ),
            (
                "src/theme.ts",
                "document.documentElement.style.setProperty('--set-by-js', '#000');",
            ),
            (
                "src/A.svelte",
                "<div style=\"--from-style: red\" style:--from-svelte={c}></div>",
            ),
        ],
    ));
    assert!(
        of(&d, Kind::Undefined).is_empty(),
        "{:?}",
        of(&d, Kind::Undefined)
    );
    assert!(
        of(&d, Kind::Unused).is_empty(),
        "a published package's tokens are used elsewhere"
    );
    assert!(d.notes.iter().any(|n| n.kind == NoteKind::Published));
    let pkg = scan(&repo(
        "pkg",
        &[
            (
                "src/main.ts",
                "import \"@radix-ui/themes/styles.css\";\nimport \"./local.css\";",
            ),
            (
                "src/local.css",
                ":root { --x: 1px; } .a { color: var(--accent-9); }",
            ),
        ],
    ));
    assert!(of(&pkg, Kind::Undefined).is_empty());
    assert_eq!(
        pkg.notes,
        [Note {
            kind: NoteKind::Packages,
            file: None,
            detail: Some("@radix-ui/themes".into())
        }]
    );
}

#[test]
fn says_nothing_of_literals_where_the_project_has_no_tokens() {
    let d = scan(&repo(
        "plain",
        &[(
            "style.css",
            "body { color: #333; padding: 13px; font-size: 15px; }",
        )],
    ));
    assert!(d.tokens.is_empty());
    assert!(d.findings.is_empty(), "{:?}", d.findings);
}

#[test]
fn stays_within_its_limits() {
    let big = format!(
        ":root {{ --big: #000; }}\n{}",
        ".a { color: #111; }\n".repeat(30_000)
    );
    let d = scan(&repo(
        "limits",
        &[("big.css", &big), ("small.css", ":root { --small: #fff; }")],
    ));
    assert!(d.tokens.iter().all(|t| t.name != "--big"));
    assert_eq!(
        d.notes,
        [Note {
            kind: NoteKind::Large,
            file: None,
            detail: Some("1".into())
        }]
    );
}
