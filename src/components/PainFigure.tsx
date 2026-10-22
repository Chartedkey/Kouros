// Body figure for marking pain (1.3.1): the friend's drawing used by the muscle map, cut into pain
// areas with left and right sides (public/muscles/pain.png, built by tools/muscles/pain.py; value =
// index + 1 in KEYS). Areas with pain are tinted on a canvas (colour blend keeps the drawing visible).
import { useEffect, useMemo, useRef, useState, type MouseEvent } from 'react';

// Same order as tools/muscles/pain.py.
const KEYS = ['back:', 'core:', 'chest:R', 'chest:L', 'shoulders:R', 'shoulders:L', 'arms:R', 'arms:L', 'neck:', 'glutes:R', 'glutes:L', 'hams:R', 'hams:L',
  'quads:R', 'quads:L', 'hips:R', 'hips:L', 'calves:R', 'calves:L', 'shin:R', 'shin:L', 'knees:R', 'knees:L', 'ankles:R', 'ankles:L', 'foot:R', 'foot:L'];

const BASE = '#1E232A';
export function painColor(level: number | undefined): string {
  if (level == null || level <= 0) return BASE;
  if (level <= 3) return '#D8C46A';
  if (level <= 6) return '#F5B642';
  return '#FF6B6B';
}
const rgb = (hex: string) => [parseInt(hex.slice(1, 3), 16), parseInt(hex.slice(3, 5), 16), parseInt(hex.slice(5, 7), 16)];

type Img = { w: number; h: number; base: HTMLImageElement; lab: Uint8ClampedArray };
let cache: Promise<Img> | null = null;
function loadAssets(): Promise<Img> {
  if (cache) return cache;
  const load = (src: string) => new Promise<HTMLImageElement>((ok, bad) => { const i = new Image(); i.onload = () => ok(i); i.onerror = bad; i.src = src; });
  cache = Promise.all([load('muscles/base.webp'), load('muscles/pain.png')]).then(([base, lab]) => {
    const c = document.createElement('canvas'); c.width = lab.width; c.height = lab.height;
    const x = c.getContext('2d', { willReadFrequently: true })!;
    x.drawImage(lab, 0, 0);
    return { w: lab.width, h: lab.height, base, lab: x.getImageData(0, 0, lab.width, lab.height).data };
  });
  cache.catch(() => { cache = null; });
  return cache;
}

/** `levels` keys are `area:side` (side '' = middle / both). */
export function PainFigure({ levels, sel, onPick }: { levels: Record<string, number>; sel?: string | null; onPick?: (area: string, side: string) => void }) {
  const [img, setImg] = useState<Img | null>(null);
  const [hover, setHover] = useState<string | null>(null);
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => { let on = true; loadAssets().then((i) => { if (on) setImg(i); }).catch(() => {}); return () => { on = false; }; }, []);
  // Level of each label: its own side, or the whole area marked without a side.
  const lv = useMemo(() => KEYS.map((k) => {
    const [a] = k.split(':');
    return Math.max(levels[k] ?? 0, levels[`${a}:`] ?? 0, k.endsWith(':') ? Math.max(levels[`${a}:L`] ?? 0, levels[`${a}:R`] ?? 0) : 0);
  }), [levels]);

  useEffect(() => {
    const c = ref.current;
    if (!img || !c) return;
    c.width = img.w; c.height = img.h;
    const x = c.getContext('2d')!;
    x.clearRect(0, 0, img.w, img.h);
    x.drawImage(img.base, 0, 0, img.w, img.h);
    const hi = hover ?? sel ?? null;
    const hiIdx = hi ? KEYS.indexOf(hi) + 1 : 0;
    const tint = x.createImageData(img.w, img.h);
    const glow = x.createImageData(img.w, img.h);
    const cols = lv.map((l) => (l > 0 ? rgb(painColor(l)) : null));
    let anyTint = false, anyGlow = false;
    for (let p = 0, q = 0; p < img.lab.length; p += 4, q += 4) {
      const v = img.lab[p];
      if (!v) continue;
      const col = cols[v - 1];
      if (col) { tint.data[q] = col[0]; tint.data[q + 1] = col[1]; tint.data[q + 2] = col[2]; tint.data[q + 3] = 235; anyTint = true; }
      if (v === hiIdx) { glow.data[q] = 255; glow.data[q + 1] = 255; glow.data[q + 2] = 255; glow.data[q + 3] = 70; anyGlow = true; }
    }
    const layer = (d: ImageData, mode: GlobalCompositeOperation) => {
      const t = document.createElement('canvas'); t.width = img.w; t.height = img.h;
      t.getContext('2d')!.putImageData(d, 0, 0);
      x.globalCompositeOperation = mode;
      x.drawImage(t, 0, 0);
      x.globalCompositeOperation = 'source-over';
    };
    if (anyTint) layer(tint, 'color');
    if (anyGlow) layer(glow, 'soft-light');
    // The colour blend only works over the drawing: restore its transparent background.
    x.globalCompositeOperation = 'destination-in';
    x.drawImage(img.base, 0, 0, img.w, img.h);
    x.globalCompositeOperation = 'source-over';
  }, [img, lv, hover, sel]);

  const at = (e: MouseEvent): string | null => {
    const c = ref.current; if (!c || !img) return null;
    const r = c.getBoundingClientRect();
    const px = Math.floor(((e.clientX - r.left) / r.width) * img.w), py = Math.floor(((e.clientY - r.top) / r.height) * img.h);
    if (px < 0 || py < 0 || px >= img.w || py >= img.h) return null;
    const v = img.lab[(py * img.w + px) * 4];
    return v ? KEYS[v - 1] ?? null : null;
  };
  return (
    <div className="pain-fig" role="img" aria-label="Фигура спереди и сзади: нажми, где болит">
      <canvas ref={ref} style={{ cursor: hover && onPick ? 'pointer' : 'default' }}
        onMouseMove={(e) => setHover(onPick ? at(e) : null)} onMouseLeave={() => setHover(null)}
        onClick={(e) => { const k = at(e); if (k && onPick) { const [a, s] = k.split(':'); onPick(a, s); } }} />
      {!img && <span className="muted tiny">Загружаю рисунок…</span>}
      <span className="mm-cap" style={{ left: '25%' }}>спереди</span>
      <span className="mm-cap" style={{ left: '75%' }}>сзади</span>
    </div>
  );
}

export const sideText = (s: string) => (s === 'L' ? 'слева' : s === 'R' ? 'справа' : '');
