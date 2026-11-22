# Exports: files, commands, wiring

Read when you produce or wire the exports. `<skill-dir>` is the folder of the skill.

## The files

| File (in `brand/exports/`) | Size | From | Note |
|---|---|---|---|
| `logo-<width>.png` | 512, 1024, 2048 wide | `logo.svg` | transparent; documents, slides, press |
| `logo-reversed-1024.png` | 1024 wide | `logo-reversed.svg` | for dark grounds |
| `mark-<n>.png` | 16, 32, 48, 180, 192, 256, 512, 1024 | `mark.svg` (`mark-small.svg` up to 32) | intermediate files for the icons |
| `favicon.ico` | 16, 32, 48 in one file | `mark-16/32/48.png` | `/favicon.ico` at the site root |
| `icon.svg` | vector | `mark.svg` | modern browsers; dark variant inside |
| `apple-touch-icon.png` | 180 × 180 | mark on the brand background | opaque, about 12 % padding |
| `icon-192.png`, `icon-512.png` | 192, 512 | `mark.svg` | web manifest, `purpose: any` |
| `icon-maskable-512.png` | 512 | mark on the brand background | mark inside the centre circle (radius 40 %) |
| `og.png` | 1200 × 630 | an HTML page rendered by Chrome | link previews; text within 60 px margins |
| `app-icon-1024.png` | 1024 × 1024 | the app icon design | source for `tauri icon` or the platform's tool |

## SVG to PNG, by tool

```sh
rsvg-convert --width=1024 --keep-aspect-ratio -o brand/exports/logo-1024.png brand/logo/logo.svg
resvg -w 1024 brand/logo/logo.svg brand/exports/logo-1024.png
inkscape brand/logo/logo.svg --export-type=png --export-width=1024 --export-filename=brand/exports/logo-1024.png
magick -background none -density 384 brand/logo/logo.svg -resize 1024x brand/exports/logo-1024.png
```

Headless Chrome, for a square mark (the SVG fills the window when its root has only a `viewBox`):

```sh
"<chrome>" --headless --hide-scrollbars --force-device-scale-factor=1 --default-background-color=00000000 \
  --window-size=512,512 --screenshot="$PWD/brand/exports/mark-512.png" "file://$PWD/brand/logo/mark.svg"
```

For a size that is not square, or a mark on a background, write a small HTML page sized to the target and screenshot it the same way.

## Resizing and packing

```sh
sips -z 32 32 brand/exports/mark-512.png --out brand/exports/mark-32.png        # macOS: height, then width
magick brand/exports/mark-512.png -resize 32x32 brand/exports/mark-32.png
magick brand/exports/mark-16.png brand/exports/mark-32.png brand/exports/mark-48.png brand/exports/favicon.ico
magick brand/exports/mark-256.png -define icon:auto-resize=16,32,48 brand/exports/favicon.ico   # input 256 px
python3 <skill-dir>/assets/png-to-ico.py brand/exports/favicon.ico brand/exports/mark-16.png brand/exports/mark-32.png brand/exports/mark-48.png
```

Windows app icon: the same ICO with 16, 24, 32, 48, 64 and 256. macOS app icon without `tauri icon`:

```sh
mkdir App.iconset   # icon_16x16.png icon_16x16@2x.png icon_32x32.png icon_32x32@2x.png icon_128x128.png
                    # icon_128x128@2x.png icon_256x256.png icon_256x256@2x.png icon_512x512.png icon_512x512@2x.png (1024)
iconutil -c icns -o brand/exports/app.icns App.iconset
```

Tauri: `<package manager> tauri icon brand/exports/app-icon-1024.png` writes `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.png`, `icon.ico`, `icon.icns`, the Windows `Square*Logo.png` and the Android and iOS icons into `src-tauri/icons/` (`-o` for another folder). Mobile projects outside Tauri use their framework's own icon tool, as their docs say.

## The social image

Write `brand/og.html`: a 1200 × 630 body with the brand background, the logo (inline SVG), the product name and the tagline the ticket gives, text inside 60 px margins, fonts local. Then:

```sh
"<chrome>" --headless --hide-scrollbars --force-device-scale-factor=1 --window-size=1200,630 \
  --screenshot="$PWD/brand/exports/og.png" "file://$PWD/brand/og.html"
```

## The adaptive SVG favicon

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
  <style>path { fill: #2b4ce0 } @media (prefers-color-scheme: dark) { path { fill: #8099ff } }</style>
  <path d="…"/>
</svg>
```

## Wiring

```html
<link rel="icon" href="/favicon.ico" sizes="any">
<link rel="icon" href="/icon.svg" type="image/svg+xml">
<link rel="apple-touch-icon" href="/apple-touch-icon.png">
<link rel="manifest" href="/manifest.webmanifest">
```

```json
{ "icons": [
  { "src": "/icon-192.png", "sizes": "192x192", "type": "image/png" },
  { "src": "/icon-512.png", "sizes": "512x512", "type": "image/png" },
  { "src": "/icon-maskable-512.png", "sizes": "512x512", "type": "image/png", "purpose": "maskable" }
] }
```

A framework with its own metadata API (Next.js `app/icon.*` files, Nuxt or SvelteKit head config) takes the same files through that API instead of hand-written tags; the social image's `og:image` tags are the `seo` role's.
