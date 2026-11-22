//! Comparing two captures: the pixels that moved past a threshold, grouped into a few rectangles, and a heatmap;
//! and the texts of the page that came or went.

use std::collections::BTreeMap;
use std::io::Cursor;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// Pixels are grouped by cells of this side.
const CELL: u32 = 8;
/// A cell counts when this share (of 255) of its pixels changed: lone anti-aliasing pixels do not.
const CELL_SHARE: u32 = 12;
/// Cells this close are one zone.
const DILATE: i64 = 2;
/// Zones closer than this are merged.
const GAP: i64 = 24;
const MAX_RECTS: usize = 8;
pub const THRESHOLD: u8 = 32;

#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn new(width: u32, height: u32, fill: [u8; 4]) -> Image {
        Image {
            width,
            height,
            rgba: fill.repeat((width * height) as usize),
        }
    }

    fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = ((y * self.width + x) * 4) as usize;
        Some([
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ])
    }

    pub fn fill(&mut self, x: u32, y: u32, w: u32, h: u32, color: [u8; 4]) {
        for y in y..(y + h).min(self.height) {
            for x in x..(x + w).min(self.width) {
                let i = ((y * self.width + x) * 4) as usize;
                self.rgba[i..i + 4].copy_from_slice(&color);
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

pub struct Diff {
    /// The zones that changed, top to bottom.
    pub rects: Vec<Rect>,
    /// The share of pixels that changed.
    pub ratio: f64,
    pub heatmap: Image,
}

pub fn decode(bytes: &[u8]) -> Result<Image> {
    // Full-page captures go past the default 64 MiB.
    let limits = png::Limits { bytes: 1 << 30 };
    let mut decoder = png::Decoder::new_with_limits(Cursor::new(bytes), limits);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().context("not a PNG image")?;
    let mut buf = vec![0; reader.output_buffer_size().context("image too large")?];
    let info = reader
        .next_frame(&mut buf)
        .context("unreadable PNG image")?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .chunks(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf
            .chunks(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Indexed => bail!("unexpected indexed PNG"),
    };
    Ok(Image {
        width: info.width,
        height: info.height,
        rgba,
    })
}

pub fn encode(image: &Image) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&image.rgba)?;
    writer.finish()?;
    Ok(out)
}

/// How far two pixels are apart, on their most changed colour channel; a pixel only one image has is as far
/// as can be.
fn distance(a: Option<[u8; 4]>, b: Option<[u8; 4]>) -> u8 {
    match (a, b) {
        (Some(a), Some(b)) => (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0),
        _ => 255,
    }
}

/// Compares the captures on the canvas that holds both: what one has beyond the other changed.
pub fn compare(before: &Image, after: &Image, threshold: u8) -> Diff {
    let (w, h) = (
        before.width.max(after.width),
        before.height.max(after.height),
    );
    let (gw, gh) = (w.div_ceil(CELL), h.div_ceil(CELL));
    let mut counts = vec![0u32; (gw * gh) as usize];
    let mut heatmap = Image::new(w, h, [255, 255, 255, 255]);
    let mut changed = 0u64;
    for y in 0..h {
        for x in 0..w {
            let (a, b) = (before.pixel(x, y), after.pixel(x, y));
            let d = distance(a, b);
            let base = b.or(a).unwrap_or([255; 4]);
            let lum = (base[0] as u32 * 299 + base[1] as u32 * 587 + base[2] as u32 * 114) / 1000;
            // The page, faded to a light grey, and what changed in red, the stronger the further.
            let faded = (255 - (255 - lum) * 35 / 100) as u8;
            let color = if d > threshold {
                changed += 1;
                counts[((y / CELL) * gw + x / CELL) as usize] += 1;
                let k = 120 + d as u32 * 135 / 255;
                let mix = |to: u32| ((faded as u32 * (255 - k) + to * k) / 255) as u8;
                [mix(220), mix(20), mix(60), 255]
            } else {
                [faded, faded, faded, 255]
            };
            let i = ((y * w + x) * 4) as usize;
            heatmap.rgba[i..i + 4].copy_from_slice(&color);
        }
    }
    let on: Vec<bool> = (0..gh)
        .flat_map(|cy| (0..gw).map(move |cx| (cx, cy)))
        .map(|(cx, cy)| {
            let cell_w = (w - cx * CELL).min(CELL);
            let cell_h = (h - cy * CELL).min(CELL);
            counts[(cy * gw + cx) as usize] * 255 > CELL_SHARE * cell_w * cell_h
        })
        .collect();
    let rects = zones(&on, gw as i64, gh as i64)
        .into_iter()
        .map(|[x0, y0, x1, y1]| {
            let (x0, y0) = (x0 as u32 * CELL, y0 as u32 * CELL);
            let (x1, y1) = ((x1 as u32 + 1) * CELL, (y1 as u32 + 1) * CELL);
            [x0 as i64, y0 as i64, x1.min(w) as i64, y1.min(h) as i64]
        })
        .collect();
    let rects = merge(rects)
        .into_iter()
        .map(|[x0, y0, x1, y1]| Rect {
            x: x0 as u32,
            y: y0 as u32,
            w: (x1 - x0) as u32,
            h: (y1 - y0) as u32,
        })
        .collect();
    Diff {
        rects,
        ratio: changed as f64 / (w as u64 * h as u64).max(1) as f64,
        heatmap,
    }
}

/// The bounding boxes, in cells, of the groups of cells that changed, each grown by [`DILATE`] cells so that
/// near cells join.
fn zones(on: &[bool], gw: i64, gh: i64) -> Vec<[i64; 4]> {
    let at = |x: i64, y: i64| x >= 0 && y >= 0 && x < gw && y < gh && on[(y * gw + x) as usize];
    let grown: Vec<bool> = (0..gh)
        .flat_map(|y| (0..gw).map(move |x| (x, y)))
        .map(|(x, y)| (-DILATE..=DILATE).any(|dy| (-DILATE..=DILATE).any(|dx| at(x + dx, y + dy))))
        .collect();
    let mut seen = vec![false; grown.len()];
    let mut boxes = Vec::new();
    for start in 0..grown.len() {
        if seen[start] || !grown[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![start as i64];
        let (sx, sy) = (start as i64 % gw, start as i64 / gw);
        let mut b = [sx, sy, sx, sy];
        while let Some(i) = stack.pop() {
            let (x, y) = (i % gw, i / gw);
            b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
            for (nx, ny) in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                let n = ny * gw + nx;
                if nx >= 0
                    && ny >= 0
                    && nx < gw
                    && ny < gh
                    && grown[n as usize]
                    && !seen[n as usize]
                {
                    seen[n as usize] = true;
                    stack.push(n);
                }
            }
        }
        boxes.push(b);
    }
    boxes
}

/// Joins the boxes that are near each other, then the ones whose union wastes least until a few are left.
fn merge(mut boxes: Vec<[i64; 4]>) -> Vec<[i64; 4]> {
    let union = |a: [i64; 4], b: [i64; 4]| {
        [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]
    };
    let area = |r: [i64; 4]| (r[2] - r[0]) * (r[3] - r[1]);
    let near = |a: [i64; 4], b: [i64; 4]| {
        a[0] - GAP < b[2] && b[0] - GAP < a[2] && a[1] - GAP < b[3] && b[1] - GAP < a[3]
    };
    'again: loop {
        for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                if near(boxes[i], boxes[j]) {
                    let b = boxes.remove(j);
                    boxes[i] = union(boxes[i], b);
                    continue 'again;
                }
            }
        }
        break;
    }
    while boxes.len() > MAX_RECTS {
        let (_, i, j) = (0..boxes.len())
            .flat_map(|i| (i + 1..boxes.len()).map(move |j| (i, j)))
            .map(|(i, j)| {
                (
                    area(union(boxes[i], boxes[j])) - area(boxes[i]) - area(boxes[j]),
                    i,
                    j,
                )
            })
            .min()
            .expect("more than one box");
        let b = boxes.remove(j);
        boxes[i] = union(boxes[i], b);
    }
    boxes.sort_by_key(|r| (r[1], r[0]));
    boxes
}

/// The texts `after` has more of than `before`, and the ones it has fewer of.
pub fn texts(before: &[String], after: &[String]) -> (Vec<String>, Vec<String>) {
    let mut count: BTreeMap<&str, i64> = BTreeMap::new();
    for t in after {
        *count.entry(t).or_default() += 1;
    }
    for t in before {
        *count.entry(t).or_default() -= 1;
    }
    let mut added = Vec::new();
    let mut removed = Vec::new();
    for (text, n) in count {
        let list = if n > 0 { &mut added } else { &mut removed };
        list.extend((0..n.abs()).map(|_| text.to_owned()));
    }
    (added, removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: [u8; 4] = [255, 255, 255, 255];

    #[test]
    fn an_identical_image_has_no_difference() {
        let mut a = Image::new(320, 200, WHITE);
        a.fill(10, 10, 50, 20, [0, 0, 0, 255]);
        let diff = compare(&a, &a.clone(), THRESHOLD);
        assert!(diff.rects.is_empty());
        assert_eq!(diff.ratio, 0.0);
        assert_eq!((diff.heatmap.width, diff.heatmap.height), (320, 200));
    }

    #[test]
    fn groups_changes_into_rectangles() {
        let before = Image::new(600, 400, WHITE);
        let mut after = before.clone();
        after.fill(50, 40, 70, 30, [0, 0, 0, 255]);
        after.fill(300, 200, 60, 60, [255, 0, 0, 255]);
        let diff = compare(&before, &after, THRESHOLD);
        assert_eq!(diff.rects.len(), 2, "{:?}", diff.rects);
        let (a, b) = (diff.rects[0], diff.rects[1]);
        assert!(
            a.x <= 50 && a.y <= 40 && a.x + a.w >= 120 && a.y + a.h >= 70,
            "{a:?}"
        );
        assert!(
            b.x <= 300 && b.y <= 200 && b.x + b.w >= 360 && b.y + b.h >= 260,
            "{b:?}"
        );
        assert!(diff.ratio > 0.0 && diff.ratio < 0.1);
        // Red where it changed, grey elsewhere.
        let px = diff.heatmap.pixel(320, 220).unwrap();
        assert!(px[0] > px[1] + 80, "{px:?}");
        let px = diff.heatmap.pixel(5, 5).unwrap();
        assert_eq!(px[0], px[1]);
    }

    #[test]
    fn changes_under_the_threshold_do_not_count() {
        let before = Image::new(200, 100, WHITE);
        let mut after = before.clone();
        after.fill(0, 0, 200, 100, [240, 240, 240, 255]);
        assert!(compare(&before, &after, THRESHOLD).rects.is_empty());
        assert_eq!(compare(&before, &after, 10).rects.len(), 1);
        // A few stray pixels are anti-aliasing, not a change.
        let mut after = before.clone();
        after.fill(100, 50, 1, 1, [0, 0, 0, 255]);
        assert!(compare(&before, &after, THRESHOLD).rects.is_empty());
    }

    #[test]
    fn a_longer_page_changed_where_the_other_has_nothing() {
        let before = Image::new(100, 100, WHITE);
        let after = Image::new(100, 180, WHITE);
        let r = compare(&before, &after, THRESHOLD).rects;
        assert_eq!(r.len(), 1);
        assert!(
            r[0].y <= 100 && r[0].y + r[0].h == 180 && r[0].w == 100,
            "{r:?}"
        );
    }

    #[test]
    fn keeps_a_few_rectangles() {
        let before = Image::new(2000, 200, WHITE);
        let mut after = before.clone();
        for i in 0..12 {
            after.fill(i * 160, 50, 10, 10, [0, 0, 0, 255]);
        }
        let rects = compare(&before, &after, THRESHOLD).rects;
        assert_eq!(rects.len(), MAX_RECTS);
        let covered: u32 = rects.iter().map(|r| r.w).sum();
        assert!(covered >= 12 * 10, "{rects:?}");
    }

    #[test]
    fn round_trips_png() {
        let mut a = Image::new(30, 20, WHITE);
        a.fill(3, 4, 5, 6, [1, 2, 3, 255]);
        assert_eq!(decode(&encode(&a).unwrap()).unwrap(), a);
        assert!(decode(b"not a png").is_err());
    }

    #[test]
    fn lists_the_texts_that_came_and_went() {
        let s = |v: &[&str]| v.iter().map(|t| t.to_string()).collect::<Vec<_>>();
        let (added, removed) = texts(
            &s(&["Invoices", "Export", "Total", "Total"]),
            &s(&["Invoices", "Exporter en CSV", "Total"]),
        );
        assert_eq!(added, s(&["Exporter en CSV"]));
        assert_eq!(removed, s(&["Export", "Total"]));
        assert_eq!(texts(&s(&["a"]), &s(&["a"])), (vec![], vec![]));
    }
}
