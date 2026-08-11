// 点阵思考球引擎（复刻 thinking-orbs / MIT，HalftoneSphere 谱系）：
// 真 3D 旋转 + 深度点径/墨量表达，纯 2D canvas fill，Chrome/Safari/Firefox 一致。
// kxen 四态：thinking(orbits) / searching(globe) / composing(ribbon) / error(静态方框)。

export type OrbState = "thinking" | "searching" | "composing" | "error";
export type OrbSize = 64 | 20;

interface Dot {
  x: number;
  y: number;
  z: number;
  r: number;
  white: number;
  a?: number;
}

type Projector = (x: number, y: number, z: number) => [number, number, number];
type ModeDraw = (ctx: CanvasRenderingContext2D, size: number, t: number, dark: boolean) => void;

function hashD(a: number, b: number): number {
  const h = Math.sin(a * 12.9898 + b * 78.233) * 43758.5453;
  return h - Math.floor(h);
}

function makeProj(yaw: number, tilt: number, cx: number, cy: number, scale: number): Projector {
  const st = Math.sin(tilt);
  const ct = Math.cos(tilt);
  const sy = Math.sin(yaw);
  const cyw = Math.cos(yaw);
  return (x, y, z) => {
    const x1 = x * cyw + z * sy;
    const z1 = -x * sy + z * cyw;
    const y1 = y * ct - z1 * st;
    const z2 = y * st + z1 * ct;
    return [cx + x1 * scale, cy - y1 * scale, z2];
  };
}

function angleDelta(a: number, b: number): number {
  return Math.atan2(Math.sin(a - b), Math.cos(a - b));
}

function paint(ctx: CanvasRenderingContext2D, dots: Dot[], dark: boolean, rMin = 0.3): void {
  dots.sort((a, b) => a.z - b.z);
  for (const d of dots) {
    const alpha = d.a ?? 1;
    if (alpha < 0.02) continue;
    const w = Math.min(1, Math.max(0, d.white));
    const g = Math.round((dark ? 1 - w : w) * 255);
    ctx.fillStyle = `rgba(${g},${g},${g},${alpha})`;
    ctx.beginPath();
    ctx.arc(d.x, d.y, Math.max(rMin, d.r), 0, Math.PI * 2);
    ctx.fill();
  }
}

function radiusScale(size: number, pow: number): number {
  return (size / 300) ** pow;
}

// --- thinking：倾斜轨道上的粒子（orbits 工作态，无核） ---

function drawOrbits(ctx: CanvasRenderingContext2D, size: number, t: number, dark: boolean): void {
  const cx = size / 2;
  const cy = size / 2;
  const R = (size / 2) * 0.82;
  const pt = makeProj(t * 0.12, 0.3, cx, cy, 1);
  const rs = radiusScale(size, 0.6);
  const dots: Dot[] = [];
  const orbitN = size === 64 ? 12 : 3;
  const ghostN = size === 64 ? 40 : 10;
  const partScale = size === 64 ? 1 : 2.4;

  for (let orb = 0; orb < orbitN; orb++) {
    const h1 = hashD(orb, 1.7);
    const h2 = hashD(orb, 5.2);
    const h3 = hashD(orb, 8.9);
    const ro = R * (0.45 + 0.52 * h1);
    const th = h1 * 2 * Math.PI;
    const phi = Math.acos(2 * h2 - 1);
    const nx = Math.sin(phi) * Math.cos(th);
    const ny = Math.cos(phi);
    const nz = Math.sin(phi) * Math.sin(th);
    let ux = -ny;
    let uy = nx;
    const ul = Math.max(1e-6, Math.sqrt(ux * ux + uy * uy));
    ux /= ul;
    uy /= ul;
    const vx = -nz * uy;
    const vy = nz * ux;
    const vz = nx * uy - ny * ux;
    const speed = (0.25 + 0.55 * h3) * (h3 > 0.5 ? 1 : -1);

    for (let k = 0; k < ghostN; k++) {
      const a = (k / ghostN) * 2 * Math.PI;
      const [px, py, z] = pt(
        (ux * Math.cos(a) + vx * Math.sin(a)) * ro,
        (uy * Math.cos(a) + vy * Math.sin(a)) * ro,
        vz * Math.sin(a) * ro,
      );
      const depth = (z / ro + 1) / 2;
      dots.push({ x: px, y: py, z, r: 0.9 * rs, white: 0.72, a: 0.5 * (0.4 + 0.6 * depth) });
    }
    for (let m = 0; m < 3; m++) {
      const a = t * speed + (m / 3) * 2 * Math.PI + h2 * 6;
      const [px, py, z] = pt(
        (ux * Math.cos(a) + vx * Math.sin(a)) * ro,
        (uy * Math.cos(a) + vy * Math.sin(a)) * ro,
        vz * Math.sin(a) * ro,
      );
      const depth = (z / ro + 1) / 2;
      dots.push({
        x: px,
        y: py,
        z,
        r: (1.2 + 1.6 * depth) * rs * partScale,
        white: 0.3 - 0.22 * depth,
      });
    }
  }
  paint(ctx, dots, dark);
}

// --- searching：扫描子午线扫过点阵球（globe） ---

function drawGlobe(ctx: CanvasRenderingContext2D, size: number, t: number, dark: boolean): void {
  const spin = 0.5;
  const cx = size / 2;
  const cy = size / 2;
  const radius = (size / 2) * 0.82;
  const tilt = 0.4 + 0.06 * Math.sin(t * 0.35);
  const pt = makeProj(t * spin, tilt, cx, cy, radius);
  const scan = t * (spin + (1.7 - spin) * (size === 64 ? 4.08 : 4.335));
  const rs = radiusScale(size, 0.6);
  const rScale = size === 64 ? 1.15 : 1.75;
  const latRings = size === 64 ? 8 : 4;
  const lonDensity = size === 64 ? 19 : 9;

  const dots: Dot[] = [];
  for (let li = 0; li <= latRings; li++) {
    const lat = -Math.PI / 2 + (li / latRings) * Math.PI;
    const cosLat = Math.cos(lat);
    const sinLat = Math.sin(lat);
    const lonCount = Math.max(1, Math.round(Math.abs(cosLat) * lonDensity));
    for (let lj = 0; lj < lonCount; lj++) {
      const lon = (lj / lonCount) * 2 * Math.PI;
      const [px, py, z] = pt(cosLat * Math.cos(lon), sinLat, cosLat * Math.sin(lon));
      const depth = (z + 1) / 2;
      const d = angleDelta(lon + t * spin, scan);
      const boost = Math.exp(-(d * d) / 0.18) * Math.max(0, z);
      dots.push({
        x: px,
        y: py,
        z,
        r: (0.6 + 1.7 * depth + 1.0 * boost) * rs * rScale,
        white: 0.62 - 0.54 * depth,
        a: 0.45 + 0.55 * Math.min(1, boost),
      });
    }
  }
  paint(ctx, dots, dark);
}

// --- composing：多股波浪绶带（ribbon，冻结 3D 翻滚） ---

function fibDir(i: number, n: number): [number, number, number] {
  const golden = Math.PI * (3 - Math.sqrt(5));
  const y = 1 - (2 * (i + 0.5)) / n;
  const rad = Math.sqrt(1 - y * y);
  const a = i * golden;
  return [rad * Math.cos(a), y, rad * Math.sin(a)];
}

function drawRibbon(ctx: CanvasRenderingContext2D, size: number, t: number, dark: boolean): void {
  const cx = size / 2;
  const cy = size / 2;
  const R = (size / 2) * 0.78;
  const pt = makeProj(0, 0.3, cx, cy, 1);
  const rs = radiusScale(size, 0.6);
  const ghostN = size === 64 ? 38 : 8;
  const lanes = size === 64 ? 20 : 5;
  const segs = size === 64 ? 22 : 10;
  const rScale = size === 64 ? 0.85 : 1.073;

  const dots: Dot[] = [];
  for (let i = 0; i < ghostN; i++) {
    const d = fibDir(i, ghostN);
    const [px, py, z] = pt(d[0] * R, d[1] * R, d[2] * R);
    const depth = (z / R + 1) / 2;
    dots.push({ x: px, y: py, z, r: 0.8 * rs, white: 0.78, a: 0.1 + 0.22 * depth });
  }

  const ta = 0.55;
  const ux = 1;
  const uy = 0;
  const uz = 0;
  const vx = 0;
  const vy = Math.cos(ta);
  const vz = Math.sin(ta);
  const nx = uy * vz - uz * vy;
  const ny = uz * vx - ux * vz;
  const nz = ux * vy - uy * vx;

  for (let w = 0; w < lanes; w++) {
    const laneOff = (w - (lanes - 1) / 2) * 0.075;
    const edge = Math.abs(w - (lanes - 1) / 2) / Math.max(1, (lanes - 1) / 2);
    for (let k = 0; k < segs; k++) {
      const a = (k / segs) * 2 * Math.PI;
      const wob = 0.16 * Math.sin(a * 3 - t * 1.7 + w * 0.22) + 0.07 * Math.sin(a * 5 + t * 1.1);
      const off = laneOff + wob;
      const x = ux * Math.cos(a) + vx * Math.sin(a) + nx * off;
      const y = uy * Math.cos(a) + vy * Math.sin(a) + ny * off;
      const z = uz * Math.cos(a) + vz * Math.sin(a) + nz * off;
      const l = Math.sqrt(x * x + y * y + z * z);
      const [px, py, zr] = pt((x / l) * R, (y / l) * R, (z / l) * R);
      const depth = (zr / R + 1) / 2;
      dots.push({
        x: px,
        y: py,
        z: zr,
        r: (1.1 + 1.7 * depth) * (1 - 0.25 * edge) * rs * rScale,
        white: 0.52 - 0.44 * depth + 0.18 * edge,
        a: 0.4 + 0.6 * depth,
      });
    }
  }
  paint(ctx, dots, dark);
}

// --- error：静态点阵方框（无动画，明确的停止语言） ---

function drawError(ctx: CanvasRenderingContext2D, size: number, _t: number, dark: boolean): void {
  const cx = size / 2;
  const cy = size / 2;
  const half = (size / 2) * 0.62;
  const n = size === 64 ? 9 : 5;
  const r = size === 64 ? 1.5 : 1.3;
  const dots: Dot[] = [];
  for (let i = 0; i < n; i++) {
    const p = -half + (i / (n - 1)) * 2 * half;
    dots.push({ x: cx + p, y: cy - half, z: 0, r, white: 0.35 });
    dots.push({ x: cx + p, y: cy + half, z: 0, r, white: 0.35 });
    dots.push({ x: cx - half, y: cy + p, z: 0, r, white: 0.35 });
    dots.push({ x: cx + half, y: cy + p, z: 0, r, white: 0.35 });
  }
  // 中心一点：错误焦点
  dots.push({ x: cx, y: cy, z: 1, r: r * 1.6, white: 0.2 });
  paint(ctx, dots, dark);
}

const DRAW: Record<OrbState, ModeDraw> = {
  thinking: drawOrbits,
  searching: drawGlobe,
  composing: drawRibbon,
  error: drawError,
};

/** 速度调参（thinking-orbs presets）。 */
const SPEED: Record<OrbState, Record<OrbSize, number>> = {
  thinking: { 64: 1.885, 20: 3.9 },
  searching: { 64: 2.015, 20: 2.665 },
  composing: { 64: 2.34, 20: 3.12 },
  error: { 64: 0, 20: 0 },
};

/** 画一帧（t 为共享时钟秒数；dark 决定墨量镜像）。 */
export function drawOrbFrame(
  ctx: CanvasRenderingContext2D,
  state: OrbState,
  size: OrbSize,
  t: number,
  dark: boolean,
): void {
  ctx.clearRect(0, 0, size, size);
  DRAW[state](ctx, size, t * SPEED[state][size], dark);
}

export const ORB_ARIA: Record<OrbState, string> = {
  thinking: "正在思考",
  searching: "正在检索",
  composing: "正在生成回复",
  error: "出错了",
};
