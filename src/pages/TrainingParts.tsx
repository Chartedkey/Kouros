// Blocks of the Training tab built from `training_overview` and `records_extra`.
import { useEffect, useMemo, useState } from 'react';
import {
  api, errText, fmtDur, fmtKm, fmtPace, plural, sportOf,
  type AthleteProfile, type EngineSummary, type RecordsExtra, type Shoe, type TrainingOverview, type WeekAgg,
} from '../lib/api';
import { SportIcon } from '../components/SportIcon';

const n0 = (x: number | null | undefined) => (x == null ? '—' : Math.round(x).toLocaleString('ru-RU'));
const n1 = (x: number | null | undefined) => (x == null ? '—' : x.toFixed(1).replace('.', ','));
const dayLabel = (d: string) => new Date(d + 'T12:00:00').toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' });
const WD = ['Пн', 'Вт', 'Ср', 'Чт', 'Пт', 'Сб', 'Вс'];
const weekday = (d: string) => WD[(new Date(d + 'T12:00:00').getDay() + 6) % 7];

function readLS(key: string, def: string): string {
  try { return localStorage.getItem(key) ?? def; } catch { return def; }
}
function writeLS(key: string, v: string) {
  try { localStorage.setItem(key, v); } catch { /* not important */ }
}

// ---------- which shoes? ----------

export function ShoeQuestion({ q, onDone }: { q: NonNullable<TrainingOverview['shoeQuestion']>; onDone: () => void }) {
  const [shoes, setShoes] = useState<Shoe[]>([]);
  useEffect(() => { api.shoesGet().then((l) => setShoes(l.filter((s) => !s.retired && (q.sport === 'run' || s.walks)))).catch(() => {}); }, [q.sport]);
  if (!shoes.length) return null;
  const pick = (id: string) => api.activitySetShoe(q.id, id).then(onDone).catch(() => {});
  return (
    <section className="card" aria-label="Кроссовки" style={{ flexDirection: 'row', flexWrap: 'wrap', alignItems: 'center', gap: 12 }}>
      <span className="stack" style={{ gap: 2, flex: '1 1 260px' }}>
        <b>В каких кроссовках была тренировка?</b>
        <span className="muted tiny">Пока пара не выбрана, пробег этой тренировки никуда не засчитан.</span>
        <span className="muted small">{q.name || sportOf(q.sport).name} · {new Date(q.startUtc * 1000).toLocaleString('ru-RU', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })}{q.distanceM ? ` · ${fmtKm(q.distanceM)}` : ''}</span>
      </span>
      <span className="row">
        {shoes.map((s) => <button key={s.id} type="button" className="pill" onClick={() => pick(s.id)}>{s.name}</button>)}
        <button type="button" className="pill" onClick={() => pick('none')}>Другие</button>
      </span>
    </section>
  );
}

// ---------- recovery ----------

/** Hours until 7-day fatigue falls back to the 42-day fitness level, adjusted by today's readiness. */
/** Hours left to recover — the same number as on «Сегодня» (heart and muscles, per workout). */
export function recoveryHours(s: EngineSummary): number | null {
  return s.recovery ? s.recovery.leftH : null;
}

export function RecoveryTile({ s }: { s: EngineSummary }) {
  const h = recoveryHours(s);
  const until = h ? new Date(Date.now() + h * 3600000).toLocaleString('ru-RU', { weekday: 'short', hour: '2-digit', minute: '2-digit' }) : null;
  return (
    <div className="tile">
      <span className="num tile-v2" style={{ color: h ? '#F5B642' : '#3DDC97' }}>{h == null ? '—' : h === 0 ? 'готов' : `≈ ${h} ч`}</span>
      <span className="muted tiny">{h ? `восстановление · до ${until}` : 'восстановление'}</span>
    </div>
  );
}

// ---------- heart and muscle load, history ----------

type Bucket = { from: string; to: string; label: string; heart: number; muscle: number; hUsual: number | null; mUsual: number | null; hA: number | null; mA: number | null };
const HEART = '#4FA3FF', MUSC = '#FF8A65';

function Bars({ list, pick, sel, onPick, color, usual }: { list: Bucket[]; pick: (b: Bucket) => number; sel: number | null; onPick: (i: number) => void; color: string; usual: (b: Bucket) => number | null }) {
  const max = Math.max(1, ...list.map((b) => Math.max(pick(b), usual(b) ?? 0))) * 1.08;
  return (
    <div className="sleepbars" style={{ height: 110 }}>
      {list.map((b, i) => {
        const u = usual(b);
        return (
          <div key={b.from} className={`sbar${sel === i ? ' sel' : ''}`} style={{ cursor: 'pointer', position: 'relative' }} onClick={() => onPick(i)} title={`${b.label}: ${Math.round(pick(b))}${u != null ? ` · обычно ${Math.round(u)}` : ''}`}>
            <div className="sstack" style={{ height: `${(pick(b) / max) * 100}%`, background: color, opacity: sel == null || sel === i ? 0.9 : 0.45 }} />
            {u != null && <span className="usual-dash" style={{ bottom: `${(u / max) * 100}%` }} />}
          </div>
        );
      })}
    </div>
  );
}

function Ratio({ list, sel }: { list: Bucket[]; sel: number | null }) {
  const W = 600, H = 120, top = 2.2;
  const y = (v: number) => H - (Math.min(v, top) / top) * H;
  const x = (i: number) => (list.length <= 1 ? W / 2 : (i / (list.length - 1)) * W);
  const line = (f: (b: Bucket) => number | null) => {
    let d = '';
    list.forEach((b, i) => {
      const v = f(b);
      if (v == null) return;
      d += `${d ? 'L' : 'M'}${x(i).toFixed(1)},${y(v).toFixed(1)}`;
    });
    return d;
  };
  return (
    <svg viewBox={`0 0 ${W} ${H}`} width="100%" height={H} preserveAspectRatio="none" aria-label="Соотношение недели к обычной">
      <rect x="0" y={y(1.3)} width={W} height={y(0.8) - y(1.3)} fill="#3DDC97" opacity="0.08" />
      <line x1="0" x2={W} y1={y(1)} y2={y(1)} stroke="#2A2F37" vectorEffect="non-scaling-stroke" />
      <line x1="0" x2={W} y1={y(1.5)} y2={y(1.5)} stroke="#FF6B6B" strokeDasharray="4 4" opacity="0.5" vectorEffect="non-scaling-stroke" />
      <path d={line((b) => b.hA)} fill="none" stroke={HEART} strokeWidth="2" vectorEffect="non-scaling-stroke" />
      <path d={line((b) => b.mA)} fill="none" stroke={MUSC} strokeWidth="2" vectorEffect="non-scaling-stroke" />
      {sel != null && <line x1={x(sel)} x2={x(sel)} y1="0" y2={H} stroke="#f1f3f5" opacity="0.5" vectorEffect="non-scaling-stroke" />}
    </svg>
  );
}

const PERIODS_L: [string, string][] = [['d28', '4 недели'], ['w13', '3 месяца'], ['w52', 'Год']];

/** «Нагрузка: сердце и мышцы» — two loads side by side, never added together. */
export function LoadDual({ s, onOpen }: { s: EngineSummary; onOpen: (id: string) => void }) {
  const [per, setPer] = useState('d28');
  const [sel, setSel] = useState<number | null>(null);
  const hist = s.load.history ?? [];
  const list = useMemo<Bucket[]>(() => {
    if (!hist.length) return [];
    if (per === 'd28') {
      return hist.slice(-28).map(([d, h, m, ha, ma, hw, mw]) => ({ from: d, to: d, label: `${weekday(d)} ${dayLabel(d)}`, heart: h, muscle: m, hUsual: hw != null ? hw / 7 : null, mUsual: mw ? mw / 7 : null, hA: ha, mA: ma }));
    }
    const n = per === 'w13' ? 13 : 52;
    // Weeks Monday to Sunday, the current one last.
    const out: Bucket[] = [];
    let cur: Bucket | null = null;
    for (const [d, h, m, ha, ma, hw, mw] of hist) {
      const wd = (new Date(d + 'T12:00:00').getDay() + 6) % 7;
      if (!cur || wd === 0) {
        cur = { from: d, to: d, label: '', heart: 0, muscle: 0, hUsual: null, mUsual: null, hA: null, mA: null };
        out.push(cur);
      }
      cur.to = d; cur.heart += h; cur.muscle += m; cur.hUsual = hw; cur.mUsual = mw || null; cur.hA = ha; cur.mA = ma;
      cur.label = `${dayLabel(cur.from)} — ${dayLabel(d)}`;
    }
    return out.slice(-n);
  }, [hist, per]);
  const b = sel != null ? list[sel] : null;
  const acts = (s.load.acts ?? []).filter((a) => {
    if (!b) return false;
    const d = new Date(a.start * 1000).toLocaleDateString('sv-SE');
    return d >= b.from && d <= b.to;
  });
  if (!list.length) return null;
  return (
    <section className="card" id="load-dual" aria-label="Нагрузка: сердце и мышцы">
      <div className="between">
        <span className="label">Нагрузка: сердце и мышцы</span>
        <div className="row" role="group" aria-label="Период">
          {PERIODS_L.map(([k, n]) => <button key={k} type="button" className="pill" aria-pressed={per === k} onClick={() => { setPer(k); setSel(null); }}>{n}</button>)}
        </div>
      </div>
      <span className="small"><b style={{ color: HEART }}>Сердце</b> <span className="muted">— по пульсу (TRIMP), {per === 'd28' ? 'по дням' : 'по неделям'}; чёрточка — обычно</span></span>
      <Bars list={list} pick={(x) => x.heart} sel={sel} onPick={(i) => setSel(sel === i ? null : i)} color={HEART} usual={(x) => x.hUsual} />
      <span className="small"><b style={{ color: MUSC }}>Мышцы и связки</b> <span className="muted">— дистанция, темп, подъёмы и спуски, подходы</span></span>
      <Bars list={list} pick={(x) => x.muscle} sel={sel} onPick={(i) => setSel(sel === i ? null : i)} color={MUSC} usual={(x) => x.mUsual} />
      <div className="between tiny muted"><span>{dayLabel(list[0].from)}</span><span>{dayLabel(list[list.length - 1].to)}</span></div>
      <span className="small"><b>Неделя к обычной</b> <span className="muted">— зелёная полоса 0,8–1,3 · пунктир 1,5 — высокий риск · <span style={{ color: HEART }}>сердце</span> и <span style={{ color: MUSC }}>мышцы</span></span></span>
      <Ratio list={list} sel={sel} />
      {b ? (
        <div className="stack" style={{ gap: 0 }}>
          <span className="small" style={{ fontWeight: 700, paddingBottom: 4 }}>
            {b.label}: сердце {n0(b.heart)}{b.hUsual != null ? ` (обычно ${n0(b.hUsual)})` : ''} · мышцы {n0(b.muscle)}{b.mUsual != null ? ` (обычно ${n0(b.mUsual)})` : ''}
          </span>
          {acts.length ? acts.map((a) => {
            const sp = sportOf(a.sport);
            return (
              <button key={a.id} type="button" className="list-row plain small" onClick={() => onOpen(a.id)}>
                <SportIcon sport={a.sport} size={30} />
                <span style={{ flex: 1, textAlign: 'left' }}>{a.name || sp.name}
                  <span className="muted"> · {new Date(a.start * 1000).toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' })}{(a.up ?? 0) > 0 ? ` · ↑${a.up} ↓${a.down} м` : ''}</span>
                  {a.bad && <span style={{ color: '#F5B642' }}> · высота записана со сбоем</span>}
                </span>
                <span className="num" style={{ color: HEART, width: 48, textAlign: 'right' }}>{a.heart ?? '—'}</span>
                <span className="num" style={{ color: MUSC, width: 48, textAlign: 'right' }}>{a.muscle}</span>
              </button>
            );
          }) : <span className="muted small">Тренировок нет.</span>}
        </div>
      ) : <span className="muted tiny">Нажми на столбик — внизу появятся тренировки этого {per === 'd28' ? 'дня' : 'периода'} с обеими нагрузками.</span>}
    </section>
  );
}

// ---------- running tolerance ----------

export function Tolerance({ t }: { t: TrainingOverview['tolerance'] }) {
  const [walks, setWalks] = useState(() => readLS('kouros.tolWalks', '1') === '1');
  const now = t.days.reduce((a, d) => a + d.km + d.extra + (walks ? d.walk : 0), 0);
  const avg = t.avgWeekRun + (walks ? t.avgWeekWalk : 0);
  const lo = avg * 0.8, hi = avg * 1.15;
  const max = Math.max(hi * 1.35, now * 1.1, 1);
  const [title, color, text] = avg < 1
    ? ['Мало истории', '#9AA3AE', 'Нужно несколько недель пробежек, чтобы понять твой коридор.']
    : now > hi ? ['Выше коридора', '#FF6B6B', 'Ноги получили больше обычного — следующие пробежки лучше короче или по мягкому покрытию.']
      : now < lo ? ['Ниже обычного', '#4FA3FF', 'Запас есть — можно добавить километры, не выходя за коридор.']
        : ['В коридоре', '#3DDC97', 'Ударная нагрузка на ноги в пределах привычной.'];
  const dmax = Math.max(...t.days.map((d) => d.km + d.extra + (walks ? d.walk : 0)), 1);
  return (
    <section className="card" aria-label="Беговая толерантность">
      <div className="between"><span className="label">Беговая толерантность · 7 дней</span><span className="muted tiny">ударная нагрузка на ноги</span></div>
      <div className="stack" style={{ gap: 2 }}>
        <span style={{ fontFamily: 'var(--display)', fontSize: 22, fontWeight: 700, color }}>{title}</span>
        <span className="muted small">{text}</span>
      </div>
      <span className="small">За 7 дней <b className="num">{n1(now)}</b> <span className="muted">из коридора {n1(lo)}–{n1(hi)} экв. км</span></span>
      <div className="corridor">
        <div className="band" style={{ left: `${(lo / max) * 100}%`, width: `${((hi - lo) / max) * 100}%` }} />
        <div className="fill" style={{ width: `${Math.min(100, (now / max) * 100)}%`, background: color }} />
      </div>
      <div className="between tiny muted"><span>0</span><span>твой безопасный коридор</span><span className="num">{n1(max)}</span></div>
      <div className="daybars">
        {t.days.map((d) => (
          <div key={d.date} className="daybar" title={`${dayLabel(d.date)}: бег ${n1(d.km)}, рельеф и интенсивность ${n1(d.extra)}, ходьба ${n1(d.walk)}`}>
            <span className="num tiny">{n1(d.km + d.extra + (walks ? d.walk : 0))}</span>
            <div className="stackbar" style={{ height: 70 }}>
              {walks && <div style={{ height: `${(d.walk / dmax) * 100}%`, background: '#7FD3FF' }} />}
              <div style={{ height: `${(d.extra / dmax) * 100}%`, background: '#F5B642' }} />
              <div style={{ height: `${(d.km / dmax) * 100}%`, background: '#3DDC97' }} />
            </div>
            <span className="muted tiny">{weekday(d.date)}</span>
          </div>
        ))}
      </div>
      <div className="row tiny" style={{ gap: 14 }}>
        <span className="row" style={{ gap: 6 }}><span className="dot" style={{ background: '#3DDC97' }} />бег, км</span>
        <span className="row" style={{ gap: 6 }}><span className="dot" style={{ background: '#F5B642' }} />рельеф и интенсивность</span>
        <span className="row" style={{ gap: 6 }}><span className="dot" style={{ background: '#7FD3FF' }} />ходьба</span>
      </div>
      <label className="row small" style={{ gap: 10, cursor: 'pointer' }}>
        <input type="checkbox" checked={walks} onChange={(e) => { setWalks(e.target.checked); writeLS('kouros.tolWalks', e.target.checked ? '1' : '0'); }} /> Учитывать ходьбу (30% от бега)
      </label>
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>
        Эквивалентные км = дистанция + 1 км на каждые 100 м подъёма, с поправкой на интенсивность по нагрузке. Коридор — твоя средняя неделя за 4 недели до этих 7 дней, от −20% до +15%.
      </span>
    </section>
  );
}

// ---------- history by week ----------

const GROUPS: Record<string, string> = { all: 'Все', run: 'Бег', walk: 'Ходьба', strength: 'Силовые', workout: 'Физио и йога', ride: 'Вело', other: 'Другое' };
const METRICS: [number, string, (v: number) => string][] = [
  [1, 'Время', (v) => `${n1(v / 3600)} ч`],
  [2, 'Дистанция', (v) => `${n1(v / 1000)} км`],
  [4, 'Нагрузка', (v) => n0(v)],
  [0, 'Тренировки', (v) => n0(v)],
  [3, 'Набор высоты', (v) => `${n0(v)} м`],
];
const PERIODS: [number, string][] = [[12, '3 месяца'], [26, 'Полгода'], [52, 'Год']];

export function History({ o }: { o: TrainingOverview }) {
  const [g, setG] = useState('all');
  const [m, setM] = useState(1);
  const [weeksN, setWeeksN] = useState(26);
  const [, label, fmt] = METRICS.find((x) => x[0] === m)!;
  const weeks = o.history.slice(-weeksN);
  const val = (s: Record<string, WeekAgg>, k: number) =>
    g === 'all' ? Object.values(s).reduce((a, v) => a + v[k], 0) : s[g]?.[k] ?? 0;
  const vals = weeks.map((w) => val(w.s, m));
  const max = Math.max(...vals, 1);
  const tot = (k: number) => weeks.reduce((a, w) => a + val(w.s, k), 0);
  return (
    <section className="card" aria-label="История тренировок">
      <div className="between"><span className="label">История тренировок</span><span className="muted tiny">по неделям</span></div>
      <div className="filters">
        <div className="row" role="group" aria-label="Вид спорта">
          {['all', ...o.sportsYear].map((k) => <button key={k} type="button" className="pill" aria-pressed={g === k} onClick={() => setG(k)}>{GROUPS[k] ?? k}</button>)}
        </div>
        <div className="row" role="group" aria-label="Период">
          {PERIODS.map(([n, nm]) => <button key={n} type="button" className="pill" aria-pressed={weeksN === n} onClick={() => setWeeksN(n)}>{nm}</button>)}
        </div>
      </div>
      <div className="tiles4" style={{ gridTemplateColumns: 'repeat(5, minmax(0, 1fr))' }}>
        {METRICS.map(([k, nm, f]) => (
          <button key={k} type="button" className={`tile tile-btn${m === k ? ' on' : ''}`} onClick={() => setM(k)}>
            <span className="num tile-v2">{f(tot(k))}</span><span className="muted tiny">{nm}</span>
          </button>
        ))}
      </div>
      <div className="between small"><b>{label}</b><span className="muted tiny">макс {fmt(max)} в неделю</span></div>
      <div className="hbars" style={{ height: 120 }}>
        {weeks.map((w, i) => (
          <div key={w.week} className="hbar" title={`Неделя с ${dayLabel(w.week)}: ${fmt(vals[i])}`}>
            <div style={{ height: `${(vals[i] / max) * 100}%` }} />
          </div>
        ))}
      </div>
      <div className="between tiny muted"><span>{weeks[0] ? dayLabel(weeks[0].week) : ''}</span><span>эта неделя</span></div>
      <span className="muted tiny">Нажми на итог сверху, чтобы показать его на графике. Виды спорта появляются, если они были за последний год.</span>
    </section>
  );
}

// ---------- intensity by week ----------

export function IntensityWeeks({ o }: { o: TrainingOverview }) {
  const [mode, setMode] = useState<'run' | 'all'>(() => (readLS('kouros.intMode', 'run') === 'all' ? 'all' : 'run'));
  const rows = o.intensityWeeks.map((w) => {
    const z = w[mode];
    const low = z[0] + z[1] + z[2], mid = z[3], high = z[4] + z[5], tot = low + mid + high;
    return { week: w.week, low, mid, high, tot, runs: w.runs };
  });
  const sum = rows.reduce((a, r) => ({ low: a.low + r.low, mid: a.mid + r.mid, high: a.high + r.high, tot: a.tot + r.tot }), { low: 0, mid: 0, high: 0, tot: 0 });
  const pct = (x: number, t: number) => (t ? Math.round((x / t) * 100) : 0);
  return (
    <section className="card" aria-label="Интенсивность по неделям">
      <div className="between">
        <span className="label">Интенсивность по неделям</span>
        <div className="row" role="group" aria-label="Что считать">
          {(['run', 'all'] as const).map((k) => <button key={k} type="button" className="pill" aria-pressed={mode === k} onClick={() => { setMode(k); writeLS('kouros.intMode', k); }}>{k === 'run' ? 'Бег' : 'Все с пульсом'}</button>)}
        </div>
      </div>
      <div className="stack" style={{ gap: 8 }}>
        {rows.map((r) => (
          <div key={r.week} className="iwrow">
            <span className="muted tiny">{dayLabel(r.week)}</span>
            {r.tot > 0 ? (
              <div style={{ display: 'flex', height: 12, borderRadius: 999, overflow: 'hidden', background: '#22262D' }}>
                <div style={{ width: `${pct(r.low, r.tot)}%`, background: '#3DDC97' }} />
                <div style={{ width: `${pct(r.mid, r.tot)}%`, background: '#F5B642' }} />
                <div style={{ width: `${pct(r.high, r.tot)}%`, background: '#FF6B6B' }} />
              </div>
            ) : <span className="muted tiny">нет тренировок с пульсом</span>}
            <span className="num tiny" style={{ textAlign: 'right' }}>{r.tot ? `${pct(r.low, r.tot)}% · ${n0(r.tot)} мин` : ''}</span>
          </div>
        ))}
      </div>
      <div className="row tiny" style={{ gap: 14 }}>
        <span><b style={{ color: '#3DDC97' }}>{pct(sum.low, sum.tot)}%</b> лёгкая Z1–Z2</span>
        <span><b style={{ color: '#F5B642' }}>{pct(sum.mid, sum.tot)}%</b> Z3</span>
        <span><b style={{ color: '#FF6B6B' }}>{pct(sum.high, sum.tot)}%</b> Z4–Z5</span>
        <span className="muted">за {plural(rows.length, 'неделю', 'недели', 'недель')}</span>
      </div>
      <span className="muted tiny">Ориентир для бегуна — около 80% лёгкой работы и 20% качественной. Разбор недели с советами — в «Тренер → Анализ недели».</span>
    </section>
  );
}

// ---------- records ----------

const DIST: Record<number, string> = { 1000: '1 км', 5000: '5 км', 10000: '10 км', 21097: 'Полумарафон', 21097.5: 'Полумарафон', 42195: 'Марафон' };
function clock(s: number) {
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), sec = Math.round(s % 60);
  return h ? `${h}:${String(m).padStart(2, '0')}:${String(sec).padStart(2, '0')}` : `${m}:${String(sec).padStart(2, '0')}`;
}

export function RecordsFull({ s, onOpen }: { s: EngineSummary; onOpen: (id: string) => void }) {
  const [x, setX] = useState<RecordsExtra | null>(null);
  const [cfg, setCfg] = useState(false);
  const [form, setForm] = useState<{ exercise: string; weight: string; reps: string } | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const load = () => api.recordsExtra().then(setX).catch((e) => setErr(errText(e)));
  useEffect(() => { load(); }, []);
  const hidden = x?.hidden ?? [];
  const items = useMemo(() => {
    const out: { key: string; name: string; val: string; sub: string; id: string | null; tag?: string }[] = [];
    for (const r of s.records) out.push({ key: `d${r.distanceM}`, name: DIST[r.distanceM] ?? `${r.distanceM} м`, val: clock(r.timeS), sub: `${dayLabel(r.date)} · ${fmtPace(r.timeS / (r.distanceM / 1000))}/км`, id: r.activityId, tag: 'бег' });
    const lr = (k: string, nm: string, v: RecordsExtra['longestRun'], tag: string, byTime = false) => {
      if (v) out.push({ key: k, name: nm, val: byTime ? fmtDur(v.timeS) : fmtKm(v.distanceM), sub: `${new Date(v.startUtc * 1000).toLocaleDateString('ru-RU', { day: 'numeric', month: 'short', year: 'numeric' })}${byTime ? '' : ` · ${fmtDur(v.timeS)}`}`, id: v.activityId, tag });
    };
    if (x) {
      lr('longestRun', 'Самая длинная пробежка', x.longestRun, 'бег');
      lr('longestWalk', 'Самая длинная прогулка', x.longestWalk, 'ходьба');
      lr('longestSession', 'Самая долгая тренировка', x.longestSession, 'всё', true);
      for (const r of x.strength) out.push({ key: `s:${r.exercise}`, name: r.exercise, val: `${n0(r.orm)} кг`, sub: `1ПМ · из ${n1(r.weightKg)} × ${r.reps}${r.manual ? ' · внесено вручную' : r.startUtc ? ` · ${new Date(r.startUtc * 1000).toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' })}` : ''}`, id: r.activityId, tag: 'силовая' });
    }
    return out;
  }, [s.records, x]);

  async function toggle(key: string) {
    const next = hidden.includes(key) ? hidden.filter((k) => k !== key) : [...hidden, key];
    try { await api.settingListSet('records_hidden', next); load(); } catch (e) { setErr(errText(e)); }
  }
  async function addManual() {
    if (!form || !x) return;
    const w = Number(form.weight.replace(',', '.')), r = Number(form.reps);
    if (!form.exercise.trim() || !(w > 0) || !(r >= 1 && r <= 30)) { setErr('Нужны упражнение, вес и число повторений'); return; }
    const next = [...x.manual, { exercise: form.exercise.trim(), weightKg: w, reps: r, date: new Date().toISOString().slice(0, 10) }];
    try { await api.settingListSet('strength_manual', next); setForm(null); setErr(null); load(); } catch (e) { setErr(errText(e)); }
  }
  async function delManual(i: number) {
    if (!x) return;
    try { await api.settingListSet('strength_manual', x.manual.filter((_, k) => k !== i)); load(); } catch (e) { setErr(errText(e)); }
  }

  const shown = items.filter((r) => !hidden.includes(r.key));
  return (
    <section className="card" aria-label="Рекорды">
      <div className="between">
        <span className="label">Рекорды</span>
        <button type="button" className="btn2" aria-pressed={cfg} onClick={() => setCfg(!cfg)}>{cfg ? 'Готово' : 'Настроить'}</button>
      </div>
      {err && <div className="note-err">{err}</div>}
      {cfg && (
        <div className="inset">
          <span className="small" style={{ fontWeight: 700 }}>Какие рекорды показывать</span>
          <div className="row">
            {items.map((r) => <button key={r.key} type="button" className="pill" aria-pressed={!hidden.includes(r.key)} onClick={() => toggle(r.key)}>{hidden.includes(r.key) ? '○' : '✓'} {r.name}</button>)}
          </div>
          <span className="muted tiny">Скрытые рекорды Kouros продолжает считать и вернёт, если включишь снова.</span>
        </div>
      )}
      <div>
        {shown.map((r) => (
          <button key={r.key} type="button" className="list-row plain small" disabled={!r.id} onClick={() => r.id && onOpen(r.id)}>
            <span className="stack" style={{ gap: 0, flex: 1, minWidth: 0 }}>
              <span className="row" style={{ gap: 8 }}><b className="ellipsis">{r.name}</b>{r.tag && <span className="chip chip-off">{r.tag}</span>}</span>
              <span className="muted tiny">{r.sub}</span>
            </span>
            <b className="num">{r.val}</b>
          </button>
        ))}
        {shown.length === 0 && <div className="list-row muted small">Рекордов пока нет</div>}
      </div>
      {x && x.manual.length > 0 && (
        <div className="stack" style={{ gap: 0 }}>
          <span className="small" style={{ fontWeight: 700, paddingBottom: 4 }}>Внесено вручную</span>
          {x.manual.map((m, i) => (
            <div key={i} className="list-row small"><span style={{ flex: 1 }}>{m.exercise} · {n1(m.weightKg)} × {m.reps}</span><span className="muted tiny">{dayLabel(m.date)}</span><button type="button" className="xbtn" aria-label="Удалить" onClick={() => delManual(i)}>×</button></div>
          ))}
        </div>
      )}
      {form ? (
        <div className="inset">
          <span className="small" style={{ fontWeight: 700 }}>Новый рекорд в силовой</span>
          <div className="row" style={{ gap: 10, alignItems: 'flex-end' }}>
            <div className="field" style={{ flex: '1 1 220px' }}><label htmlFor="rx">Упражнение</label><input id="rx" style={{ width: '100%' }} value={form.exercise} onChange={(e) => setForm({ ...form, exercise: e.target.value })} /></div>
            <div className="field"><label htmlFor="rw">Вес, кг</label><input id="rw" inputMode="decimal" style={{ width: 90 }} value={form.weight} onChange={(e) => setForm({ ...form, weight: e.target.value })} /></div>
            <div className="field"><label htmlFor="rr">Повторений</label><input id="rr" inputMode="numeric" style={{ width: 90 }} value={form.reps} onChange={(e) => setForm({ ...form, reps: e.target.value })} /></div>
          </div>
          <div className="row"><button type="button" className="btn" onClick={addManual}>Сохранить</button><button type="button" className="btn2" onClick={() => setForm(null)}>Отмена</button></div>
        </div>
      ) : <button type="button" className="btn2" style={{ alignSelf: 'flex-start' }} onClick={() => setForm({ exercise: '', weight: '', reps: '' })}>+ Рекорд в силовой вручную</button>}
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>
        Бег — лучший отрезок внутри любой пробежки. Силовые — расчётный максимум на одно повторение (1ПМ, формула Эпли) из подходов, вписанных в разборе силовой: так сравнимы 60 × 10 и 70 × 5.
      </span>
    </section>
  );
}

// ---------- zones with pace, editable ----------

const ZN = ['Z1', 'Z2', 'Z3', 'Z4', 'Z5'];
const ZTITLE = ['Восстановление', 'Аэробная база', 'Темп', 'Порог', 'Максимум'];
/** What each zone gives, in plain words: [what it builds, why it matters, effect on running, how to train]. */
const ZINFO: [string, string, string, string][] = [
  [
    'Кровоток в мышцах и связках, восстановление после тяжёлых дней. Нагрузка на сердце и суставы минимальная.',
    'Позволяет двигаться и набирать объём, не добавляя усталости. Это активный отдых, а не «пустая» тренировка.',
    'Сам по себе форму почти не растит, но ускоряет восстановление — следующая качественная тренировка проходит лучше.',
    'Разминка и заминка, восстановительный бег или быстрая ходьба 20–40 минут на следующий день после тяжёлой работы. Дыхание совсем спокойное.',
  ],
  [
    'Сердце прокачивает больше крови за удар, в мышцах растёт сеть капилляров и митохондрий, тело лучше использует жир как топливо.',
    'Фундамент всей подготовки. Чем шире база, тем быстрее ты бежишь при том же пульсе и тем легче переносишь интенсивные тренировки.',
    'Со временем темп в этой зоне растёт — главный признак, что форма идёт вверх. Ещё растёт выносливость на длинных дистанциях.',
    '70–80% всего бегового времени. Ровные пробежки 40–90 минут и длинная раз в неделю. Ориентир — можешь говорить целыми предложениями. Если на подъёме пульс уходит выше верхней границы, переходи на шаг.',
  ],
  [
    'Умение долго держать уверенный темп, экономичность бега на скорости полумарафона и марафона.',
    'Полезна дозированно: даёт больше усталости, чем Z2, а прироста — меньше, чем Z4. Частая ошибка — бегать «лёгкие» пробежки здесь.',
    'Помогает держать соревновательный темп на длинных стартах, особенно перед полумарафоном и марафоном.',
    'Темповые отрезки 2–3 × 10–15 минут или 20–30 минут непрерывно, 1 раз в неделю и не чаще. Говорить можно короткими фразами.',
  ],
  [
    'Поднимает анаэробный порог — скорость, выше которой в мышцах быстро копится лактат и наступает «закисление».',
    'Самая результативная зона для скорости на 5–21 км. Но и самая утомительная: после неё нужно 1–2 дня лёгкой работы.',
    'Порог сдвигается к более быстрому темпу — ты дольше держишь высокую скорость без отказа ног.',
    'Интервалы 4–6 × 1–2 км или 3 × 2 км с отдыхом 2–3 минуты трусцой, раз в неделю, когда готовность выше 60%. Говорить можно 2–3 словами.',
  ],
  [
    'Максимальное потребление кислорода (МПК), мощность сердца, скорость и техника на высоком темпе.',
    'Даёт быстрый прирост, но тяжело переносится. После такой работы нужно 36–72 часа восстановления — так обычно рекомендуют после лабораторного теста.',
    'Растёт «потолок» — скорость на 1–5 км и запас для финишных ускорений.',
    'Короткие интервалы 5–8 × 2–3 минуты или 400–800 м с отдыхом равной длительности, не чаще раза в 7–10 дней и только при хорошей готовности. Говорить не получается.',
  ],
];
const ZC = ['#7FD3FF', '#3DDC97', '#F5B642', '#FF8A65', '#FF6B6B'];
/** Zone edges from max HR (60/70/80/87/93/100 %) and pace zones from the threshold pace — same as the backend. */
export const zonesFromMax = (m: number) => [0.6, 0.7, 0.8, 0.87, 0.93, 1].map((k) => Math.round(m * k));
export const paceFromThreshold = (t: number) => [1.736, 1.384, 1.161, 1.072, 1.003, 0.945].map((k) => Math.round(t * k));

function parsePace(v: string): number | null {
  const m = v.trim().match(/^(\d{1,2})[:.,](\d{2})$/);
  return m ? Number(m[1]) * 60 + Number(m[2]) : null;
}

export function ZonesCard({ today, onSaved }: { today: string; onSaved: () => void }) {
  const [p, setP] = useState<(AthleteProfile & { hrRestAuto?: number }) | null>(null);
  const [edit, setEdit] = useState<{ hr: string[]; pace: string[] } | null>(null);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [open, setOpen] = useState<number | null>(null);
  const load = () => api.profileGet().then(setP).catch((e) => setMsg({ ok: false, text: errText(e) }));
  useEffect(() => { load(); }, []);
  if (!p) return null;
  const pz = p.paceZones ?? paceFromThreshold(p.thresholdPaceS ?? 330);
  const age = p.zonesDate ? Math.round((Date.parse(today) - Date.parse(p.zonesDate)) / 86400000) : null;
  const hrLo = p.zones[0] - 10, hrHi = p.zones[5] + 5;

  function start() { setEdit({ hr: p!.zones.map(String), pace: pz.map((x) => fmtPace(x)) }); setMsg(null); }
  async function save(next: AthleteProfile) {
    try {
      const { hrRestAuto: _a, ...prof } = next as AthleteProfile & { hrRestAuto?: number };
      await api.profileSet(prof);
      setEdit(null); setMsg({ ok: true, text: 'Сохранено. Нагрузка и зоны тренировок пересчитаются.' }); load(); onSaved();
    } catch (e) { setMsg({ ok: false, text: errText(e) }); }
  }
  function saveEdit() {
    if (!edit || !p) return;
    const zones = edit.hr.map((v) => Number(v.replace(',', '.')));
    const pace = edit.pace.map(parsePace);
    if (zones.some((z) => !(z > 40 && z < 240))) { setMsg({ ok: false, text: 'Пульс — числа от 40 до 240' }); return; }
    if (pace.some((x) => x == null)) { setMsg({ ok: false, text: 'Темп — в виде 5:39' }); return; }
    save({ ...p, zones, hrMax: zones[5], paceZones: pace as number[] });
  }

  return (
    <section className="card" aria-label="Пульсовые и темповые зоны">
      <div className="between">
        <span className="label">Пульсовые и темповые зоны</span>
        {!edit && <button type="button" className="btn2" onClick={start}>Изменить</button>}
      </div>
      <span className="muted small">{p.zonesDate ? `Источник: тест · ${dayLabel(p.zonesDate)}` : 'Источник: профиль (без лабораторного теста зоны примерные)'}{age != null ? ` · ${plural(age, 'день', 'дня', 'дней')} назад` : ''}</span>
      <div className="ztable">
        <div className="zt th"><span>Зона</span><span>Пульс</span><span>Темп, /км</span><span>Шкала пульса</span><span /></div>
        {ZN.map((nm, i) => {
          const isOpen = open === i && !edit;
          const cells = (
            <>
              <span className="row" style={{ gap: 8, flexWrap: 'nowrap' }}><span className="dot" style={{ background: ZC[i] }} /><b className="num small">{nm}</b></span>
              {edit ? (
                <span className="row" style={{ gap: 4, flexWrap: 'nowrap' }}>
                  <input className="input sm" aria-label={`${nm}: пульс от`} value={edit.hr[i]} onChange={(e) => { const h = edit.hr.slice(); h[i] = e.target.value; setEdit({ ...edit, hr: h }); }} />
                  <input className="input sm" aria-label={`${nm}: пульс до`} value={edit.hr[i + 1]} onChange={(e) => { const h = edit.hr.slice(); h[i + 1] = e.target.value; setEdit({ ...edit, hr: h }); }} />
                </span>
              ) : <span className="num small">{p.zones[i]}–{p.zones[i + 1]}</span>}
              {edit ? (
                <span className="row" style={{ gap: 4, flexWrap: 'nowrap' }}>
                  <input className="input sm" aria-label={`${nm}: темп от`} value={edit.pace[i + 1]} onChange={(e) => { const x = edit.pace.slice(); x[i + 1] = e.target.value; setEdit({ ...edit, pace: x }); }} />
                  <input className="input sm" aria-label={`${nm}: темп до`} value={edit.pace[i]} onChange={(e) => { const x = edit.pace.slice(); x[i] = e.target.value; setEdit({ ...edit, pace: x }); }} />
                </span>
              ) : <span className="num small">{fmtPace(pz[i + 1])}–{fmtPace(pz[i])}</span>}
              <div className="zscale"><div style={{ left: `${((p.zones[i] - hrLo) / (hrHi - hrLo)) * 100}%`, width: `${((p.zones[i + 1] - p.zones[i]) / (hrHi - hrLo)) * 100}%`, background: ZC[i] }} /></div>
              {!edit && <span className="chev sm" style={{ transform: isOpen ? 'rotate(180deg)' : 'none' }}>⌄</span>}
            </>
          );
          return (
            <div key={nm} className="zwrap">
              {edit ? <div className="zt">{cells}</div> : (
                <button type="button" className="zt plain" aria-expanded={isOpen} onClick={() => setOpen(isOpen ? null : i)}>{cells}</button>
              )}
              {isOpen && (
                <div className="zinfo" style={{ borderColor: ZC[i] }}>
                  <b className="small">{nm} · {ZTITLE[i]} · {p.zones[i]}–{p.zones[i + 1]} уд/мин · {fmtPace(pz[i + 1])}–{fmtPace(pz[i])} /км</b>
                  {(['Что развивает', 'Почему важна', 'Как влияет на бег', 'Как тренироваться'] as const).map((h, k) => (
                    <p key={h} className="small"><b>{h}: </b>{ZINFO[i][k]}</p>
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </div>
      {!edit && <span className="muted tiny">Нажми на зону — откроется, что она развивает и как в ней тренироваться.</span>}
      {edit && (
        <div className="row">
          <button type="button" className="btn" onClick={saveEdit}>Сохранить</button>
          <button type="button" className="btn2" onClick={() => setEdit({ hr: zonesFromMax(p.hrMax).map(String), pace: paceFromThreshold(p.thresholdPaceS ?? 330).map((x) => fmtPace(x)) })}>Рассчитать от макс. пульса и порога</button>
          <button type="button" className="btn2" onClick={() => setEdit(null)}>Отмена</button>
        </div>
      )}
      {edit && <span className="muted tiny">Максимальный пульс = верх Z5. После сохранения Kouros пересчитает нагрузку, зоны и интенсивность всех тренировок.</span>}
      {msg && <div className={msg.ok ? 'note-ok' : 'note-err'}>{msg.text}</div>}
      {age != null && age >= 90 && (
        <div className="note-warn stack" style={{ gap: 4 }}>
          <b>{age >= 150 ? 'Пора на повторный тест' : `Через ${plural(Math.max(1, 150 - age), 'день', 'дня', 'дней')} — 5 месяцев с теста`}</b>
          <span>Обычно тест повторяют раз в 3–5 месяцев. Если результаты на тренировках выросли, пороги, скорее всего, сдвинулись вверх, и часть «лёгких» пробежек на самом деле ещё легче, чем кажется. После нового теста внеси зоны здесь.</span>
        </div>
      )}
    </section>
  );
}
