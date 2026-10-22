// Muscle map (1.2): a detailed drawing (front and back, grey) with each muscle group tinted by how much
// load is still "in" it against its usual level (from `engine_summary.muscleMap`). Assets in
// public/muscles are built by tools/muscles/make.py: base.webp, one alpha mask per group, labels.png
// (group index + 1 per pixel) to know which group was clicked.
import { useEffect, useRef, useState, type MouseEvent } from 'react';
import { sportOf, type MuscleGroupState } from '../lib/api';

// For the athlete: in order (fresh + normal, grey), tired (loaded), overloaded (over).
const COLOR: Record<string, string> = { fresh: '#8b939e', normal: '#8b939e', loaded: '#F5B642', over: '#FF6B6B' };
// Owner's choice (1.2): fresh muscles stay grey — only what needs attention is coloured.
const ALPHA: Record<string, number> = { fresh: 0, normal: 0, loaded: 0.92, over: 0.96 };
const WORD: Record<string, string> = { fresh: 'в порядке', normal: 'в порядке', loaded: 'утомлены', over: 'перегружены' };
const LEGEND: [string, string][] = [['fresh', 'в порядке'], ['loaded', 'утомлены'], ['over', 'перегружены']];
// Same order as muscle_svc::GROUPS — the label map stores the index + 1.
const KEYS = ['calves', 'quads', 'hams', 'glutes', 'hips', 'core', 'back', 'chest', 'shoulders', 'arms', 'knees', 'ankles'];

function useLabels() {
  const [lab, setLab] = useState<{ w: number; h: number; d: Uint8ClampedArray } | null>(null);
  useEffect(() => {
    const img = new Image();
    img.onload = () => {
      try {
        const c = document.createElement('canvas'); c.width = img.width; c.height = img.height;
        const x = c.getContext('2d', { willReadFrequently: true }); if (!x) return;
        x.drawImage(img, 0, 0);
        setLab({ w: img.width, h: img.height, d: x.getImageData(0, 0, img.width, img.height).data });
      } catch { /* no picking then */ }
    };
    img.src = 'muscles/labels.png';
  }, []);
  return lab;
}

function Body({ by, sel, onPick }: { by: Record<string, MuscleGroupState>; sel: string | null; onPick: (k: string | null) => void }) {
  const lab = useLabels();
  const ref = useRef<HTMLDivElement>(null);
  const [hover, setHover] = useState<string | null>(null);
  const at = (e: MouseEvent): string | null => {
    const el = ref.current; if (!el || !lab) return null;
    const r = el.getBoundingClientRect();
    const x = Math.floor(((e.clientX - r.left) / r.width) * lab.w), y = Math.floor(((e.clientY - r.top) / r.height) * lab.h);
    if (x < 0 || y < 0 || x >= lab.w || y >= lab.h) return null;
    const v = lab.d[(y * lab.w + x) * 4];
    return v > 0 ? KEYS[v - 1] ?? null : null;
  };
  const mask = (k: string) => ({ WebkitMaskImage: `url(muscles/m_${k}.png)`, maskImage: `url(muscles/m_${k}.png)` });
  return (
    <div ref={ref} className="mm-body" onClick={(e) => onPick(at(e))} onMouseMove={(e) => setHover(at(e))} onMouseLeave={() => setHover(null)}
      style={{ cursor: hover ? 'pointer' : 'default' }} role="img" aria-label="Карта мышц спереди и сзади">
      <img src="muscles/base.webp" alt="" draggable={false} />
      {KEYS.map((k) => {
        const g = by[k];
        if (!g || !ALPHA[g.state]) return null;
        return <div key={k} className="mm-tint" style={{ ...mask(k), background: COLOR[g.state], opacity: ALPHA[g.state] }} />;
      })}
      {(hover ?? sel) && <div className="mm-hi" style={mask((hover ?? sel)!)} />}
      <span className="mm-cap" style={{ left: '25%' }}>спереди</span>
      <span className="mm-cap" style={{ left: '75%' }}>сзади</span>
    </div>
  );
}

export function MuscleMap({ groups, onOpen }: { groups: MuscleGroupState[]; onOpen?: (id: string) => void }) {
  const by = Object.fromEntries(groups.map((g) => [g.key, g]));
  const [sel, setSel] = useState<string | null>(null);
  const g = sel ? by[sel] : null;
  const hot = groups.filter((x) => x.state === 'loaded' || x.state === 'over').sort((a, b) => (b.ratio ?? 0) - (a.ratio ?? 0));
  return (
    <section className="card" aria-label="Карта мышц">
      <div className="between">
        <span className="label">Карта мышц · что ещё не восстановилось</span>
        <span className="row tiny" style={{ gap: 10 }}>
          {LEGEND.map(([k, w]) => <span key={k} className="row" style={{ gap: 4 }}><span className="dot" style={{ background: COLOR[k] }} />{w}</span>)}
        </span>
      </div>
      <div className="row" style={{ gap: 16, alignItems: 'flex-start', flexWrap: 'wrap' }}>
        <div style={{ flex: '3 1 420px', minWidth: 0 }}><Body by={by} sel={sel} onPick={setSel} /></div>
        <div className="stack" style={{ gap: 8, flex: '2 1 240px', minWidth: 0 }}>
          {g ? (
            <>
              <b>{g.name} — <span style={{ color: COLOR[g.state] }}>{WORD[g.state]}</span></b>
              <span className="muted small">{g.ratio != null ? `Сейчас ${Math.round(g.ratio * 100)}% от твоего обычного уровня` : 'Обычный уровень ещё набирается'}</span>
              {g.contrib.length ? g.contrib.map((c) => {
                const sp = sportOf(c.sport);
                return (
                  <button key={c.id} type="button" className="list-row plain small" onClick={onOpen ? () => onOpen(c.id) : undefined}>
                    <span className="badge" style={{ color: sp.color }}>{sp.letter}</span>
                    <span style={{ flex: 1, textAlign: 'left' }}>{c.name || sp.name} <span className="muted">· {new Date(c.start * 1000).toLocaleDateString('ru-RU', { weekday: 'short', day: 'numeric', month: 'short' })}</span></span>
                    <span className="num">{c.share}%</span>
                  </button>
                );
              }) : <span className="muted small">За последние дни эту группу почти не нагружал.</span>}
            </>
          ) : (
            <>
              <span className="small">{hot.length ? `Больше всего нагружены: ${hot.slice(0, 3).map((x) => x.name.toLowerCase()).join(', ')}.` : 'Ничего не перегружено.'}</span>
              <span className="muted small">Нажми на мышцу на рисунке — покажу, какие тренировки её нагрузили.</span>
            </>
          )}
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>Оценка по тренировкам: дистанция и темп, подъёмы и особенно спуски (квадрицепсы и колени), упражнения в подходах. Нагрузка постепенно «выветривается» — примерно вдвое за сутки. Мышцы часы не измеряют, так что это ориентир, а не прибор.</span>
        </div>
      </div>
    </section>
  );
}
