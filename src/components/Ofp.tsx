// «ОФП и комплексы» (1.4): on the workout page the athlete picks one of his complexes or a single
// exercise and says how many rounds / sets he did, reps or seconds per exercise, extra load (none,
// band, weight) or «не делал». Nothing is filled in by itself — «В прошлый раз» is a hint with a
// button. Each block is saved as ordinary set rows (activity_svc::save_ofp), so tonnage, muscle
// load and the muscle map work as for any sets. «Мои комплексы» (Тренировки → Инвентарь) edits the
// complexes and your own exercises.
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  api, errText,
  type ActivityDetail, type Complex, type CustomExercise, type ExerciseItem, type ExerciseLibrary, type OfpBlock, type OfpItem, type OfpLast, type ResBand,
} from '../lib/api';
import { ExerciseCard, ExerciseInput, MUSCLE, refreshExerciseLib, useExerciseLib } from './Exercises';
import { useBackendEvent } from '../lib/events';

const ruDay = (d: string) => new Date(`${d}T12:00:00`).toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' });
const num = (v: string) => (v.trim() === '' ? null : Math.max(0, Number(v.replace(',', '.'))) || null);
const roundsWord = (n: number, single: boolean) => {
  const m10 = n % 10, m100 = n % 100;
  const f = m10 === 1 && m100 !== 11 ? 0 : m10 >= 2 && m10 <= 4 && (m100 < 10 || m100 >= 20) ? 1 : 2;
  return `${n} ${single ? ['подход', 'подхода', 'подходов'][f] : ['круг', 'круга', 'кругов'][f]}`;
};

export const PAIN_AREAS: [string, string][] = [
  ['foot', 'Стопа'], ['ankles', 'Ахилл и голеностоп'], ['shin', 'Голень'], ['calves', 'Икры'], ['knees', 'Колено'], ['quads', 'Перед бедра'],
  ['hams', 'Зад бедра'], ['hips', 'Тазобедренный'], ['glutes', 'Ягодицы'], ['back', 'Спина'], ['core', 'Кор'], ['shoulders', 'Плечо'], ['neck', 'Шея'],
];

function itemOf(ex: ExerciseItem | undefined, exId: string | null, name: string | null, leg?: boolean | null): OfpItem {
  return { exId, name: ex?.ru ?? name, reps: null, sec: null, leg: leg ?? ex?.leg ?? false, load: { kind: 'none' }, skip: false };
}
const timed = (it: OfpItem, ex: ExerciseItem | undefined) => it.sec != null || (it.reps == null && !!ex?.st);

/** Short text of a block: «3 круга · Приседания 15 · Собака 10/нога · Планка 45 с». */
export function blockText(b: OfpBlock, bands: ResBand[]): string {
  const single = !b.cid && b.items.length === 1;
  const parts = b.items.map((it) => {
    if (it.skip) return `${it.name ?? 'упражнение'} — не делал`;
    const v = it.sec != null ? `${it.sec} с` : it.reps != null ? `${it.reps}${it.leg ? '/нога' : ''}` : '';
    const ld = it.load.kind === 'kg' && it.load.kg ? ` +${it.load.kg} кг` : it.load.kind === 'band' ? ` + ${(it.load.band?.ids ?? []).map((id) => bands.find((x) => x.id === id)?.name ?? 'резина').join('+') || 'резина'}` : '';
    return `${it.name ?? 'упражнение'}${v ? ` ${v}` : ''}${ld}`;
  });
  return `${roundsWord(b.rounds, single)} · ${parts.join(' · ')}`;
}

function LoadPick({ it, bands, onChange }: { it: OfpItem; bands: ResBand[]; onChange: (p: Partial<OfpItem>) => void }) {
  const k = it.load.kind;
  return (
    <span className="row" style={{ gap: 4, flexWrap: 'wrap' }}>
      <span className="row" role="group" aria-label="Утяжеление" style={{ gap: 4 }}>
        <button type="button" className="pill sm" aria-pressed={k === 'none'} onClick={() => onChange({ load: { kind: 'none' } })}>без</button>
        <button type="button" className="pill sm" aria-pressed={k === 'band'} onClick={() => onChange({ load: { kind: 'band', band: it.load.band ?? { ids: bands[0] ? [bands[0].id] : [], mode: 'add' } } })}>резина</button>
        <button type="button" className="pill sm" aria-pressed={k === 'kg'} onClick={() => onChange({ load: { kind: 'kg', kg: it.load.kg ?? null } })}>вес</button>
      </span>
      {k === 'kg' && (
        <input className="input sm" style={{ width: 64 }} inputMode="decimal" aria-label="Вес, кг" placeholder="кг" value={it.load.kg ?? ''}
          onChange={(e) => onChange({ load: { kind: 'kg', kg: num(e.target.value) } })} />
      )}
      {k === 'band' && (bands.length === 0
        ? <span className="muted tiny">резины добавь в «Инвентарь»</span>
        : bands.map((b) => {
          const on = it.load.band?.ids.includes(b.id) ?? false;
          return (
            <button key={b.id} type="button" className="pill sm" aria-pressed={on} title={`${b.lo}–${b.hi} ${b.unit === 'lb' ? 'lb' : 'кг'}`}
              onClick={() => {
                const ids = on ? (it.load.band?.ids ?? []).filter((x) => x !== b.id) : [...(it.load.band?.ids ?? []), b.id];
                onChange({ load: { kind: 'band', band: { ids, mode: it.load.band?.mode ?? 'add' } } });
              }}>
              <span className="dot" style={{ background: b.color || '#9AA3AE', marginRight: 5 }} />{b.name || 'Резина'}
            </button>
          );
        }))}
    </span>
  );
}

function BlockEditor({ b, lib, bands, last, bodyKg, onChange, onRemove }: {
  b: OfpBlock; lib: ExerciseLibrary | null; bands: ResBand[]; last?: { date: string; block: OfpBlock }; bodyKg?: number | null;
  onChange: (b: OfpBlock) => void; onRemove: () => void;
}) {
  const [info, setInfo] = useState<ExerciseItem | null>(null);
  const single = !b.cid && b.items.length === 1;
  const exOf = (it: OfpItem) => (it.exId ? lib?.items.find((x) => x.id === it.exId) : undefined);
  const upd = (i: number, p: Partial<OfpItem>) => onChange({ ...b, items: b.items.map((x, k) => (k === i ? { ...x, ...p } : x)) });
  return (
    <div className="inset ofp-block" aria-label={b.name}>
      <div className="between" style={{ alignItems: 'flex-start', gap: 8 }}>
        <span className="stack" style={{ gap: 2, minWidth: 0 }}>
          <b className="small">{b.name || b.items[0]?.name || 'Упражнение'}</b>
          {b.cid && <span className="muted tiny">комплекс · {b.items.length} упр.</span>}
        </span>
        <span className="row" style={{ gap: 6, flexWrap: 'nowrap' }}>
          <span className="muted small">{single ? 'Подходов' : 'Кругов'}</span>
          <button type="button" className="xbtn" aria-label="Меньше" onClick={() => onChange({ ...b, rounds: Math.max(1, (b.rounds || 1) - 1) })}>−</button>
          <input className="input sm num" style={{ width: 44, textAlign: 'center' }} inputMode="numeric" aria-label={single ? 'Подходов' : 'Кругов'}
            value={b.rounds || ''} placeholder="?" onChange={(e) => onChange({ ...b, rounds: Math.min(30, Math.round(num(e.target.value) ?? 0)) })} />
          <button type="button" className="xbtn" aria-label="Больше" onClick={() => onChange({ ...b, rounds: Math.min(30, (b.rounds || 0) + 1) })}>+</button>
          <button type="button" className="xbtn" aria-label="Убрать блок" title="Убрать из тренировки" onClick={onRemove}>×</button>
        </span>
      </div>
      {last && (
        <div className="ofp-last">
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>В прошлый раз ({ruDay(last.date)}): {blockText(last.block, bands)}</span>
          <button type="button" className="linkbtn tiny" onClick={() => onChange({ ...last.block, cid: b.cid, name: b.name })}>Как в прошлый раз</button>
        </div>
      )}
      <div className="stack" style={{ gap: 6 }}>
        {b.items.map((it, i) => {
          const ex = exOf(it);
          const tm = timed(it, ex);
          return (
            <div key={i} className={`ofp-item${it.skip ? ' skip' : ''}`}>
              <span className="row" style={{ gap: 6, flexWrap: 'nowrap', minWidth: 0 }}>
                <span className="muted tiny num" style={{ width: 16 }}>{i + 1}</span>
                {ex ? <button type="button" className="linkbtn small ellipsis" style={{ textAlign: 'left' }} title="Как выполнять" onClick={() => setInfo(ex)}>{it.name}</button>
                  : <span className="small ellipsis">{it.name}</span>}
                {it.leg && <span className="tag tiny">на ногу</span>}
              </span>
              {!it.skip && (
                <span className="row" style={{ gap: 6, flexWrap: 'wrap' }}>
                  <input className="input sm num" style={{ width: 76 }} inputMode="numeric" aria-label={tm ? 'Секунд' : 'Повторов'} placeholder={tm ? 'сек' : 'повт.'}
                    value={(tm ? it.sec : it.reps) ?? ''} onChange={(e) => upd(i, tm ? { sec: num(e.target.value), reps: null } : { reps: num(e.target.value), sec: null })} />
                  <button type="button" className="xbtn" title={tm ? 'Считать повторы' : 'Считать время'} aria-label="Повторы или время"
                    onClick={() => upd(i, tm ? { sec: null, reps: null } : { sec: it.reps ?? 30, reps: null })}>{tm ? 'с' : '×'}</button>
                  <LoadPick it={it} bands={bands} onChange={(p) => upd(i, p)} />
                </span>
              )}
              <label className="row tiny muted" style={{ gap: 4, cursor: 'pointer' }}>
                <input type="checkbox" checked={it.skip} onChange={(e) => upd(i, { skip: e.target.checked })} /> не делал
              </label>
            </div>
          );
        })}
      </div>
      {info && <ExerciseCard ex={info} bodyKg={bodyKg} onClose={() => setInfo(null)} />}
    </div>
  );
}

/** Picker of a complex or a single exercise. */
function AddBlock({ lib, complexes, onAdd }: { lib: ExerciseLibrary | null; complexes: Complex[]; onAdd: (b: OfpBlock) => void }) {
  const [mode, setMode] = useState<'none' | 'cx' | 'ex'>('none');
  const runner = useMemo(() => (lib?.runnerOfp ?? []).map((id) => lib?.items.find((x) => x.id === id)).filter((x): x is ExerciseItem => !!x), [lib]);
  const exOf = (id: string) => lib?.items.find((x) => x.id === id);
  const addEx = (ex: ExerciseItem | undefined, id: string | null, name: string | null) => {
    if (!name) return;
    onAdd({ cid: null, name: ex?.ru ?? name, rounds: 0, items: [itemOf(ex, id, name)] });
    setMode('none');
  };
  return (
    <div className="stack" style={{ gap: 8 }}>
      <div className="row" style={{ gap: 8 }}>
        <button type="button" className="btn2" aria-pressed={mode === 'cx'} onClick={() => setMode(mode === 'cx' ? 'none' : 'cx')}>+ Комплекс</button>
        <button type="button" className="btn2" aria-pressed={mode === 'ex'} onClick={() => setMode(mode === 'ex' ? 'none' : 'ex')}>+ Упражнение</button>
      </div>
      {mode === 'cx' && (
        complexes.length === 0
          ? <span className="muted small">Комплексов пока нет — создай их в «Тренировки → Инвентарь → Мои комплексы».</span>
          : (
            <div className="row" style={{ gap: 6 }}>
              {complexes.map((c) => (
                <button key={c.id} type="button" className="pill" onClick={() => {
                  onAdd({ cid: c.id, name: c.name, rounds: 0, items: c.items.map((x) => { const ex = exOf(x.exId); return itemOf(ex, x.exId, ex?.ru ?? x.exId, x.leg); }) });
                  setMode('none');
                }}>{c.name}</button>
              ))}
            </div>
          )
      )}
      {mode === 'ex' && (
        <div className="stack" style={{ gap: 8 }}>
          <ExerciseInput value={null} own={[]} lib={lib} onPick={(name, id) => addEx(id ? exOf(id) : undefined, id, name)} onInfo={() => {}} />
          {runner.length > 0 && (
            <>
              <span className="muted tiny">ОФП для бегунов:</span>
              <div className="row" style={{ gap: 6 }}>
                {runner.map((x) => <button key={x.id} type="button" className="pill sm" onClick={() => addEx(x, x.id, x.ru)}>{x.ru}</button>)}
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}

export function OfpCard({ d, onSaved }: { d: ActivityDetail; onSaved: () => void }) {
  const [blocks, setBlocks] = useState<OfpBlock[]>(d.ofp ?? []);
  const [complexes, setComplexes] = useState<Complex[]>([]);
  const [last, setLast] = useState<OfpLast>({});
  const [bands, setBands] = useState<ResBand[]>([]);
  const [dirty, setDirty] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const lib = useExerciseLib();
  const id = d.activity.id;
  const loadRefs = useCallback(() => {
    api.complexesGet().then((r) => setComplexes(r.complexes)).catch(() => {});
    api.ofpLast(id).then(setLast).catch(() => {});
    api.bandsGet().then(setBands).catch(() => {});
  }, [id]);
  useEffect(loadRefs, [loadRefs]);
  useBackendEvent<string>('data-changed', useCallback((k: string) => { if (k === 'complexes' || k === 'bands') loadRefs(); }, [loadRefs]));
  const keyOf = (b: OfpBlock) => b.cid ?? (b.items[0]?.exId ? `ex:${b.items[0].exId}` : `name:${b.items[0]?.name ?? ''}`);
  const set = (bs: OfpBlock[]) => { setBlocks(bs); setDirty(true); setMsg(null); };
  const missing = blocks.some((b) => !b.rounds);
  async function save() {
    if (missing) { setMsg({ ok: false, text: 'Укажи, сколько кругов или подходов сделал.' }); return; }
    try {
      await api.ofpSave(id, blocks);
      setDirty(false); setMsg({ ok: true, text: 'Сохранено.' }); onSaved();
    } catch (e) { setMsg({ ok: false, text: errText(e) }); }
  }
  const empty = blocks.length === 0;
  return (
    <section className="card" aria-label="ОФП и комплексы">
      <span className="label">ОФП и комплексы</span>
      {empty && <span className="muted small">Добавь комплекс или отдельное упражнение и отметь, сколько сделал. Они попадут в подходы, нагрузку и карту мышц.</span>}
      {blocks.map((b, i) => (
        <BlockEditor key={i} b={b} lib={lib} bands={bands} last={last[keyOf(b)]} bodyKg={d.bodyKg}
          onChange={(nb) => set(blocks.map((x, k) => (k === i ? nb : x)))} onRemove={() => set(blocks.filter((_, k) => k !== i))} />
      ))}
      <AddBlock lib={lib} complexes={complexes} onAdd={(b) => set([...blocks, b])} />
      {(dirty || msg) && (
        <div className="row">
          <button type="button" className="btn" disabled={!dirty} onClick={save}>Сохранить ОФП</button>
          {msg && <span className={msg.ok ? 'small' : 'small note-err'} style={msg.ok ? { color: 'var(--green)' } : undefined}>{msg.text}</span>}
        </div>
      )}
    </section>
  );
}

// ---------- «Мои комплексы» ----------

const MUSCLE_KEYS = Object.keys(MUSCLE);

function CustomEditor({ x, onChange, onDone }: { x: CustomExercise; onChange: (x: CustomExercise) => void; onDone: () => void }) {
  const toggle = (k: 'p' | 's', m: string) => onChange({ ...x, [k]: x[k].includes(m) ? x[k].filter((y) => y !== m) : [...x[k], m] });
  return (
    <div className="inset stack" style={{ gap: 8 }}>
      <input className="input" aria-label="Название упражнения" placeholder="Название" value={x.ru} onChange={(e) => onChange({ ...x, ru: e.target.value })} />
      <textarea className="input" rows={2} aria-label="Как выполнять" placeholder="Коротко: как выполнять" value={x.tip} onChange={(e) => onChange({ ...x, tip: e.target.value })} />
      <span className="muted tiny">Основные мышцы</span>
      <div className="row" style={{ gap: 4 }}>{MUSCLE_KEYS.map((m) => <button key={m} type="button" className="pill sm" aria-pressed={x.p.includes(m)} onClick={() => toggle('p', m)}>{MUSCLE[m]}</button>)}</div>
      <span className="muted tiny">Помогают</span>
      <div className="row" style={{ gap: 4 }}>{MUSCLE_KEYS.map((m) => <button key={m} type="button" className="pill sm" aria-pressed={x.s.includes(m)} onClick={() => toggle('s', m)}>{MUSCLE[m]}</button>)}</div>
      <div className="row" style={{ gap: 14 }}>
        <label className="row small" style={{ gap: 6 }}><input type="checkbox" checked={x.leg} onChange={(e) => onChange({ ...x, leg: e.target.checked })} /> на каждую ногу / сторону</label>
        <label className="row small" style={{ gap: 6 }}><input type="checkbox" checked={x.st} onChange={(e) => onChange({ ...x, st: e.target.checked })} /> на время (удержание)</label>
      </div>
      <button type="button" className="btn2" style={{ alignSelf: 'flex-start' }} onClick={onDone}>Готово</button>
    </div>
  );
}

const toCustom = (x: ExerciseItem): CustomExercise => ({ id: x.id, ru: x.ru, tip: x.tip, p: x.p, s: x.s, st: x.st, leg: !!x.leg, bw: x.bw, syn: x.syn });

export function ComplexesCard() {
  const [cx, setCx] = useState<Complex[] | null>(null);
  const [custom, setCustom] = useState<CustomExercise[]>([]);
  const [open, setOpen] = useState<string | null>(null);
  const [editEx, setEditEx] = useState<string | null>(null);
  const [del, setDel] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [info, setInfo] = useState<ExerciseItem | null>(null);
  const [pickKey, setPickKey] = useState(0);
  const lib = useExerciseLib();
  const file = useRef<HTMLInputElement>(null);
  const load = () => api.complexesGet().then((r) => { setCx(r.complexes); setCustom(r.custom.map(toCustom)); setDirty(false); }).catch((e) => setMsg({ ok: false, text: errText(e) }));
  useEffect(() => { load(); }, []);
  const exOf = (id: string): ExerciseItem | undefined => {
    const c = custom.find((x) => x.id === id);
    if (c) return { ...c, en: '', syn: c.syn ?? [], cat: 'strength', eq: 'body only', lvl: '', mech: '', force: '', img: 0, own: true };
    return lib?.items.find((x) => x.id === id);
  };
  const libPlus = useMemo<ExerciseLibrary | null>(() => (lib ? { ...lib, items: [...lib.items.filter((x) => !x.own), ...custom.map((c) => exOf(c.id)!)] } : null), [lib, custom]); // eslint-disable-line react-hooks/exhaustive-deps
  const change = (f: (c: Complex[]) => Complex[]) => { setCx((c) => f(c ?? [])); setDirty(true); setMsg(null); };
  const upd = (id: string, p: Partial<Complex>) => change((c) => c.map((x) => (x.id === id ? { ...x, ...p } : x)));
  const newId = (p: string) => `${p}_${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
  async function save() {
    try {
      await api.complexesSave(cx ?? [], custom);
      refreshExerciseLib(); setDirty(false); setMsg({ ok: true, text: 'Сохранено.' });
    } catch (e) { setMsg({ ok: false, text: errText(e) }); }
  }
  async function importFile(f: File) {
    try {
      const r = await api.complexesImport(await f.text());
      refreshExerciseLib(); await load();
      setMsg({ ok: true, text: `Добавлено комплексов: ${r.complexes}, своих упражнений: ${r.exercises}.` });
    } catch (e) { setMsg({ ok: false, text: errText(e) }); }
  }
  if (!cx) return <section className="card" aria-label="Мои комплексы"><span className="label">Мои комплексы</span><span className="muted small">Загружаю…</span></section>;
  const usedBy = (exId: string) => cx.filter((c) => c.items.some((i) => i.exId === exId)).map((c) => c.name);
  return (
    <section className="card" aria-label="Мои комплексы">
      <span className="label">Мои комплексы</span>
      <span className="muted small" style={{ lineHeight: 1.5 }}>Комплекс — это список упражнений. Сколько кругов, повторов и с какой резиной сделал, отмечаешь на странице тренировки в карточке «ОФП и комплексы».</span>
      {cx.length === 0 && <span className="muted small">Комплексов пока нет.</span>}
      {cx.map((c) => (
        <div key={c.id} className="stack" style={{ gap: 6 }}>
          <div className="list-row">
            <button type="button" className="linkbtn" style={{ flex: 1, textAlign: 'left' }} aria-expanded={open === c.id} onClick={() => setOpen(open === c.id ? null : c.id)}>
              <b className="small">{c.name}</b> <span className="muted tiny">· {c.items.length} упр.{c.areas.length ? ` · ${c.areas.map((a) => PAIN_AREAS.find((p) => p[0] === a)?.[1] ?? a).join(', ').toLowerCase()}` : ''}</span>
            </button>
            <span className="muted">{open === c.id ? '▴' : '▾'}</span>
          </div>
          {open === c.id && (
            <div className="inset stack" style={{ gap: 8 }}>
              <input className="input" aria-label="Название комплекса" value={c.name} onChange={(e) => upd(c.id, { name: e.target.value })} />
              <input className="input" aria-label="Заметка" placeholder="Заметка (где делать, что нужно)" value={c.note} onChange={(e) => upd(c.id, { note: e.target.value })} />
              <span className="muted tiny">Помогает при боли (Kouros предложит его в советах по боли)</span>
              <div className="row" style={{ gap: 4 }}>
                {PAIN_AREAS.map(([k, n]) => <button key={k} type="button" className="pill sm" aria-pressed={c.areas.includes(k)} onClick={() => upd(c.id, { areas: c.areas.includes(k) ? c.areas.filter((x) => x !== k) : [...c.areas, k] })}>{n}</button>)}
              </div>
              {c.items.map((it, i) => {
                const ex = exOf(it.exId);
                return (
                  <div key={i} className="row ofp-edit-row">
                    <span className="muted tiny num" style={{ width: 16 }}>{i + 1}</span>
                    {ex ? <button type="button" className="linkbtn small ellipsis" style={{ flex: 1, textAlign: 'left' }} onClick={() => setInfo(ex)}>{ex.ru}</button>
                      : <span className="small muted" style={{ flex: 1 }}>{it.exId} (нет в библиотеке)</span>}
                    {(it.leg ?? ex?.leg) && <span className="tag tiny">на ногу</span>}
                    {ex?.st && <span className="tag tiny">на время</span>}
                    {ex?.own && <button type="button" className="xbtn" title="Изменить своё упражнение" aria-label="Изменить упражнение" onClick={() => setEditEx(editEx === ex.id ? null : ex.id)}>✎</button>}
                    <button type="button" className="xbtn" aria-label="Выше" disabled={i === 0} onClick={() => { const a = [...c.items]; [a[i - 1], a[i]] = [a[i], a[i - 1]]; upd(c.id, { items: a }); }}>↑</button>
                    <button type="button" className="xbtn" aria-label="Ниже" disabled={i === c.items.length - 1} onClick={() => { const a = [...c.items]; [a[i + 1], a[i]] = [a[i], a[i + 1]]; upd(c.id, { items: a }); }}>↓</button>
                    <button type="button" className="xbtn" aria-label="Убрать" onClick={() => upd(c.id, { items: c.items.filter((_, k) => k !== i) })}>×</button>
                    {editEx === ex?.id && ex?.own && (
                      <div style={{ flexBasis: '100%' }}>
                        <CustomEditor x={custom.find((y) => y.id === ex.id)!} onChange={(nx) => { setCustom(custom.map((y) => (y.id === nx.id ? nx : y))); setDirty(true); }} onDone={() => setEditEx(null)} />
                      </div>
                    )}
                  </div>
                );
              })}
              <span className="muted tiny">Добавить упражнение из библиотеки или своё:</span>
              <ExerciseInput key={pickKey} value={null} own={[]} lib={libPlus} onInfo={setInfo} onPick={(name, id) => {
                setPickKey((k) => k + 1);
                if (id) upd(c.id, { items: [...c.items, { exId: id }] });
                else if (name) {
                  const nx: CustomExercise = { id: newId('c'), ru: name, tip: '', p: [], s: [], st: false, leg: false, bw: null };
                  setCustom([...custom, nx]); upd(c.id, { items: [...c.items, { exId: nx.id }] }); setEditEx(nx.id);
                }
              }} />
              <span className="muted tiny">Нет в списке — напиши название и нажми вне поля: Kouros создаст своё упражнение, отметь в нём мышцы.</span>
              <div className="row" style={{ gap: 10 }}>
                {del === c.id ? (
                  <>
                    <span className="small">Удалить «{c.name}»? Прошлые тренировки с ним останутся.</span>
                    <button type="button" className="btn2 btn-danger" onClick={() => { change((a) => a.filter((x) => x.id !== c.id)); setDel(null); setOpen(null); }}>Удалить</button>
                    <button type="button" className="linkbtn small" onClick={() => setDel(null)}>Отмена</button>
                  </>
                ) : <button type="button" className="linkbtn small" style={{ color: 'var(--muted)' }} onClick={() => setDel(c.id)}>Удалить комплекс</button>}
              </div>
            </div>
          )}
        </div>
      ))}
      {custom.filter((x) => usedBy(x.id).length === 0).length > 0 && (
        <span className="muted tiny">Свои упражнения вне комплексов: {custom.filter((x) => usedBy(x.id).length === 0).map((x) => x.ru).join(', ')}.</span>
      )}
      <div className="row" style={{ gap: 8 }}>
        <button type="button" className="btn2" onClick={() => { const id = newId('x'); change((a) => [...a, { id, name: 'Новый комплекс', note: '', areas: [], items: [] }]); setOpen(id); }}>+ Новый комплекс</button>
        <button type="button" className="btn2" onClick={() => file.current?.click()}>Загрузить из файла</button>
        <input ref={file} type="file" accept=".json,application/json" hidden onChange={(e) => { const f = e.target.files?.[0]; if (f) importFile(f); e.target.value = ''; }} />
        {dirty && <button type="button" className="btn" onClick={save}>Сохранить</button>}
        {dirty && <button type="button" className="linkbtn small" onClick={load}>Отменить</button>}
      </div>
      {msg && <span className={msg.ok ? 'small' : 'small note-err'} style={msg.ok ? { color: 'var(--green)' } : undefined}>{msg.text}</span>}
      {info && <ExerciseCard ex={info} onClose={() => setInfo(null)} />}
    </section>
  );
}
