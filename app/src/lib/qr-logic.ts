// No imports, so `pnpm test` runs it in plain Node.

/**
 * A QR code (ISO/IEC 18004) of `text`: byte mode, error correction level M, versions 1 to 10 (up to 213 bytes of UTF-8,
 * plenty for a link). `rows[y][x]` is true for a dark module; the quiet zone around it is the caller's. `mask` forces
 * one of the eight masks, otherwise the one with the lowest penalty is taken.
 */
export function qr(text: string, mask?: number): boolean[][] {
  const data = [...new TextEncoder().encode(text)];
  const version = VERSIONS.findIndex((v, i) => data.length <= capacity(i + 1, v)) + 1;
  if (version === 0) throw new Error(`too long for a QR code: ${data.length} bytes`);
  const codewords = interleave(version, encode(version, data));
  const base = new Grid(version);
  base.place(codewords);
  const masks = mask === undefined ? [0, 1, 2, 3, 4, 5, 6, 7] : [mask];
  let best: Grid | null = null;
  let lowest = Infinity;
  for (const m of masks) {
    const grid = base.copy();
    grid.mask(m);
    grid.format(m);
    const score = penalty(grid.dark);
    if (score < lowest) [best, lowest] = [grid, score];
  }
  return best!.dark;
}

/** Per version, at level M: error correction codewords per block, then the blocks' data codewords. */
const VERSIONS: [number, number[]][] = [
  [10, [16]],
  [16, [28]],
  [26, [44]],
  [18, [32, 32]],
  [24, [43, 43]],
  [16, [27, 27, 27, 27]],
  [18, [31, 31, 31, 31]],
  [22, [38, 38, 39, 39]],
  [22, [36, 36, 36, 37, 37]],
  [26, [43, 43, 43, 43, 44]],
];

const ALIGN: number[][] = [[], [6, 18], [6, 22], [6, 26], [6, 30], [6, 34], [6, 22, 38], [6, 24, 42], [6, 26, 46], [6, 28, 50]];

const countBits = (version: number) => (version < 10 ? 8 : 16);
const dataCodewords = ([, blocks]: [number, number[]]) => blocks.reduce((a, b) => a + b, 0);
const capacity = (version: number, v: [number, number[]]) => Math.floor((dataCodewords(v) * 8 - 4 - countBits(version)) / 8);

/** Mode, count, bytes, terminator and padding, as data codewords. */
function encode(version: number, data: number[]): number[] {
  const bits: number[] = [];
  const push = (value: number, n: number) => {
    for (let i = n - 1; i >= 0; i--) bits.push((value >>> i) & 1);
  };
  push(0b0100, 4);
  push(data.length, countBits(version));
  for (const b of data) push(b, 8);
  const total = dataCodewords(VERSIONS[version - 1]) * 8;
  push(0, Math.min(4, total - bits.length));
  push(0, (8 - (bits.length % 8)) % 8);
  const out: number[] = [];
  for (let i = 0; i < bits.length; i += 8) out.push(bits.slice(i, i + 8).reduce((a, b) => (a << 1) | b, 0));
  for (let pad = 0xec; out.length < total / 8; pad ^= 0xec ^ 0x11) out.push(pad);
  return out;
}

/** The data split in blocks, each followed by its Reed-Solomon codewords, read column by column. */
function interleave(version: number, data: number[]): number[] {
  const [ecLength, sizes] = VERSIONS[version - 1];
  const divisor = generator(ecLength);
  const blocks: number[][] = [];
  const ecs: number[][] = [];
  let at = 0;
  for (const size of sizes) {
    const block = data.slice(at, at + size);
    at += size;
    blocks.push(block);
    ecs.push(remainder(block, divisor));
  }
  const out: number[] = [];
  for (let i = 0; i < Math.max(...sizes); i++) for (const b of blocks) if (i < b.length) out.push(b[i]);
  for (let i = 0; i < ecLength; i++) for (const e of ecs) out.push(e[i]);
  return out;
}

/** GF(256) with the polynomial x^8 + x^4 + x^3 + x^2 + 1. */
function multiply(a: number, b: number): number {
  let r = 0;
  for (let i = 7; i >= 0; i--) {
    r = (r << 1) ^ ((r >>> 7) * 0x11d);
    r ^= ((b >>> i) & 1) * a;
  }
  return r & 0xff;
}

/** The product of (x - α^i) for i below `degree`, highest power first, its leading 1 left out. */
function generator(degree: number): number[] {
  const result = new Array<number>(degree).fill(0);
  result[degree - 1] = 1;
  let root = 1;
  for (let i = 0; i < degree; i++) {
    for (let j = 0; j < degree; j++) {
      result[j] = multiply(result[j], root);
      if (j + 1 < degree) result[j] ^= result[j + 1];
    }
    root = multiply(root, 0x02);
  }
  return result;
}

function remainder(data: number[], divisor: number[]): number[] {
  const result = new Array<number>(divisor.length).fill(0);
  for (const b of data) {
    const factor = b ^ (result.shift() as number);
    result.push(0);
    divisor.forEach((coef, i) => (result[i] ^= multiply(coef, factor)));
  }
  return result;
}

class Grid {
  version: number;
  size: number;
  dark: boolean[][];
  fixed: boolean[][];

  constructor(version: number) {
    this.version = version;
    this.size = version * 4 + 17;
    this.dark = Array.from({ length: this.size }, () => new Array<boolean>(this.size).fill(false));
    this.fixed = Array.from({ length: this.size }, () => new Array<boolean>(this.size).fill(false));
    this.patterns();
  }

  copy(): Grid {
    const g = Object.create(Grid.prototype) as Grid;
    g.version = this.version;
    g.size = this.size;
    g.dark = this.dark.map((r) => [...r]);
    g.fixed = this.fixed;
    return g;
  }

  set(x: number, y: number, dark: boolean) {
    this.dark[y][x] = dark;
    this.fixed[y][x] = true;
  }

  patterns() {
    const n = this.size;
    for (let i = 0; i < n; i++) {
      this.set(6, i, i % 2 === 0);
      this.set(i, 6, i % 2 === 0);
    }
    for (const [cx, cy] of [[3, 3], [n - 4, 3], [3, n - 4]]) {
      for (let dy = -4; dy <= 4; dy++) {
        for (let dx = -4; dx <= 4; dx++) {
          const [x, y] = [cx + dx, cy + dy];
          const d = Math.max(Math.abs(dx), Math.abs(dy));
          if (x >= 0 && x < n && y >= 0 && y < n) this.set(x, y, d !== 2 && d !== 4);
        }
      }
    }
    const align = ALIGN[this.version - 1];
    const last = align.length - 1;
    align.forEach((cx, i) =>
      align.forEach((cy, j) => {
        if ((i === 0 && j === 0) || (i === 0 && j === last) || (i === last && j === 0)) return;
        for (let dy = -2; dy <= 2; dy++)
          for (let dx = -2; dx <= 2; dx++) this.set(cx + dx, cy + dy, Math.max(Math.abs(dx), Math.abs(dy)) !== 1);
      }),
    );
    // Reserved for the format, drawn with each mask.
    this.format(0);
    if (this.version >= 7) {
      let rem = this.version;
      for (let i = 0; i < 12; i++) rem = (rem << 1) ^ ((rem >>> 11) * 0x1f25);
      const bits = (this.version << 12) | rem;
      for (let i = 0; i < 18; i++) {
        const dark = ((bits >>> i) & 1) === 1;
        const [a, b] = [n - 11 + (i % 3), Math.floor(i / 3)];
        this.set(a, b, dark);
        this.set(b, a, dark);
      }
    }
  }

  /** Level M (00) and the mask, BCH-coded. */
  format(mask: number) {
    const n = this.size;
    let rem = mask;
    for (let i = 0; i < 10; i++) rem = (rem << 1) ^ ((rem >>> 9) * 0x537);
    const bits = ((mask << 10) | rem) ^ 0x5412;
    const bit = (i: number) => ((bits >>> i) & 1) === 1;
    for (let i = 0; i <= 5; i++) this.set(8, i, bit(i));
    this.set(8, 7, bit(6));
    this.set(8, 8, bit(7));
    this.set(7, 8, bit(8));
    for (let i = 9; i < 15; i++) this.set(14 - i, 8, bit(i));
    for (let i = 0; i < 8; i++) this.set(n - 1 - i, 8, bit(i));
    for (let i = 8; i < 15; i++) this.set(8, n - 15 + i, bit(i));
    this.set(8, n - 8, true);
  }

  /** In two-column strips from the right, up then down, skipping the timing column. */
  place(codewords: number[]) {
    const n = this.size;
    let i = 0;
    for (let right = n - 1; right >= 1; right -= 2) {
      if (right === 6) right = 5;
      for (let vert = 0; vert < n; vert++) {
        for (let j = 0; j < 2; j++) {
          const x = right - j;
          const y = ((right + 1) & 2) === 0 ? n - 1 - vert : vert;
          if (!this.fixed[y][x] && i < codewords.length * 8) {
            this.dark[y][x] = ((codewords[i >>> 3] >>> (7 - (i & 7))) & 1) === 1;
            i++;
          }
        }
      }
    }
  }

  mask(m: number) {
    const flip = [
      (x: number, y: number) => (x + y) % 2 === 0,
      (_: number, y: number) => y % 2 === 0,
      (x: number) => x % 3 === 0,
      (x: number, y: number) => (x + y) % 3 === 0,
      (x: number, y: number) => (Math.floor(x / 3) + Math.floor(y / 2)) % 2 === 0,
      (x: number, y: number) => ((x * y) % 2) + ((x * y) % 3) === 0,
      (x: number, y: number) => (((x * y) % 2) + ((x * y) % 3)) % 2 === 0,
      (x: number, y: number) => (((x + y) % 2) + ((x * y) % 3)) % 2 === 0,
    ][m];
    for (let y = 0; y < this.size; y++)
      for (let x = 0; x < this.size; x++) if (!this.fixed[y][x] && flip(x, y)) this.dark[y][x] = !this.dark[y][x];
  }
}

/** The four penalty rules of the standard, which pick the mask. */
export function penalty(dark: boolean[][]): number {
  const n = dark.length;
  let score = 0;
  const lines = [...dark, ...dark.map((_, x) => dark.map((row) => row[x]))];
  const finder = [true, false, true, true, true, false, true];
  for (const line of lines) {
    let run = 1;
    for (let i = 1; i <= n; i++) {
      if (i < n && line[i] === line[i - 1]) run++;
      else {
        if (run >= 5) score += run - 2;
        run = 1;
      }
    }
    for (let i = 0; i + 7 <= n; i++) {
      if (!finder.every((d, k) => line[i + k] === d)) continue;
      const light = (from: number) => from >= 0 && from + 4 <= n && [0, 1, 2, 3].every((k) => !line[from + k]);
      if (light(i - 4) || light(i + 7)) score += 40;
    }
  }
  for (let y = 0; y + 1 < n; y++)
    for (let x = 0; x + 1 < n; x++) {
      const c = dark[y][x];
      if (dark[y][x + 1] === c && dark[y + 1][x] === c && dark[y + 1][x + 1] === c) score += 3;
    }
  const total = n * n;
  const count = dark.reduce((a, row) => a + row.filter(Boolean).length, 0);
  score += Math.floor(Math.abs((count * 100) / total - 50) / 5) * 10;
  return score;
}

/** One SVG path drawing every dark module as a unit square, offset by the quiet zone `margin`. */
export function qrPath(rows: boolean[][], margin = 4): string {
  const parts: string[] = [];
  rows.forEach((row, y) => row.forEach((d, x) => d && parts.push(`M${x + margin} ${y + margin}h1v1h-1z`)));
  return parts.join("");
}
