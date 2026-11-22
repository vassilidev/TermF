---
name: logo-export
description: >-
  Cleans logo SVGs and exports every file a product needs from them, with the tools already on the machine:
  PNG at the useful sizes, favicon.ico (16, 32, 48) and icon.svg, apple-touch-icon, web manifest and maskable
  icons, app icons (ICNS, ICO, PNG, `tauri icon` in a Tauri project), the 1200 × 630 social image, mono and
  reversed variants, each file checked. Use when a task asks for a logo file, a favicon, app or home-screen
  icons, an OG or social image, or to export, resize or convert an SVG mark. Not for designing the identity,
  colours or guidelines (brand-identity), nor for the page metadata that links a social image (seo).
license: MIT
metadata:
  termf-stacks: "*"
  termf-roles: "brand"
  termf-versions: "*"
  termf-verified: "2026-11-22 Tauri CLI 2 icon, librsvg rsvg-convert, resvg 0.50, ImageMagick 7, macOS sips and iconutil, Chrome headless"
  termf-sources: "https://v2.tauri.app/develop/icons/ https://v2.tauri.app/reference/cli/ https://gitlab.gnome.org/GNOME/librsvg/-/blob/main/rsvg-convert.rst https://github.com/linebender/resvg https://imagemagick.org/script/defines.php https://web.dev/articles/add-manifest https://web.dev/articles/maskable-icon https://web.dev/articles/building/an-adaptive-favicon https://developer.chrome.com/docs/chromium/headless"
---
# Logo files and exports

`<skill-dir>` below is the folder of this file. Write every export to `brand/exports/` (then copy to where the app serves it: `public/`, `static/`, the app's icon folder), the sources to `brand/logo/`.

1. **Inventory.** List the SVG sources and where the project expects each export: the web root (`public/`, `static/`), the HTML head or the framework's metadata, the web manifest, the desktop or mobile icon folders (`src-tauri/icons/`, `build/`, `android/`, `ios/`). Done when each file to produce has its path and size.
2. **Clean each SVG.** One `viewBox`, no fixed `width`/`height` on the root unless a size is the point; text converted to paths (a `<text>` element renders with another font where the font is missing); no `<image>`, `<script>`, `<foreignObject>`, `on…=` attributes or external `href`; colours as `fill`/`stroke` attributes, which every renderer reads; editor metadata (Inkscape, Illustrator, Figma ids) removed; coordinates at two or three decimals; a `<title>` with the product name. Converting text needs a tool that has the font (Inkscape `--export-text-to-path`, or the design tool it came from): without one, ask for the outlined file. Done when `grep -nE '<text|<image|<script|foreignObject| on[a-z]+=' brand/logo/*.svg` finds nothing.
3. **Variants as files.** `logo.svg` (primary), `logo-reversed.svg` (for dark grounds), `logo-mono.svg` (one colour, black), `mark.svg` (the symbol alone), and `stacked` when the brand has it; each with fixed colours, because an exported file has no surrounding `currentColor`. Done when each file renders on its intended ground.
4. **Small sizes.** Render `mark.svg` at 16, 24 and 32 px and look at the images. When a detail closes up, draw a simplified small mark (`mark-small.svg`) for the favicon and the 16–32 px icons. Done when the 16 px render reads as the mark.
5. **Find the tool, in this order:** `rsvg-convert`, `resvg`, `inkscape`, `magick` (ImageMagick 7), then headless Chrome (installed with the project's browser tests or on the desktop); `sips` and `iconutil` on macOS for resizing and ICNS, `python3` for ICO. Check each with `command -v`, write which ones you use, and ask before installing anything. Done when every export below has a tool.
6. **Export.** Use the table and the commands of `references/exports.md`: PNG sizes, `favicon.ico`, `icon.svg` (with a dark variant inside), `apple-touch-icon.png` (180 × 180, opaque), `icon-192.png`, `icon-512.png`, `icon-maskable-512.png`, `og.png` (1200 × 630), app icons. For a Tauri project, give `tauri icon` a 1024 px square PNG with transparency (or the SVG) through the project's package manager (`pnpm tauri icon brand/exports/app-icon-1024.png`): it writes every desktop and mobile icon into `src-tauri/icons/`. Done when every file of step 1 exists.
7. **Check every file.** Dimensions and type: `file <file>` (PNG sizes, the icons inside an ICO), `sips -g pixelWidth -g pixelHeight <file>` on macOS, or `magick identify <file>`. Then look at each image with the Read tool: transparent where it should be, opaque where iOS needs it, nothing cropped, the mark centred. Done when each file has its check in your report.
8. **Wire it** when the task asks: the `<link>` tags or the framework's metadata, the manifest's `icons`, and the files copied where the app serves them (snippet in `references/exports.md`).
9. Run the commands of `## Verify` and put each one with its result in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- A `<text>` element in a logo renders with a fallback font on any machine without that font, including the CI and the reader's browser: outline the text before any export.
- `sips -z` takes the height first, then the width; it resizes PNGs well but does not read SVG reliably: render the SVG with another tool first.
- `magick … -define icon:auto-resize=16,32,48` needs a 256 px input; with smaller PNGs, list them instead (`magick fav-16.png fav-32.png fav-48.png favicon.ico`) or use `<skill-dir>/assets/png-to-ico.py`.
- iOS fills a transparent apple-touch-icon with black: give it the brand background and some padding.
- A maskable icon is cropped to a circle or a squircle: keep the mark inside the centre circle of radius 40 % of the width.
- `tauri icon` rejects a non-square source unless `--fit` is given, and `-p` (custom sizes) skips all the default icons.
- An `.iconset` needs the exact names `icon_16x16.png` … `icon_512x512@2x.png` (the last one is 1024 px) for `iconutil -c icns`.
- Headless Chrome screenshots at the device scale factor: pass `--force-device-scale-factor=1` for exact pixel sizes, and `--default-background-color=00000000` for a transparent background.

## Verify

- `grep -nE '<text|<image|<script|foreignObject| on[a-z]+=' brand/logo/*.svg` prints nothing.
- `file brand/exports/*` shows each PNG at its size and `favicon.ico` with its 16, 32 and 48 icons.
- In a Tauri project, the project's build (`pnpm tauri build` or its CI) picks the new icons up.
- Each image looked at; the 16 px favicon and the maskable icon in particular.

## Security

- Logos from outside (a client's file, an old site) can carry scripts or links in their SVG: clean them (step 2) before they reach the app, where an inline SVG runs in the page.
- A logo is never built from a stock image or a "free logo" asset: free licences exclude trademarks.
- Install no tool without asking, and never with `curl … | sh`; prefer the system package manager the user already uses.
- The social image shows only public text: no unreleased name, internal URL or real customer.

## References

- Read `references/exports.md` for the file list, the exact commands per tool and the HTML snippet.
- `<skill-dir>/assets/png-to-ico.py`: packs square PNGs into an ICO with the Python standard library.
