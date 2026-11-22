// TermF's app icons and installer images, from the brand mark (site/icon.svg), the macOS icon
// (src-tauri/icons/icon.svg) and the colour tokens of src/app.css. Run from app/ after a brand change:
// `node src-tauri/installer/make.mjs`. The project's Tauri CLI renders the SVGs (`tauri icon --png`).
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { crc32, deflateSync, inflateSync } from "node:zlib";

const here = dirname(fileURLToPath(import.meta.url));
const tauriDir = join(here, "..");
const app = join(tauriDir, "..");
const icons = join(tauriDir, "icons");
const cli = join(app, "node_modules/@tauri-apps/cli/tauri.js");
const css = readFileSync(join(app, "src/app.css"), "utf8");
// The light theme's value: it comes first in app.css.
const token = (name) => css.match(new RegExp(`--${name}: (#[0-9a-f]{6})`))[1];
const mark = readFileSync(join(app, "../site/icon.svg"), "utf8").replace(/^<svg[^>]*>|<\/svg>\s*$/g, "");
const macIcon = readFileSync(join(icons, "icon.svg"), "utf8");

// Thicker chevrons, so that 16 to 32 pixel icons stay sharp.
const bold = mark.replace(/stroke-width="(\d+)"/g, (_, w) => `stroke-width="${Math.round(w * 1.15)}"`);
const chevrons = mark.match(/<g[\s\S]*<\/g>/)[0];
// The mark's squircle spans 100..924 of its 1024 grid: `at` fits it in a `size` square whose corner is x, y.
const at = (x, y, size, body = mark) =>
  `<g transform="translate(${x} ${y}) scale(${size / 824}) translate(-100 -100)">${body}</g>`;
// The Tauri CLI renders square images only: wider ones are drawn in the top left corner of a square, then cropped.
const svg = (w, h, body) => {
  const side = Math.max(w, h);
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${side}" height="${side}" viewBox="0 0 ${side} ${side}">${body}</svg>`;
};
const brandFill = (id) =>
  `<defs><linearGradient id="${id}" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#5d5fea"/><stop offset="1" stop-color="#3f3fc6"/></linearGradient></defs>`;
// A brand panel: the icon's gradient with its three chevrons, as on the installers' welcome pages.
const panel = (w, h) =>
  `${brandFill("panel")}<rect width="${w}" height="${h}" fill="url(#panel)"/>` +
  `<g transform="translate(${w / 2} ${h * 0.34}) scale(${(w * 0.46) / 362}) translate(-510 -512)">${chevrons}</g>`;

const tmp = mkdtempSync(join(tmpdir(), "termf-brand-"));
let count = 0;
function render(source, sizes) {
  const file = join(tmp, `${count}.svg`);
  const out = join(tmp, `${count++}`);
  writeFileSync(file, source);
  execFileSync(process.execPath, [cli, "icon", file, "--png", sizes.join(","), "-o", out], { stdio: "ignore" });
  return Object.fromEntries(sizes.map((s) => [s, readFileSync(join(out, `${s}x${s}.png`))]));
}

function decode(png) {
  let w = 0;
  let h = 0;
  const idat = [];
  for (let i = 8; i < png.length; ) {
    const len = png.readUInt32BE(i);
    const type = png.toString("ascii", i + 4, i + 8);
    const data = png.subarray(i + 8, i + 8 + len);
    if (type === "IHDR") {
      [w, h] = [data.readUInt32BE(0), data.readUInt32BE(4)];
      if (data[8] !== 8 || data[9] !== 6 || data[12] !== 0) throw new Error("expected an 8-bit RGBA PNG");
    }
    if (type === "IDAT") idat.push(data);
    i += 12 + len;
  }
  const raw = inflateSync(Buffer.concat(idat));
  const stride = w * 4;
  const px = Buffer.alloc(stride * h);
  for (let y = 0; y < h; y++) {
    const filter = raw[y * (stride + 1)];
    const line = y * (stride + 1) + 1;
    for (let x = 0; x < stride; x++) {
      const a = x >= 4 ? px[y * stride + x - 4] : 0;
      const b = y ? px[(y - 1) * stride + x] : 0;
      const c = x >= 4 && y ? px[(y - 1) * stride + x - 4] : 0;
      const p = a + b - c;
      const [pa, pb, pc] = [Math.abs(p - a), Math.abs(p - b), Math.abs(p - c)];
      const paeth = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      px[y * stride + x] = (raw[line + x] + [0, a, b, (a + b) >> 1, paeth][filter]) & 255;
    }
  }
  return { w, h, px };
}

const crop = ({ w, px }, cw, ch) => {
  const out = Buffer.alloc(cw * ch * 4);
  for (let y = 0; y < ch; y++) px.copy(out, y * cw * 4, y * w * 4, (y * w + cw) * 4);
  return { w: cw, h: ch, px: out };
};

function encodePng({ w, h, px }, dpi = 72) {
  const chunk = (type, data) => {
    const b = Buffer.alloc(12 + data.length);
    b.writeUInt32BE(data.length);
    b.write(type, 4, "ascii");
    data.copy(b, 8);
    b.writeUInt32BE(crc32(b.subarray(4, 8 + data.length)), 8 + data.length);
    return b;
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  const raw = Buffer.alloc((w * 4 + 1) * h);
  for (let y = 0; y < h; y++) px.copy(raw, y * (w * 4 + 1) + 1, y * w * 4, (y + 1) * w * 4);
  // pHYs: Finder shows a 144 dpi background at half its pixel size, sharp on Retina screens.
  const phys = Buffer.alloc(9);
  phys.writeUInt32BE(Math.round(dpi / 0.0254));
  phys.writeUInt32BE(Math.round(dpi / 0.0254), 4);
  phys[8] = 1;
  const sig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  return Buffer.concat([sig, chunk("IHDR", ihdr), chunk("pHYs", phys), chunk("IDAT", deflateSync(raw, { level: 9 })), chunk("IEND", Buffer.alloc(0))]);
}

// 24-bit BMP, as NSIS and WiX want, transparency flattened on white.
function bmp({ w, h, px }) {
  const stride = Math.ceil((w * 3) / 4) * 4;
  const b = Buffer.alloc(54 + stride * h);
  b.write("BM");
  b.writeUInt32LE(b.length, 2);
  b.writeUInt32LE(54, 10);
  b.writeUInt32LE(40, 14);
  b.writeInt32LE(w, 18);
  b.writeInt32LE(h, 22);
  b.writeUInt16LE(1, 26);
  b.writeUInt16LE(24, 28);
  b.writeUInt32LE(stride * h, 34);
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      const a = px[i + 3] / 255;
      const o = 54 + (h - 1 - y) * stride + x * 3;
      for (let c = 0; c < 3; c++) b[o + c] = Math.round(px[i + 2 - c] * a + 255 * (1 - a));
    }
  return b;
}

function ico(images) {
  const head = Buffer.alloc(6 + 16 * images.length);
  head.writeUInt16LE(1, 2);
  head.writeUInt16LE(images.length, 4);
  let offset = head.length;
  images.forEach(([size, png], i) => {
    const e = 6 + 16 * i;
    head[e] = head[e + 1] = size & 255;
    head.writeUInt16LE(1, e + 4);
    head.writeUInt16LE(32, e + 6);
    head.writeUInt32LE(png.length, e + 8);
    head.writeUInt32LE(offset, e + 12);
    offset += png.length;
  });
  return Buffer.concat([head, ...images.map(([, png]) => png)]);
}

// is32 / il32 hold run-length coded RGB channels; literal runs only (a header byte n, then n + 1 bytes) is valid.
function icnsRgb({ px }) {
  const out = [];
  for (let c = 0; c < 3; c++) {
    const channel = [];
    for (let i = c; i < px.length; i += 4) channel.push(px[i]);
    for (let i = 0; i < channel.length; i += 128) {
      const run = channel.slice(i, i + 128);
      out.push(run.length - 1, ...run);
    }
  }
  return Buffer.from(out);
}
const icnsMask = ({ px }) => Buffer.from(px.filter((_, i) => i % 4 === 3));
function icns(entries) {
  const part = (type, data) => {
    const h = Buffer.alloc(8);
    h.write(type);
    h.writeUInt32BE(data.length + 8, 4);
    return Buffer.concat([h, data]);
  };
  return part("icns", Buffer.concat(entries.map(([type, data]) => part(type, data))));
}

try {
  // macOS: the Apple grid (src-tauri/icons/icon.svg); up to 32 px, without its shadow, with thicker chevrons and
  // the squircle on whole pixels (1 px margin at 16, 3 at 32).
  const mac = render(macIcon, [64, 128, 256, 512, 1024]);
  const macSmall = { ...render(svg(1024, 1024, at(64, 64, 896, bold)), [16]), ...render(svg(1024, 1024, at(96, 96, 832, bold)), [32]) };
  writeFileSync(
    join(icons, "icon.icns"),
    icns([
      ["is32", icnsRgb(decode(macSmall[16]))],
      ["s8mk", icnsMask(decode(macSmall[16]))],
      ["il32", icnsRgb(decode(macSmall[32]))],
      ["l8mk", icnsMask(decode(macSmall[32]))],
      ["ic11", macSmall[32]],
      ["ic12", mac[64]],
      ["ic07", mac[128]],
      ["ic13", mac[256]],
      ["ic08", mac[256]],
      ["ic14", mac[512]],
      ["ic09", mac[512]],
      ["ic10", mac[1024]],
    ]),
  );

  // Windows and Linux: the squircle fills the canvas but for a 1/16 margin, on whole pixels at 16, 32, 48…
  const flat = render(svg(1024, 1024, at(64, 64, 896)), [48, 64, 128, 256, 512]);
  const flatSmall = render(svg(1024, 1024, at(64, 64, 896, bold)), [16, 24, 32]);
  writeFileSync(
    join(icons, "icon.ico"),
    ico([16, 24, 32].map((s) => [s, flatSmall[s]]).concat([48, 64, 256].map((s) => [s, flat[s]]))),
  );
  writeFileSync(join(icons, "32x32.png"), flatSmall[32]);
  for (const s of [64, 128, 256]) writeFileSync(join(icons, `${s}x${s}.png`), flat[s]);
  writeFileSync(join(icons, "icon.png"), flat[512]);

  // NSIS: header (150 × 57, right of the white title area) and welcome / finish panel (164 × 314).
  const white = (w, h) => `<rect width="${w}" height="${h}" fill="#ffffff"/>`;
  const image = (body, w, h, scale = 1) =>
    crop(decode(render(svg(w * scale, h * scale, body), [Math.max(w, h) * scale])[Math.max(w, h) * scale]), w * scale, h * scale);
  const header = (w, h) => white(w, h) + at(w - 12 - 40, (h - 40) / 2, 40);
  writeFileSync(join(here, "nsis-header.bmp"), bmp(image(header(150, 57), 150, 57)));
  writeFileSync(join(here, "nsis-sidebar.bmp"), bmp(image(panel(164, 314), 164, 314)));
  // WiX: banner (493 × 58, text on its left) and dialog (493 × 312, text right of 164 px).
  writeFileSync(join(here, "wix-banner.bmp"), bmp(image(header(493, 58), 493, 58)));
  writeFileSync(join(here, "wix-dialog.bmp"), bmp(image(white(493, 312) + panel(164, 312), 493, 312)));

  // DMG window, 660 × 400 points, drawn at 2x: the app at (180, 170), Applications at (480, 170), chevrons between.
  const s = 2;
  const dmg =
    `<defs><linearGradient id="bg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${token("chrome")}"/><stop offset="1" stop-color="${token("bg")}"/></linearGradient></defs>` +
    `<rect width="${660 * s}" height="${400 * s}" fill="url(#bg)"/>` +
    [0.35, 0.65, 1]
      .map(
        (opacity, i) =>
          `<path d="M${(306 + i * 20) * s} ${156 * s} l${12 * s} ${14 * s} l${-12 * s} ${14 * s}" fill="none" stroke="${token("accent")}" stroke-opacity="${opacity}" stroke-width="${5 * s}" stroke-linecap="round" stroke-linejoin="round"/>`,
      )
      .join("");
  writeFileSync(join(here, "dmg-background.png"), encodePng(image(dmg, 660 * s, 400 * s), 144));
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
