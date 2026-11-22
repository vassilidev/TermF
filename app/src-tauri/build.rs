use std::{env, fs, path::Path};

// The window's colour before the page paints and the macOS menu's words come from the UI's own sources, so that
// they never drift: `--bg` of app.css (light, then dark) and the `packagingMenu*` keys of i18n.ts (fr, then en).
fn brand() {
    let css = fs::read_to_string("../src/app.css").expect("app.css");
    let i18n = fs::read_to_string("../src/lib/i18n.ts").expect("i18n.ts");
    println!("cargo:rerun-if-changed=../src/app.css");
    println!("cargo:rerun-if-changed=../src/lib/i18n.ts");
    let colours: Vec<String> = css
        .split("--bg:")
        .skip(1)
        .map(|rest| {
            let hex = rest.trim_start().get(1..7).expect("--bg: #rrggbb");
            let c = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).expect("--bg: #rrggbb");
            format!("Color({}, {}, {}, 255)", c(0), c(2), c(4))
        })
        .collect();
    assert_eq!(
        colours.len(),
        2,
        "app.css: one --bg for light, one for dark"
    );
    let english = i18n.find("\nconst en").expect("i18n.ts: const en");
    let words = |part: &str| {
        part.lines()
            .filter_map(|line| line.trim().strip_prefix("packagingMenu"))
            .map(|rest| {
                let (key, text) = rest.split_once(": ").expect("packagingMenu key");
                let text = text.trim_end_matches(',').trim_matches('"');
                format!("({key:?}, {text:?})")
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let code = format!(
        "pub const BACKGROUNDS: [Color; 2] = [{}];\n\
         #[cfg_attr(not(target_os = \"macos\"), allow(dead_code))]\n\
         pub const MENU: [&[(&str, &str)]; 2] = [&[{}], &[{}]];\n",
        colours.join(", "),
        words(&i18n[..english]),
        words(&i18n[english..]),
    );
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("brand.rs"),
        code,
    )
    .unwrap();
}

fn main() {
    brand();
    tauri_build::build()
}
