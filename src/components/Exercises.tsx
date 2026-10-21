// Exercise library UI (0.22): search by Russian name, synonym or English name, a card with a
// two-frame animation (start ↔ end photo), muscles, equipment and a short how-to.
import { useEffect, useMemo, useRef, useState } from 'react';
import { api, type ResBand, type ExerciseItem, type ExerciseLibrary } from '../lib/api';

let cache: Promise<ExerciseLibrary> | null = null;
export function useExerciseLib(): ExerciseLibrary | null {
  const [lib, setLib] = useState<ExerciseLibrary | null>(null);
  useEffect(() => {
    if (!cache) cache = api.exerciseLibrary().catch(() => ({ source: '', version: '', items: [] }));
    cache.then(setLib);
  }, []);
  return lib;
}

export const MUSCLE: Record<string, string> = {
  abdominals: 'пресс', abductors: 'отводящие бедра', adductors: 'приводящие бедра', biceps: 'бицепс', calves: 'икры', chest: 'грудь',
  forearms: 'предплечья', glutes: 'ягодицы', hamstrings: 'задняя поверхность бедра', lats: 'широчайшие', 'lower back': 'поясница',
  'middle back': 'середина спины', neck: 'шея', quadriceps: 'квадрицепсы', shoulders: 'плечи', traps: 'трапеции', triceps: 'трицепс',
};
export const EQUIP: Record<string, string> = {
  barbell: 'штанга', dumbbell: 'гантели', 'body only': 'свой вес', cable: 'блок, кроссовер', machine: 'тренажёр', kettlebells: 'гири',
  bands: 'резина / эспандер', 'medicine ball': 'медбол', 'exercise ball': 'фитбол', 'foam roll': 'роллер', 'e-z curl bar': 'EZ-гриф', other: 'другое',
};
const LEVEL: Record<string, string> = { beginner: 'новичок', intermediate: 'средний', expert: 'опытный' };
const CAT: Record<string, string> = {
  strength: 'силовое', stretching: 'растяжка', plyometrics: 'плиометрика', powerlifting: 'пауэрлифтинг', 'olympic weightlifting': 'тяжёлая атлетика',
  strongman: 'стронгмен', cardio: 'кардио',
};

const norm = (s: string) => s.toLowerCase().replace(/ё/g, 'е').replace(/[^a-zа-я0-9 ]/g, ' ').replace(/\s+/g, ' ').trim();

export function searchExercises(items: ExerciseItem[], q: string, limit = 12): ExerciseItem[] {
  const n = norm(q);
  if (!n) return [];
  const words = n.split(' ');
  const scored: [number, ExerciseItem][] = [];
  for (const x of items) {
    const names = [x.ru, ...x.syn, x.en].map(norm);
    let best = 0;
    for (const [k, name] of names.entries()) {
      if (!words.every((w) => name.includes(w))) continue;
      const sc = (name === n ? 100 : name.startsWith(n) ? 60 : name.split(' ').some((t) => t.startsWith(words[0])) ? 40 : 20) - (k === names.length - 1 ? 5 : 0) - name.length / 50;
      best = Math.max(best, sc);
    }
    if (best > 0) scored.push([best, x]);
  }
  return scored.sort((a, b) => b[0] - a[0]).slice(0, limit).map((s) => s[1]);
}

/** Start ↔ end photo, switching every 0.8 s. */
export function ExerciseAnim({ ex, height = 160 }: { ex: ExerciseItem; height?: number }) {
  const [f, setF] = useState(0);
  useEffect(() => {
    if (ex.img < 2) return;
    const t = setInterval(() => setF((x) => 1 - x), 800);
    return () => clearInterval(t);
  }, [ex.img]);
  if (!ex.img) return null;
  return (
    <div className="ex-anim" style={{ maxWidth: height * 1.5, backgroundImage: `url(ex/${ex.id}.webp)`, backgroundSize: ex.img > 1 ? '200% 100%' : '100% 100%', backgroundPosition: ex.img > 1 ? (f ? '100% 0' : '0 0') : 'center' }} role="img" aria-label={ex.ru} />
  );
}

export function ExerciseCard({ ex, bodyKg, onClose }: { ex: ExerciseItem; bodyKg?: number | null; onClose: () => void }) {
  useEffect(() => {
    const k = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', k);
    return () => window.removeEventListener('keydown', k);
  }, [onClose]);
  return (
    <div className="ex-modal" role="dialog" aria-label={ex.ru} onClick={onClose}>
      <div className="card ex-modal-card" onClick={(e) => e.stopPropagation()}>
        <div className="between" style={{ alignItems: 'flex-start' }}>
          <span className="stack" style={{ gap: 2 }}>
            <span style={{ fontFamily: 'var(--display)', fontSize: 20, fontWeight: 700 }}>{ex.ru}</span>
            <span className="muted tiny">{ex.en}</span>
          </span>
          <button type="button" className="xbtn" aria-label="Закрыть" onClick={onClose}>×</button>
        </div>
        <ExerciseAnim ex={ex} height={220} />
        <span className="small" style={{ lineHeight: 1.55 }}>{ex.tip}</span>
        <div className="stack small" style={{ gap: 4 }}>
          <span><span className="muted">Основные мышцы:</span> {ex.p.map((m) => MUSCLE[m] ?? m).join(', ') || '—'}</span>
          {ex.s.length > 0 && <span><span className="muted">Помогают:</span> {ex.s.map((m) => MUSCLE[m] ?? m).join(', ')}</span>}
          <span><span className="muted">Оборудование:</span> {ex.eq ? EQUIP[ex.eq] ?? ex.eq : '—'} · {CAT[ex.cat ?? ''] ?? ex.cat} · {LEVEL[ex.lvl ?? ''] ?? ex.lvl}</span>
          {ex.bw != null && <span><span className="muted">Свой вес:</span> работает ≈{Math.round(ex.bw * 100)}% веса тела{bodyKg ? ` — ≈${Math.round(ex.bw * bodyKg)} кг при твоих ${String(bodyKg).replace('.', ',')} кг` : ''}.</span>}
          {ex.st && <span className="muted">Статическое упражнение: важно время под нагрузкой, а не килограммы.</span>}
        </div>
        <span className="muted tiny">Фото и описание — Free Exercise DB (общественное достояние), русский текст — Kouros.</span>
      </div>
    </div>
  );
}

/** Text field with suggestions from the library (and your own names). */
export function ExerciseInput({ value, exId, own, lib, onPick, onInfo }: {
  value: string | null; exId?: string | null; own: string[]; lib: ExerciseLibrary | null;
  onPick: (name: string | null, exId: string | null) => void; onInfo: (ex: ExerciseItem) => void;
}) {
  const [q, setQ] = useState(value ?? '');
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLSpanElement>(null);
  useEffect(() => setQ(value ?? ''), [value]);
  const ex = exId ? lib?.items.find((x) => x.id === exId) : undefined;
  const hits = useMemo(() => (open && lib ? searchExercises(lib.items, q, 10) : []), [open, lib, q]);
  const ownHits = useMemo(() => {
    const n = norm(q);
    return open && n ? own.filter((o) => norm(o).includes(n) && !hits.some((h) => h.ru === o)).slice(0, 4) : [];
  }, [open, own, q, hits]);
  useEffect(() => {
    if (!open) return;
    const k = (e: MouseEvent) => { if (box.current && !box.current.contains(e.target as Node)) setOpen(false); };
    document.addEventListener('mousedown', k);
    return () => document.removeEventListener('mousedown', k);
  }, [open]);
  return (
    <span ref={box} className="ex-input">
      {ex && ex.img > 0 && (
        <button type="button" className="ex-thumb" title="Как выполнять" aria-label="Как выполнять" onClick={() => onInfo(ex)} style={{ backgroundImage: `url(ex/${ex.id}.webp)` }} />
      )}
      <input
        className="input sm" aria-label="Упражнение" value={q} placeholder="найди упражнение"
        onFocus={() => setOpen(true)}
        onChange={(e) => { setQ(e.target.value); setOpen(true); }}
        onBlur={() => { if (q !== (value ?? '')) onPick(q.trim() || null, null); }}
        onKeyDown={(e) => { if (e.key === 'Enter' && hits[0]) { onPick(hits[0].ru, hits[0].id); setOpen(false); (e.target as HTMLInputElement).blur(); } }}
      />
      {open && (hits.length > 0 || ownHits.length > 0) && (
        <span className="ex-drop" role="listbox">
          {hits.map((h) => (
            <button key={h.id} type="button" className="ex-opt" onMouseDown={(e) => e.preventDefault()} onClick={() => { onPick(h.ru, h.id); setQ(h.ru); setOpen(false); }}>
              {h.img > 0 && <span className="ex-thumb sm" style={{ backgroundImage: `url(ex/${h.id}.webp)` }} />}
              <span className="stack" style={{ gap: 0, minWidth: 0 }}>
                <span className="small ellipsis">{h.ru}</span>
                <span className="muted tiny ellipsis">{h.eq ? EQUIP[h.eq] ?? h.eq : ''}{h.p.length ? ` · ${h.p.map((m) => MUSCLE[m] ?? m).join(', ')}` : ''}</span>
              </span>
            </button>
          ))}
          {ownHits.map((o) => (
            <button key={o} type="button" className="ex-opt" onMouseDown={(e) => e.preventDefault()} onClick={() => { onPick(o, null); setQ(o); setOpen(false); }}>
              <span className="small">{o}</span><span className="muted tiny">своё</span>
            </button>
          ))}
        </span>
      )}
    </span>
  );
}

/** Average pull of a band, kg (middle of the printed range). */
export const bandKg = (b: ResBand) => ((b.lo + b.hi) / 2) * (b.unit === 'lb' ? 0.4536 : 1);

/** Effective load of one repetition, mirrors exercise_svc::effective. */
export function effectiveKg(ex: ExerciseItem | undefined, weightKg: number | null, bodyKg: number | null | undefined, band: { ids: string[]; mode: 'add' | 'assist' } | null | undefined, bands: ResBand[]): number | null {
  if (ex?.st) return null;
  const bw = ex?.bw != null && bodyKg ? ex.bw * bodyKg : null;
  const ids = band?.ids ?? [];
  const bk = ids.map((id) => bands.find((b) => b.id === id)).filter((b): b is ResBand => !!b).reduce((a, b) => a + bandKg(b), 0);
  const bd = ids.length ? (band?.mode === 'assist' ? -bk : bk) : null;
  if (weightKg == null && bw == null && bd == null) return null;
  return Math.max(0, Math.round(((weightKg ?? 0) + (bw ?? 0) + (bd ?? 0)) * 10) / 10);
}
