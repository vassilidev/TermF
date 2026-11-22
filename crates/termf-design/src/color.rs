//! Colors as CSS writes them, compared the way eyes see them (OKLab) and measured the way WCAG does.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    /// sRGB, 0 to 1.
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Rgba {
    pub fn hex(&self) -> String {
        let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let mut out = format!("#{:02x}{:02x}{:02x}", c(self.r), c(self.g), c(self.b));
        if self.a < 0.999 {
            out.push_str(&format!("{:02x}", c(self.a)));
        }
        out
    }

    /// Over an opaque background.
    pub fn over(&self, bg: &Rgba) -> Rgba {
        let mix = |f: f64, b: f64| f * self.a + b * (1.0 - self.a);
        Rgba {
            r: mix(self.r, bg.r),
            g: mix(self.g, bg.g),
            b: mix(self.b, bg.b),
            a: 1.0,
        }
    }

    fn oklab(&self) -> [f64; 3] {
        let [r, g, b] = [lin(self.r), lin(self.g), lin(self.b)];
        let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
        let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
        let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
        [
            0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s,
            1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s,
            0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s,
        ]
    }

    fn luminance(&self) -> f64 {
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }
}

fn lin(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn gamma(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn from_oklab(l: f64, a: f64, b: f64, alpha: f64) -> Rgba {
    let l_ = (l + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m_ = (l - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s_ = (l - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3);
    Rgba {
        r: gamma(4.076_741_662_1 * l_ - 3.307_711_591_3 * m_ + 0.230_969_929_2 * s_),
        g: gamma(-1.268_438_004_6 * l_ + 2.609_757_401_1 * m_ - 0.341_319_396_5 * s_),
        b: gamma(-0.004_196_086_3 * l_ - 0.703_418_614_7 * m_ + 1.707_614_701 * s_),
        a: alpha,
    }
}

/// The perceptual distance (OKLab ΔE × 100): about 2 is the smallest difference most people notice side by side.
/// Colors of different opacity are never close.
pub fn delta_e(x: &Rgba, y: &Rgba) -> f64 {
    if (x.a - y.a).abs() > 0.01 {
        return f64::INFINITY;
    }
    let (p, q) = (x.oklab(), y.oklab());
    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt() * 100.0
}

/// The WCAG contrast ratio, from 1 to 21; `fg` is laid over `bg` first when translucent.
pub fn contrast(fg: &Rgba, bg: &Rgba) -> f64 {
    let fg = fg.over(bg);
    let (a, b) = (fg.luminance(), bg.luminance());
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

const NAMED: [(&str, [u8; 4]); 9] = [
    ("white", [255, 255, 255, 255]),
    ("black", [0, 0, 0, 255]),
    ("transparent", [0, 0, 0, 0]),
    ("red", [255, 0, 0, 255]),
    ("green", [0, 128, 0, 255]),
    ("blue", [0, 0, 255, 255]),
    ("gray", [128, 128, 128, 255]),
    ("grey", [128, 128, 128, 255]),
    ("silver", [192, 192, 192, 255]),
];

/// A color the way CSS writes it: hex, `rgb()`, `hsl()`, `oklch()`, `oklab()` and a few keywords; anything that
/// needs a computed value (`var()`, `color-mix()`, `currentColor`) is not one.
pub fn parse(text: &str) -> Option<Rgba> {
    let text = text.trim().to_ascii_lowercase();
    if let Some(hex) = text.strip_prefix('#') {
        return parse_hex(hex);
    }
    if let Some(&(_, [r, g, b, a])) = NAMED.iter().find(|(n, _)| *n == text) {
        let f = |v: u8| v as f64 / 255.0;
        return Some(Rgba {
            r: f(r),
            g: f(g),
            b: f(b),
            a: f(a),
        });
    }
    let open = text.find('(')?;
    let name = &text[..open];
    let inner = text[open + 1..].strip_suffix(')')?;
    let (main, alpha) = match inner.split_once('/') {
        Some((m, a)) => (m, Some(a.trim())),
        None => (inner, None),
    };
    let mut args: Vec<&str> = main
        .split([',', ' '])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let alpha = match alpha.or_else(|| (args.len() == 4).then(|| args.pop()).flatten()) {
        Some(a) => fraction(a, 1.0)?,
        None => 1.0,
    };
    if args.len() != 3 {
        return None;
    }
    match name {
        "rgb" | "rgba" => Some(Rgba {
            r: fraction(args[0], 255.0)?,
            g: fraction(args[1], 255.0)?,
            b: fraction(args[2], 255.0)?,
            a: alpha,
        }),
        "hsl" | "hsla" => {
            let h = hue(args[0])?;
            let s = fraction(args[1], 100.0)?;
            let l = fraction(args[2], 100.0)?;
            let k = |n: f64| (n + h / 30.0).rem_euclid(12.0);
            let a = s * l.min(1.0 - l);
            let f = |n: f64| l - a * (k(n) - 3.0).min(9.0 - k(n)).clamp(-1.0, 1.0);
            Some(Rgba {
                r: f(0.0),
                g: f(8.0),
                b: f(4.0),
                a: alpha,
            })
        }
        "oklch" => {
            let l = fraction(args[0], 1.0)?;
            let c = number(args[1], 0.4)?;
            let h = hue(args[2])?.to_radians();
            Some(from_oklab(l, c * h.cos(), c * h.sin(), alpha))
        }
        "oklab" => Some(from_oklab(
            fraction(args[0], 1.0)?,
            number(args[1], 0.4)?,
            number(args[2], 0.4)?,
            alpha,
        )),
        _ => None,
    }
}

fn parse_hex(hex: &str) -> Option<Rgba> {
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let full: String = match hex.len() {
        3 | 4 => hex.chars().flat_map(|c| [c, c]).collect(),
        6 | 8 => hex.to_owned(),
        _ => return None,
    };
    let byte = |i: usize| {
        u8::from_str_radix(&full[i..i + 2], 16)
            .ok()
            .map(|v| v as f64 / 255.0)
    };
    Some(Rgba {
        r: byte(0)?,
        g: byte(2)?,
        b: byte(4)?,
        a: if full.len() == 8 { byte(6)? } else { 1.0 },
    })
}

/// A number, or a percentage of `percent_of`, as a fraction of `scale` (`50%` and `128` of 255 are both ~0.5).
fn fraction(text: &str, scale: f64) -> Option<f64> {
    match text.strip_suffix('%') {
        Some(p) => p.parse::<f64>().ok().map(|v| (v / 100.0).clamp(0.0, 1.0)),
        None => text
            .parse::<f64>()
            .ok()
            .map(|v| (v / scale).clamp(0.0, 1.0)),
    }
}

/// A number, or a percentage of `full` (`oklch` chroma: 100% is 0.4).
fn number(text: &str, full: f64) -> Option<f64> {
    match text.strip_suffix('%') {
        Some(p) => p.parse::<f64>().ok().map(|v| v / 100.0 * full),
        None => text.parse().ok(),
    }
}

fn hue(text: &str) -> Option<f64> {
    if let Some(v) = text.strip_suffix("deg") {
        return v.parse().ok();
    }
    if let Some(v) = text.strip_suffix("turn") {
        return v.parse::<f64>().ok().map(|t| t * 360.0);
    }
    if let Some(v) = text.strip_suffix("rad") {
        return v.parse::<f64>().ok().map(f64::to_degrees);
    }
    if text == "none" {
        return Some(0.0);
    }
    text.parse().ok()
}

/// The color literals of a CSS value, outside `var(…)` (its fallback is the token's business) and `url(…)`, with
/// their text as written.
pub fn literals(value: &str) -> Vec<(String, Rgba)> {
    let mut out = Vec::new();
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &value[i..];
        let lower = rest
            .get(..4)
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        if lower == "var(" || lower == "url(" {
            i += skip_parens(rest);
            continue;
        }
        if bytes[i] == b'#' && (i == 0 || !is_word(bytes[i - 1])) {
            let len = rest[1..].bytes().take_while(u8::is_ascii_hexdigit).count();
            let end = 1 + len;
            if matches!(len, 3 | 4 | 6 | 8)
                && !rest[end..].bytes().next().is_some_and(is_word)
                && let Some(c) = parse_hex(&rest[1..end])
            {
                out.push((rest[..end].to_owned(), c));
            }
            i += end;
            continue;
        }
        if bytes[i].is_ascii_alphabetic() && (i == 0 || !is_word(bytes[i - 1])) {
            let name_len = rest
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric())
                .count();
            let name = rest[..name_len].to_ascii_lowercase();
            if ["rgb", "rgba", "hsl", "hsla", "oklch", "oklab"].contains(&name.as_str())
                && rest[name_len..].starts_with('(')
            {
                let end = name_len + skip_parens(&rest[name_len..]);
                if let Some(c) = parse(&rest[..end]) {
                    out.push((rest[..end].to_owned(), c));
                }
                i += end;
                continue;
            }
            i += name_len.max(1);
            continue;
        }
        i += rest.chars().next().map_or(1, char::len_utf8);
    }
    out
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

/// The length of a parenthesized group at the start of `text` (to the end when unbalanced).
fn skip_parens(text: &str) -> usize {
    let mut depth = 0;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> String {
        parse(text).unwrap_or_else(|| panic!("{text}")).hex()
    }

    #[test]
    fn reads_every_css_notation() {
        assert_eq!(hex("#abc"), "#aabbcc");
        assert_eq!(hex("#AABBCC80"), "#aabbcc80");
        assert_eq!(hex("rgb(255, 0, 0)"), "#ff0000");
        assert_eq!(hex("rgb(15 18 30 / 0.06)"), "#0f121e0f");
        assert_eq!(hex("rgba(0,0,0,50%)"), "#00000080");
        assert_eq!(hex("hsl(0 100% 50%)"), "#ff0000");
        assert_eq!(hex("hsl(120deg, 100%, 25%)"), "#008000");
        assert_eq!(hex("oklch(62.8% 0.2577 29.23)"), "#ff0000");
        assert_eq!(hex("oklch(1 0 0)"), "#ffffff");
        assert_eq!(hex("white"), "#ffffff");
        for not in [
            "var(--x)",
            "currentColor",
            "#12",
            "#ggg",
            "color-mix(in srgb, red, blue)",
            "12px",
        ] {
            assert!(parse(not).is_none(), "{not}");
        }
    }

    #[test]
    fn measures_perceptual_distance_and_wcag_contrast() {
        let c = |t: &str| parse(t).unwrap();
        assert!(delta_e(&c("#374151"), &c("#383f50")) < 1.0);
        assert!(delta_e(&c("#374151"), &c("#4b5563")) > 5.0);
        assert!(delta_e(&c("#000"), &c("#0000")).is_infinite());
        assert!((contrast(&c("#000"), &c("#fff")) - 21.0).abs() < 0.01);
        let grey = contrast(&c("#777"), &c("#fff"));
        assert!((grey - 4.48).abs() < 0.01, "{grey}");
        // Half-transparent black over white is mid grey.
        let veil = contrast(&c("rgb(0 0 0 / 0.5)"), &c("#fff"));
        assert!((3.9..4.0).contains(&veil), "{veil}");
    }

    #[test]
    fn finds_literals_outside_var_and_url() {
        let found: Vec<String> = literals(
            "1px solid #e5e7eb, var(--x, #fff) url(#a) rgb(0 0 0 / 10%) #fade-in hsl(210 40% 98%)",
        )
        .into_iter()
        .map(|(t, _)| t)
        .collect();
        assert_eq!(found, ["#e5e7eb", "rgb(0 0 0 / 10%)", "hsl(210 40% 98%)"]);
    }
}
