// Route of a workout: OpenStreetMap underneath (when switched on in Settings),
// the track coloured by pulse zone, and a cursor shared with the charts.
import { useCallback, useEffect, useMemo, useRef, useState, type PointerEvent as RPointerEvent } from 'react';

const TS = 256;
const BASE = typeof navigator !== 'undefined' && /Windows/.test(navigator.userAgent) ? 'http://kmap.localhost' : 'kmap://localhost';
const ZONE_COLORS = ['#7FD3FF', '#3DDC97', '#F5B642', '#FF8A65', '#FF6B6B'];

function zoneOf(hr: number | null | undefined, z: number[]): number {
  if (hr == null) return -1;
  for (let i = 4; i >= 0; i--) if (hr >= z[i]) return i;
  return 0;
}

/** Web Mercator at zoom 0, in pixels of a 256 px world. */
function project(lat: number, lng: number): [number, number] {
  const s = Math.sin((lat * Math.PI) / 180);
  return [((lng + 180) / 360) * TS, (0.5 - Math.log((1 + s) / (1 - s)) / (4 * Math.PI)) * TS];
}

type Pt = { i: number; x: number; y: number; zone: number };

/** Consecutive points of one zone become one polyline. */
function segments(pts: Pt[], px: (p: Pt) => [number, number]) {
  const out: { color: string; points: string }[] = [];
  let cur: { zone: number; pts: string[] } | null = null;
  for (const p of pts) {
    const [x, y] = px(p);
    const s = `${x.toFixed(1)},${y.toFixed(1)}`;
    if (!cur || cur.zone !== p.zone) {
      if (cur) { cur.pts.push(s); out.push({ color: cur.zone < 0 ? '#3DDC97' : ZONE_COLORS[cur.zone], points: cur.pts.join(' ') }); }
      cur = { zone: p.zone, pts: [s] };
    } else cur.pts.push(s);
  }
  if (cur) out.push({ color: cur.zone < 0 ? '#3DDC97' : ZONE_COLORS[cur.zone], points: cur.pts.join(' ') });
  return out;
}

export function RouteMap({ lat, lng, hr, zones, cur, onCur, online }: {
  lat: (number | null)[]; lng: (number | null)[]; hr: (number | null)[]; zones: number[];
  cur: number | null; onCur: (i: number | null) => void; online: boolean;
}) {
  const pts0: Pt[] = useMemo(() => {
    const out: Pt[] = [];
    lat.forEach((a, i) => {
      const b = lng[i];
      if (a == null || b == null) return;
      const [x, y] = project(a, b);
      out.push({ i, x, y, zone: zoneOf(hr[i], zones) });
    });
    return out;
  }, [lat, lng, hr, zones]);

  const box = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(800);
  const H = 400;
  const [view, setView] = useState<{ z: number; cx: number; cy: number } | null>(null);
  const [fail, setFail] = useState(0);
  const [ok, setOk] = useState(0);
  const drag = useRef<{ x: number; y: number; moved: boolean } | null>(null);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setW(el.offsetWidth || 800));
    ro.observe(el);
    setW(el.offsetWidth || 800);
    return () => ro.disconnect();
  }, []);

  const fit = useCallback(() => {
    if (!pts0.length) return;
    const xs = pts0.map((p) => p.x), ys = pts0.map((p) => p.y);
    const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    let z = 17;
    while (z > 2 && ((x1 - x0) * 2 ** z > w - 60 || (y1 - y0) * 2 ** z > H - 60)) z--;
    setView({ z, cx: ((x0 + x1) / 2) * 2 ** z, cy: ((y0 + y1) / 2) * 2 ** z });
  }, [pts0, w]);
  useEffect(() => { fit(); }, [fit]);

  // The wheel zooms the map (Ctrl + wheel stays the app-wide scale).
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const h = (e: WheelEvent) => {
      if (e.ctrlKey || !view) return;
      e.preventDefault();
      const r = el.getBoundingClientRect(), k = r.width / el.offsetWidth || 1;
      const mx = (e.clientX - r.left) / k, my = (e.clientY - r.top) / k;
      const dz = e.deltaY < 0 ? 1 : -1;
      const z = Math.max(3, Math.min(18, view.z + dz));
      if (z === view.z) return;
      const f = 2 ** (z - view.z);
      const wx = view.cx - w / 2 + mx, wy = view.cy - H / 2 + my;
      setView({ z, cx: wx * f - mx + w / 2, cy: wy * f - my + H / 2 });
    };
    el.addEventListener('wheel', h, { passive: false });
    return () => el.removeEventListener('wheel', h);
  }, [view, w]);

  if (!pts0.length || !view) return <div ref={box} style={{ height: 40 }} />;

  const scale = 2 ** view.z;
  const ox = view.cx - w / 2, oy = view.cy - H / 2;
  const px = (p: Pt): [number, number] => [p.x * scale - ox, p.y * scale - oy];
  const segs = segments(pts0, px);
  const curPt = cur == null ? null : pts0.reduce<Pt | null>((b, p) => (b == null || Math.abs(p.i - cur) < Math.abs(b.i - cur) ? p : b), null);

  const tiles: { key: string; x: number; y: number; src: string }[] = [];
  if (online) {
    const n = 2 ** view.z;
    for (let tx = Math.floor(ox / TS); tx <= Math.floor((ox + w) / TS); tx++) {
      for (let ty = Math.floor(oy / TS); ty <= Math.floor((oy + H) / TS); ty++) {
        if (ty < 0 || ty >= n) continue;
        const wx = ((tx % n) + n) % n;
        tiles.push({ key: `${view.z}/${tx}/${ty}`, x: tx * TS - ox, y: ty * TS - oy, src: `${BASE}/${view.z}/${wx}/${ty}.png` });
      }
    }
  }

  function local(e: RPointerEvent): [number, number] {
    const el = box.current!;
    const r = el.getBoundingClientRect(), k = r.width / el.offsetWidth || 1;
    return [(e.clientX - r.left) / k, (e.clientY - r.top) / k];
  }
  function onMove(e: RPointerEvent) {
    if (drag.current && view) {
      const k = box.current ? box.current.getBoundingClientRect().width / box.current.offsetWidth || 1 : 1;
      const dx = (e.clientX - drag.current.x) / k, dy = (e.clientY - drag.current.y) / k;
      if (Math.abs(dx) + Math.abs(dy) > 2) drag.current.moved = true;
      drag.current.x = e.clientX; drag.current.y = e.clientY;
      setView({ ...view, cx: view.cx - dx, cy: view.cy - dy });
      return;
    }
    const [mx, my] = local(e);
    let best: Pt | null = null, bd = 24 * 24;
    for (const p of pts0) {
      const [x, y] = px(p);
      const d = (x - mx) ** 2 + (y - my) ** 2;
      if (d < bd) { bd = d; best = p; }
    }
    if (best) onCur(best.i);
  }

  const [sx, sy] = px(pts0[0]);
  const [ex, ey] = px(pts0[pts0.length - 1]);
  return (
    <div ref={box} className="map" style={{ height: H, cursor: drag.current ? 'grabbing' : 'grab' }}
      onPointerDown={(e) => { drag.current = { x: e.clientX, y: e.clientY, moved: false }; (e.target as Element).setPointerCapture?.(e.pointerId); }}
      onPointerUp={() => { drag.current = null; }}
      onPointerMove={onMove}
      onPointerLeave={() => { if (!drag.current) onCur(null); }}>
      {online && (
        <div className="map-tiles" aria-hidden="true">
          {tiles.map((t) => (
            <img key={t.key} src={t.src} alt="" draggable={false} width={TS} height={TS} style={{ left: t.x, top: t.y }}
              onLoad={() => setOk((v) => v + 1)} onError={(e) => { (e.target as HTMLImageElement).style.visibility = 'hidden'; setFail((v) => v + 1); }} />
          ))}
        </div>
      )}
      <svg width={w} height={H} style={{ position: 'absolute', inset: 0 }} aria-label="Маршрут">
        {segs.map((s, i) => <polyline key={i} points={s.points} fill="none" stroke="#0b0d10" strokeOpacity="0.55" strokeWidth="7" strokeLinejoin="round" strokeLinecap="round" />)}
        {segs.map((s, i) => <polyline key={`c${i}`} points={s.points} fill="none" stroke={s.color} strokeWidth="4" strokeLinejoin="round" strokeLinecap="round" />)}
        <circle cx={sx} cy={sy} r="7" fill="#3DDC97" stroke="#0b0d10" strokeWidth="2" />
        <circle cx={ex} cy={ey} r="7" fill="#0b0d10" stroke="#f1f3f5" strokeWidth="2.5" />
        {curPt && (() => { const [x, y] = px(curPt); return <circle cx={x} cy={y} r="8" fill="#f1f3f5" stroke="#0b0d10" strokeWidth="3" />; })()}
      </svg>
      <div className="map-ctl">
        <button type="button" aria-label="Приблизить" onClick={() => setView({ z: Math.min(18, view.z + 1), cx: view.cx * (view.z < 18 ? 2 : 1), cy: view.cy * (view.z < 18 ? 2 : 1) })}>+</button>
        <button type="button" aria-label="Отдалить" onClick={() => setView({ z: Math.max(3, view.z - 1), cx: view.cx / (view.z > 3 ? 2 : 1), cy: view.cy / (view.z > 3 ? 2 : 1) })}>−</button>
        <button type="button" aria-label="Весь маршрут" onClick={fit}>⤢</button>
      </div>
      {online && <span className="map-attr">© OpenStreetMap</span>}
      {online && fail > 0 && ok === 0 && <span className="map-note">Карта не загрузилась — проверь интернет. Трек показан без подложки.</span>}
    </div>
  );
}
