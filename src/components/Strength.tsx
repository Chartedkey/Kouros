// 1.4.3: the «Силовая» card of a workout — sets grouped by exercise (name once, picture, muscles,
// band for the whole exercise), a picker with filters by muscles / equipment / type, a compact
// band button, and one-sided exercises («на каждую руку / ногу»: reps for one side, work counted
// for both; «только одна сторона» per set). Rows are still plain `strength_sets` rows.
import { useEffect, useMemo, useRef, useState } from 'react';
import { api, errText, fmtDur, type ActivityDetail, type ExerciseItem, type ExerciseLibrary, type ResBand, type SetRow } from '../lib/api';
import { EQUIP, ExerciseCard, MUSCLE, bandKg, effectiveKg, searchExercises, useExerciseLib } from './Exercises';

// ---------- facets ----------

export const MUSCLE_GROUPS: [string, string, string[]][] = [
  ['quads', 'Перед бедра', ['quadriceps']], ['hams', 'Задняя поверхность', ['hamstrings']], ['glutes', 'Ягодицы и бёдра', ['glutes', 'abductors', 'adductors']],
  ['calves', 'Икры и стопы', ['calves']], ['back', 'Спина', ['lats', 'middle back', 'lower back', 'traps']], ['chest', 'Грудь', ['chest']],
  ['shoulders', 'Плечи', ['shoulders', 'neck']], ['arms', 'Руки', ['biceps', 'triceps', 'forearms']], ['core', 'Кор', ['abdominals']],
];
export const EQUIP_GROUPS: [string, string][] = [
  ['body', 'Свой вес'], ['bands', 'Резина'], ['mini', 'Мини-резинка'], ['dumbbell', 'Гантели'], ['barbell', 'Штанга'], ['kb', 'Гиря'], ['machine', 'Тренажёр / блок'],
];
const TYPES: [string, string][] = [['strength', 'Силовое'], ['ofp', 'ОФП для бега'], ['stretch', 'Растяжка']];

const eqOf = (x: ExerciseItem, lib: ExerciseLibrary | null): string => {
  if (x.id.startsWith('k_mb_')) return 'mini';
  if (x.eq === 'bands' || /^k_(lb|fb|rb)_/.test(x.id) || ((lib?.bandList ?? []).includes(x.id) && x.en.toLowerCase().includes('band'))) return 'bands';
  if (x.eq === 'body only' || x.own) return 'body';
  if (x.eq === 'dumbbell') return 'dumbbell';
  if (x.eq === 'barbell' || x.eq === 'e-z curl bar') return 'barbell';
  if (x.eq === 'kettlebells') return 'kb';
  if (x.eq === 'machine' || x.eq === 'cable') return 'machine';
  return 'other';
};
export const typeOf = (x: ExerciseItem, lib: ExerciseLibrary | null): string => {
  if ((lib?.runnerOfp ?? []).includes(x.id) || x.id.startsWith('k_rb_') || x.id.startsWith('k_fb_')) return 'ofp';
  if (x.cat === 'stretching') return 'stretch';
  return 'strength';
};
export const sideWord = (x?: ExerciseItem | null) => (x?.side === 'arm' ? 'на каждую руку' : x?.side === 'leg' ? 'на каждую ногу' : x?.side ? 'на каждую сторону' : null);
const sideShort = (x?: ExerciseItem | null) => (x?.side === 'arm' ? 'рука' : x?.side === 'leg' ? 'нога' : 'сторона');
const musclesText = (x: ExerciseItem) => x.p.map((m) => MUSCLE[m] ?? m).join(', ');

let recentCache: Promise<string[]> | null = null;
export function useRecentExercises(): string[] {
  const [r, setR] = useState<string[]>([]);
  useEffect(() => {
    if (!recentCache) recentCache = api.exercisesRecent().catch(() => []);
    recentCache.then(setR);
  }, []);
  return r;
}
export const forgetRecent = () => { recentCache = null; };

// ---------- picker ----------

/** Window to choose an exercise: search + filters (muscles, equipment, type), «Недавние», «Мои». */
export function ExercisePicker({ lib, mode, onPick, onClose }: {
  lib: ExerciseLibrary | null; mode: 'strength' | 'ofp'; onPick: (ex: ExerciseItem | null, name: string) => void; onClose: () => void;
}) {
  const [q, setQ] = useState('');
  const [mus, setMus] = useState<string[]>([]);
  const [eq, setEq] = useState<string[]>([]);
  const [ty, setTy] = useState<string[]>([mode]);
  const [own, setOwn] = useState(false);
  const [more, setMore] = useState(1);
  const [info, setInfo] = useState<ExerciseItem | null>(null);
  const recent = useRecentExercises();
  useEffect(() => {
    const k = (e: KeyboardEvent) => { if (e.key === 'Escape' && !info) onClose(); };
    window.addEventListener('keydown', k);
    return () => window.removeEventListener('keydown', k);
  }, [onClose, info]);
  const items = lib?.items ?? [];
  const hits = useMemo(() => {
    const base = q.trim() ? searchExercises(items, q, 400) : items;
    const mset = MUSCLE_GROUPS.filter((g) => mus.includes(g[0])).flatMap((g) => g[2]);
    return base.filter((x) =>
      (!own || x.own)
      && (!mus.length || x.p.some((m) => mset.includes(m)))
      && (!eq.length || eq.includes(eqOf(x, lib)))
      && (!ty.length || ty.includes(typeOf(x, lib))));
  }, [items, q, mus, eq, ty, own, lib]);
  const recentItems = recent.map((id) => items.find((x) => x.id === id)).filter((x): x is ExerciseItem => !!x).slice(0, 8);
  const tog = (arr: string[], set: (v: string[]) => void, k: string) => { set(arr.includes(k) ? arr.filter((x) => x !== k) : [...arr, k]); setMore(1); };
  const shown = hits.slice(0, 36 * more);
  const anyFilter = mus.length || eq.length || own || ty.join() !== mode;
  return (
    <div className="ex-modal" role="dialog" aria-label="Выбери упражнение" onClick={onClose}>
      <div className="card ex-picker" onClick={(e) => e.stopPropagation()}>
        <div className="between">
          <span style={{ fontFamily: 'var(--display)', fontSize: 20, fontWeight: 700 }}>Выбери упражнение</span>
          <button type="button" className="xbtn" aria-label="Закрыть" onClick={onClose}>×</button>
        </div>
        <input className="input" autoFocus placeholder="Поиск: присед, тяга, ротация…" value={q} onChange={(e) => { setQ(e.target.value); setMore(1); }} aria-label="Поиск упражнения" />
        <div className="picker-facets">
          <span className="picker-h">Мышцы</span>
          <div className="row" style={{ gap: 6 }}>{MUSCLE_GROUPS.map(([k, n]) => <button key={k} type="button" className="pill sm" aria-pressed={mus.includes(k)} onClick={() => tog(mus, setMus, k)}>{n}</button>)}</div>
          <span className="picker-h">Инвентарь</span>
          <div className="row" style={{ gap: 6 }}>{EQUIP_GROUPS.map(([k, n]) => <button key={k} type="button" className="pill sm" aria-pressed={eq.includes(k)} onClick={() => tog(eq, setEq, k)}>{n}</button>)}</div>
          <span className="picker-h">Тип</span>
          <div className="row" style={{ gap: 6, alignItems: 'center' }}>
            {TYPES.map(([k, n]) => <button key={k} type="button" className="pill sm" aria-pressed={ty.includes(k)} onClick={() => tog(ty, setTy, k)}>{n}</button>)}
            <button type="button" className="pill sm" aria-pressed={own} onClick={() => { setOwn(!own); setMore(1); }}>Мои</button>
            <span className="muted tiny" style={{ marginLeft: 'auto' }}>
              найдено {hits.length}{anyFilter ? <> · <button type="button" className="linkbtn tiny" onClick={() => { setMus([]); setEq([]); setTy([mode]); setOwn(false); }}>сбросить фильтры</button></> : null}
            </span>
          </div>
        </div>
        {recentItems.length > 0 && !q && (
          <div className="stack" style={{ gap: 6 }}>
            <span className="picker-h">Недавние</span>
            <div className="row" style={{ gap: 6 }}>{recentItems.map((x) => <button key={x.id} type="button" className="pill sm" onClick={() => onPick(x, x.ru)}>{x.ru}</button>)}</div>
          </div>
        )}
        <div className="picker-grid">
          {shown.map((x) => (
            <div key={x.id} className="picker-item">
              <button type="button" className="plain picker-pick" onClick={() => onPick(x, x.ru)} title="Добавить">
                {x.img > 0 ? <span className="picker-img" style={{ backgroundImage: `url(ex/${x.id}.webp)` }} /> : <span className="picker-img picker-noimg">{x.own ? '★' : 'K'}</span>}
                <b className="small" style={{ textAlign: 'left' }}>{x.ru}</b>
                <span className="muted tiny" style={{ textAlign: 'left' }}>{musclesText(x) || '—'} · {EQUIP[x.eq ?? ''] ?? x.eq ?? ''}{sideWord(x) ? ` · ${sideWord(x)}` : ''}</span>
              </button>
              <button type="button" className="linkbtn tiny picker-info" onClick={() => setInfo(x)}>как делать</button>
            </div>
          ))}
        </div>
        {hits.length > shown.length && <button type="button" className="btn2" style={{ alignSelf: 'center' }} onClick={() => setMore(more + 1)}>Показать ещё</button>}
        {hits.length === 0 && (
          <div className="stack" style={{ gap: 8 }}>
            <span className="muted small">Ничего не нашлось{anyFilter ? ' — попробуй убрать фильтры' : ''}.</span>
            {q.trim() && <button type="button" className="btn2" style={{ alignSelf: 'flex-start' }} onClick={() => onPick(null, q.trim())}>Записать «{q.trim()}» как своё упражнение</button>}
          </div>
        )}
        {info && <ExerciseCard ex={info} onClose={() => setInfo(null)} />}
      </div>
    </div>
  );
}

// ---------- band button ----------

type Band = { ids: string[]; mode: 'add' | 'assist' } | null | undefined;
export const bandText = (b: Band, bands: ResBand[]) => (b?.ids ?? []).map((id) => bands.find((x) => x.id === id)?.name || 'резина').join(' + ');

/** One button «〰 Зелёная + Фиолетовая»; the bands open in a small window. */
export function BandButton({ value, bands, onChange, assistable, compact, mixed }: { value: Band; bands: ResBand[]; onChange: (b: Band) => void; assistable?: boolean; compact?: boolean; mixed?: boolean }) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!open) return;
    const k = (e: MouseEvent) => { if (box.current && !box.current.contains(e.target as Node)) setOpen(false); };
    document.addEventListener('mousedown', k);
    return () => document.removeEventListener('mousedown', k);
  }, [open]);
  const ids = value?.ids ?? [];
  const toggle = (id: string) => {
    const n = ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id];
    onChange(n.length ? { ids: n, mode: value?.mode ?? 'add' } : null);
  };
  return (
    <span ref={box} className="band-btn-wrap">
      <button type="button" className={`pill sm${ids.length || mixed ? ' on' : ''}${compact ? ' band-mini' : ''}`} aria-pressed={ids.length > 0 || !!mixed} aria-expanded={open} onClick={() => setOpen(!open)}
        title={compact ? (ids.length ? `Резина этого подхода: ${bandText(value, bands)}` : 'Резина для этого подхода') : mixed ? 'У подходов разная резина — выбор здесь поставит одну на все подходы' : 'Резина на все подходы'}>
        {compact
          ? (ids.length ? <>{ids.map((id) => <span key={id} className="dot" style={{ background: bands.find((x) => x.id === id)?.color || '#9AA3AE', marginRight: 3 }} />)}{bandText(value, bands)}</> : '〰')
          : <>〰 {ids.length ? bandText(value, bands) : mixed ? 'разные' : 'резина'}{value?.mode === 'assist' && ids.length ? ' · помогает' : ''}</>}
      </button>
      {open && (
        <span className="band-pop" role="dialog" aria-label="Резина">
          {bands.length === 0 ? <span className="muted small">Резин пока нет — добавь их в «Тренировки → Инвентарь» с диапазоном, который написан на ленте.</span> : (
            <>
              <span className="row" style={{ gap: 6 }}>
                {bands.map((b) => (
                  <button key={b.id} type="button" className="pill sm" aria-pressed={ids.includes(b.id)} onClick={() => toggle(b.id)} title={`${String(b.lo).replace('.', ',')}–${String(b.hi).replace('.', ',')} ${b.unit === 'lb' ? 'lb' : 'кг'} · ≈${Math.round(bandKg(b))} кг`}>
                    <span className="dot" style={{ background: b.color || '#9AA3AE', marginRight: 5 }} />{b.name || 'Резина'}
                  </button>
                ))}
              </span>
              {assistable && ids.length > 0 && (
                <span className="row" style={{ gap: 6 }}>
                  <button type="button" className="pill sm" aria-pressed={value?.mode !== 'assist'} onClick={() => onChange({ ids, mode: 'add' })}>добавляет нагрузку</button>
                  <button type="button" className="pill sm" aria-pressed={value?.mode === 'assist'} onClick={() => onChange({ ids, mode: 'assist' })}>помогает</button>
                </span>
              )}
              <span className="row" style={{ gap: 10 }}>
                {ids.length > 0 && <button type="button" className="linkbtn tiny" onClick={() => onChange(null)}>без резины</button>}
                <button type="button" className="linkbtn tiny" onClick={() => setOpen(false)}>готово</button>
              </span>
            </>
          )}
        </span>
      )}
    </span>
  );
}

// ---------- the card ----------

type SetX = { row: SetRow; rests: SetRow[] };
type Block = { key: string; exId: string | null; name: string | null; sets: SetX[] };

function toBlocks(rows: SetRow[]): Block[] {
  const out: Block[] = [];
  let n = 0;
  for (const r of rows) {
    if (r.kind === 'rest') {
      const b = out[out.length - 1];
      if (b && b.sets.length) b.sets[b.sets.length - 1].rests.push(r);
      continue;
    }
    if (r.kind === 'skip') continue;
    const key = r.exId ?? r.exercise ?? '';
    const last = out[out.length - 1];
    if (last && (last.exId ?? last.name ?? '') === key) last.sets.push({ row: r, rests: [] });
    else out.push({ key: `b${n++}`, exId: r.exId ?? null, name: r.exercise, sets: [{ row: r, rests: [] }] });
  }
  return out;
}
const fromBlocks = (bs: Block[]): SetRow[] => bs.flatMap((b) => b.sets.flatMap((s) => [{ ...s.row, exId: b.exId, exercise: b.name }, ...s.rests]));
const epley = (w: number | null, r: number | null) => (w && r && r > 0 && r <= 15 ? w * (1 + r / 30) : null);
const num = (v: string) => (v.trim() === '' ? null : Number(v.replace(',', '.')));
const fmt = (x: number) => String(Math.round(x * 10) / 10).replace('.', ',');

export function StrengthCard({ d, onSaved }: { d: ActivityDetail; onSaved: () => void }) {
  const [blocks, setBlocks] = useState<Block[]>(() => toBlocks(d.sets));
  const [bands, setBands] = useState<ResBand[]>([]);
  const [dirty, setDirty] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [pick, setPick] = useState<{ block: number | null } | null>(null);
  const [info, setInfo] = useState<ExerciseItem | null>(null);
  const [menu, setMenu] = useState<number | null>(null);
  const [showRest, setShowRest] = useState(false);
  const lib = useExerciseLib();
  useEffect(() => { api.bandsGet().then(setBands).catch(() => {}); }, []);
  const exOf = (id: string | null) => (id ? lib?.items.find((x) => x.id === id) : undefined);
  const set = (bs: Block[]) => { setBlocks(bs); setDirty(true); setMsg(null); };
  const updBlock = (i: number, p: Partial<Block>) => set(blocks.map((b, k) => (k === i ? { ...b, ...p } : b)));
  const updSet = (i: number, j: number, p: Partial<SetRow>) => updBlock(i, { sets: blocks[i].sets.map((s, k) => (k === j ? { ...s, row: { ...s.row, ...p } } : s)) });
  const effOf = (b: Block, r: SetRow) => effectiveKg(exOf(b.exId), r.weightKg, d.bodyKg, r.band, bands);
  const factor = (b: Block, r: SetRow) => (exOf(b.exId)?.side && r.sides !== 1 ? 2 : 1);

  function addSet(i: number) {
    const b = blocks[i];
    const last = b.sets[b.sets.length - 1]?.row;
    const row: SetRow = { startUtc: null, durationS: last && exOf(b.exId)?.st ? last.durationS : null, reps: last?.reps ?? null, weightKg: last?.weightKg ?? null, kind: 'active', exercise: b.name, exId: b.exId, band: last?.band ?? null, sides: null };
    updBlock(i, { sets: [...b.sets, { row, rests: [] }] });
  }
  function picked(ex: ExerciseItem | null, name: string) {
    const target = pick?.block;
    setPick(null);
    forgetRecent();
    if (target != null) { updBlock(target, { exId: ex?.id ?? null, name: ex?.ru ?? name }); return; }
    set([...blocks, { key: `n${Date.now()}`, exId: ex?.id ?? null, name: ex?.ru ?? name, sets: [{ row: { startUtc: null, durationS: null, reps: null, weightKg: null, kind: 'active', exercise: ex?.ru ?? name, exId: ex?.id ?? null, band: null }, rests: [] }] }]);
  }
  function move(i: number, dir: -1 | 1) {
    const j = i + dir;
    if (j < 0 || j >= blocks.length) return;
    const a = [...blocks]; [a[i], a[j]] = [a[j], a[i]]; set(a);
  }
  async function save() {
    try {
      await api.strengthSetsSave(d.activity.id, fromBlocks(blocks));
      setMsg({ ok: true, text: 'Сохранено.' }); setDirty(false); forgetRecent(); onSaved();
    } catch (e) { setMsg({ ok: false, text: errText(e) }); }
  }

  let nSets = 0, nReps = 0, tonnage = 0, estimated = false;
  for (const b of blocks) for (const s of b.sets) {
    const f = factor(b, s.row);
    nSets++; nReps += (s.row.reps ?? 0) * f;
    const e = effOf(b, s.row);
    if (e != null && e !== s.row.weightKg) estimated = true;
    tonnage += (e ?? s.row.weightKg ?? 0) * (s.row.reps ?? 0) * f;
  }
  const named = blocks.filter((b) => b.name).length;

  return (
    <section className="card" aria-label="Силовая">
      <div className="between">
        <span className="label">Силовая · упражнения и подходы</span>
        <label className="row small" style={{ gap: 8, cursor: 'pointer' }}><input type="checkbox" checked={showRest} onChange={(e) => setShowRest(e.target.checked)} /> показывать отдых</label>
      </div>
      {blocks.length === 0 && <span className="muted small">Подходов пока нет. Нажми «+ Упражнение», выбери упражнение и впиши вес и повторы.</span>}
      {blocks.map((b, i) => {
        const ex = exOf(b.exId);
        const side = sideWord(ex);
        const st = !!ex?.st;
        const bandsAll = b.sets.map((s) => JSON.stringify(s.row.band?.ids ?? []));
        const sameBand = bandsAll.every((x) => x === bandsAll[0]);
        const anyBand = b.sets.some((s) => (s.row.band?.ids.length ?? 0) > 0);
        const bandEx = !!ex && ['bands', 'mini'].includes(eqOf(ex, lib));
        const assist = !!ex?.en.toLowerCase().match(/pull|chin|dip/);
        const reps = b.sets.reduce((a, s) => a + (s.row.reps ?? 0), 0);
        return (
          <div key={b.key} className="exb">
            <div className="exh">
              {ex && ex.img > 0 ? <button type="button" className="thumb" title="Как выполнять" onClick={() => setInfo(ex)} style={{ backgroundImage: `url(ex/${ex.id}.webp)` }} /> : <span className="thumb picker-noimg">{ex?.own ? '★' : 'K'}</span>}
              <span className="stack" style={{ gap: 1, flex: 1, minWidth: 0 }}>
                {b.name ? (
                  <button type="button" className="plain exh-name" onClick={() => (ex ? setInfo(ex) : setPick({ block: i }))}>{b.name}</button>
                ) : (
                  <button type="button" className="linkbtn small" style={{ alignSelf: 'flex-start' }} onClick={() => setPick({ block: i })}>Выбрать упражнение — {b.sets.length} подх. без названия</button>
                )}
                <span className="muted tiny">
                  {ex ? musclesText(ex) : b.name ? 'своё упражнение' : 'часы не сохранили названия'}
                  {ex?.bw != null && !st ? ` · свой вес ≈${Math.round(ex.bw * 100)}% веса тела` : ''}
                  {side ? <> · <span className="tag2">{side}</span></> : null}
                </span>
              </span>
              {!st && <BandButton value={sameBand ? b.sets[0]?.row.band : null} mixed={!sameBand} bands={bands} assistable={assist}
                onChange={(v) => updBlock(i, { sets: b.sets.map((s) => ({ ...s, row: { ...s.row, band: v } })) })} />}
              <span className="small muted exh-sum">{b.sets.length} {b.sets.length === 1 ? 'подход' : b.sets.length < 5 ? 'подхода' : 'подходов'}{reps ? ` · ${side ? `${reps}/${sideShort(ex)}` : `${reps} повт.`}` : ''}</span>
              <span className="exh-menu">
                <button type="button" className="xbtn" aria-label="Ещё" aria-expanded={menu === i} onClick={() => setMenu(menu === i ? null : i)}>⋯</button>
                {menu === i && (
                  <span className="band-pop" style={{ right: 0, left: 'auto' }} onMouseLeave={() => setMenu(null)}>
                    <button type="button" className="linkbtn small" onClick={() => { setMenu(null); setPick({ block: i }); }}>Заменить упражнение</button>
                    <button type="button" className="linkbtn small" disabled={i === 0} onClick={() => { setMenu(null); move(i, -1); }}>Выше</button>
                    <button type="button" className="linkbtn small" disabled={i === blocks.length - 1} onClick={() => { setMenu(null); move(i, 1); }}>Ниже</button>
                    <button type="button" className="linkbtn small" style={{ color: '#FF6B6B' }} onClick={() => { setMenu(null); set(blocks.filter((_, k) => k !== i)); }}>Удалить упражнение</button>
                  </span>
                )}
              </span>
            </div>
            <div className="setr setr-h muted tiny"><span>#</span><span>{st ? '' : 'вес, кг'}</span><span>{st ? 'секунд' : side ? `повторы / ${sideShort(ex)}` : 'повторы'}</span><span title="Сколько килограммов на самом деле работало в повторе">{st ? '' : 'нагрузка'}</span><span>{st ? '' : '1ПМ'}</span><span /></div>
            {b.sets.map((s, j) => {
              const r = s.row;
              const eff = effOf(b, r);
              const orm = epley(eff ?? r.weightKg, r.reps);
              return (
                <div key={j} className="stack" style={{ gap: 2 }}>
                  <div className="setr">
                    <span className="muted small num">{j + 1}</span>
                    {st ? <span /> : <input className="input sm" inputMode="decimal" aria-label="Вес" placeholder={ex?.bw != null ? 'свой вес' : '—'} value={r.weightKg ?? ''} onChange={(e) => updSet(i, j, { weightKg: num(e.target.value) })} />}
                    {st
                      ? <input className="input sm" inputMode="numeric" aria-label="Секунд" value={r.durationS ?? ''} onChange={(e) => updSet(i, j, { durationS: num(e.target.value) })} />
                      : <input className="input sm" inputMode="numeric" aria-label="Повторы" value={r.reps ?? ''} onChange={(e) => updSet(i, j, { reps: num(e.target.value) })} />}
                    <span className="num small" style={{ color: eff != null && eff !== r.weightKg ? '#9DB8FF' : 'var(--muted)' }}>{st ? '' : eff != null ? `${eff !== r.weightKg ? '≈' : ''}${Math.round(eff)}` : ''}</span>
                    <span className="num small muted">{orm ? Math.round(orm) : ''}</span>
                    <span className="row setr-act">
                      {!st && (anyBand || bandEx) && <BandButton compact value={r.band} bands={bands} assistable={assist} onChange={(v) => updSet(i, j, { band: v })} />}
                      {side && (
                        <button type="button" className="pill sm" aria-pressed={r.sides === 1} title="Сделал только одну сторону — посчитать один раз" onClick={() => updSet(i, j, { sides: r.sides === 1 ? null : 1 })}>1 сторона</button>
                      )}
                      {r.durationS && !st ? <span className="muted tiny num">{fmtDur(r.durationS)}</span> : null}
                      <button type="button" className="xbtn" aria-label="Удалить подход" onClick={() => {
                        const left = b.sets.filter((_, k) => k !== j);
                        set(left.length ? blocks.map((x, k) => (k === i ? { ...x, sets: left } : x)) : blocks.filter((_, k) => k !== i));
                      }}>×</button>
                    </span>
                  </div>
                  {showRest && s.rests.map((x, k) => <span key={k} className="muted tiny" style={{ paddingLeft: 40 }}>отдых {x.durationS ? fmtDur(x.durationS) : ''}</span>)}
                </div>
              );
            })}
            <button type="button" className="linkbtn small" style={{ alignSelf: 'flex-start', marginLeft: 40 }} onClick={() => addSet(i)}>+ подход</button>
          </div>
        );
      })}
      <div className="row">
        <button type="button" className="btn2" onClick={() => setPick({ block: null })}>+ Упражнение</button>
        {dirty && <button type="button" className="btn" onClick={save}>Сохранить</button>}
        {msg && <span className={msg.ok ? 'small' : 'small note-err'} style={msg.ok ? { color: 'var(--green)' } : undefined}>{msg.text}</span>}
      </div>
      {blocks.length > 0 && (
        <div className="tiles4">
          <div className="tile"><span className="num tile-v2">{named}</span><span className="muted tiny">упражнений</span></div>
          <div className="tile"><span className="num tile-v2">{nSets}</span><span className="muted tiny">подходов</span></div>
          <div className="tile"><span className="num tile-v2">{nReps}</span><span className="muted tiny">повторений</span></div>
          <div className="tile"><span className="num tile-v2">{tonnage ? `${estimated ? '≈' : ''}${Math.round(tonnage).toLocaleString('ru-RU')}` : '—'}</span><span className="muted tiny">тоннаж, кг</span></div>
        </div>
      )}
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>
        Нагрузка — сколько килограммов работало в повторе: вес снаряда, плюс доля веса тела{d.bodyKg ? ` (по твоему весу ${fmt(d.bodyKg)} кг)` : ''}, плюс или минус резина; ≈ — оценка. Резину можно выбрать на всё упражнение (кнопка сверху) или для каждого подхода отдельно (кнопка в строке подхода); «+ подход» берёт резину предыдущего. 1ПМ — расчётный максимум на одно повторение. У упражнений «на каждую руку / ногу» повторы пишутся на одну сторону, а тоннаж и нагрузка на мышцы считаются за обе; «1 сторона» — если делал только одну.
      </span>
      {pick && <ExercisePicker lib={lib} mode="strength" onPick={picked} onClose={() => setPick(null)} />}
      {info && <ExerciseCard ex={info} bodyKg={d.bodyKg} onClose={() => setInfo(null)} />}
    </section>
  );
}

