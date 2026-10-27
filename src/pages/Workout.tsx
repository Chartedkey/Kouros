import { useCallback, useEffect, useMemo, useState, type PointerEvent as RPointerEvent } from 'react';
import {
  api, errText, fmtDur, fmtKm, fmtPace, fmtWhen, plural, sportOf,
  type ActivityDetail, type AthleteProfile, type ResBand, type ExerciseItem, type KmSplit, type SetRow, type Shoe,
} from '../lib/api';
import { ExerciseCard, ExerciseInput, effectiveKg, useExerciseLib } from '../components/Exercises';
import { useDataChanged } from '../lib/events';
import { RouteMap } from './RouteMap';
import { OfpCard } from '../components/Ofp';
import { SportIcon, sportKey } from '../components/SportIcon';
import { StrengthCard } from '../components/Strength';
import { SportTiles } from '../components/SportTiles';

const ZONE_NAMES = ['Z1', 'Z2', 'Z3', 'Z4', 'Z5'];
const ZONE_COLORS = ['#7FD3FF', '#3DDC97', '#F5B642', '#FF8A65', '#FF6B6B'];
const RPE_SPORTS = ['strength', 'workout', 'yoga'];
const STRENGTH = ['strength', 'workout'];
const METHOD: Record<string, string> = { hr: 'по графику пульса', avg_hr: 'по среднему пульсу', rpe: 'по ощущениям × время', estimate: 'оценка по виду и времени' };

function zoneColor(hr: number | null, z: number[]): string {
  if (hr == null) return '#3A404A';
  for (let i = 4; i >= 0; i--) if (hr >= z[i]) return ZONE_COLORS[i];
  return '#6B737E';
}

/** Per-kilometre pulse bars coloured by zone, pace underneath. */
export function HrBars({ splits, zones }: { splits: KmSplit[]; zones: number[] }) {
  const hrs = splits.map((s) => s.hr ?? 0).filter((x) => x > 0);
  if (!hrs.length) return null;
  const lo = Math.min(...hrs) - 10, hi = Math.max(...hrs);
  return (
    <div className="stack" style={{ gap: 6 }}>
      <span className="small" style={{ fontWeight: 600 }}>По километрам · пульс и темп</span>
      <div className="kmbars">
        {splits.map((s) => {
          const pace = s.full ? s.timeS : s.distM ? s.timeS / (s.distM / 1000) : null;
          return (
            <div key={s.km} className="kmbar" title={`${s.km}-й км: пульс ${s.hr ?? '—'}, темп ${fmtPace(pace)}`}>
              <span className="num tiny">{s.hr ?? '—'}</span>
              <div style={{ height: `${s.hr ? 12 + ((s.hr - lo) / Math.max(1, hi - lo)) * 58 : 4}px`, background: zoneColor(s.hr, zones), borderRadius: 6, width: '100%' }} />
              <span className="muted tiny num">{fmtPace(pace)}</span>
              <span className="muted tiny">{s.full ? s.km : `${((s.distM ?? 0) / 1000).toFixed(1).replace('.', ',')}`}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** A simple line chart; the axis numbers are HTML, not SVG text. Shares a cursor with the map. */
function LineChart({ t, v, color, label, fmt, invert, cur, onCur }: {
  t: number[]; v: (number | null)[]; color: string; label: string; fmt: (x: number) => string; invert?: boolean;
  cur: number | null; onCur: (i: number | null) => void;
}) {
  const pts = t.map((x, i) => [x, v[i]] as [number, number | null]).filter((p): p is [number, number] => p[1] != null && isFinite(p[1]));
  if (pts.length < 5) return null;
  const W = 800, H = 130;
  const vals = pts.map((p) => p[1]).sort((a, b) => a - b);
  const lo = vals[Math.floor(vals.length * 0.02)], hi = vals[Math.floor(vals.length * 0.98)];
  const t0 = t[0], t1 = t[t.length - 1] || 1;
  const x = (s: number) => ((s - t0) / Math.max(1, t1 - t0)) * W;
  const y = (val: number) => {
    const f = (Math.min(hi, Math.max(lo, val)) - lo) / Math.max(1e-9, hi - lo);
    return 6 + (invert ? f : 1 - f) * (H - 12);
  };
  const line = pts.map((p) => `${x(p[0]).toFixed(1)},${y(p[1]).toFixed(1)}`).join(' ');
  const avg = vals.reduce((a, b) => a + b, 0) / vals.length;
  const cv = cur != null ? v[cur] : null;
  function move(e: RPointerEvent<HTMLDivElement>) {
    const r = e.currentTarget.getBoundingClientRect();
    const f = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
    onCur(nearestIdx(t, t0 + f * (t1 - t0)));
  }
  return (
    <div className="stack" style={{ gap: 6 }}>
      <div className="between small"><b>{label}{cv != null ? <span className="num" style={{ color, marginLeft: 8 }}>{fmt(cv)}</span> : null}</b><span className="muted tiny">среднее {fmt(avg)} · диапазон {fmt(invert ? hi : lo)}–{fmt(invert ? lo : hi)}</span></div>
      <div className="chart" onPointerMove={move} onPointerDown={move} onPointerLeave={() => onCur(null)}>
        <svg viewBox={`0 0 ${W} ${H}`} width="100%" height={H} preserveAspectRatio="none" aria-hidden="true">
          <line x1="0" x2={W} y1={y(avg)} y2={y(avg)} stroke="#3A404A" strokeDasharray="4 4" vectorEffect="non-scaling-stroke" />
          <polyline points={line} fill="none" stroke={color} strokeWidth="2" vectorEffect="non-scaling-stroke" />
          {cur != null && <line x1={x(t[cur])} x2={x(t[cur])} y1="0" y2={H} stroke="#f1f3f5" strokeOpacity="0.7" vectorEffect="non-scaling-stroke" />}
        </svg>
        <span className="axis-top tiny muted num">{fmt(invert ? lo : hi)}</span>
        <span className="axis-bot tiny muted num">{fmt(invert ? hi : lo)}</span>
      </div>
      <div className="between tiny muted num"><span>0:00</span><span>{fmtDur(t1 - t0)}</span></div>
    </div>
  );
}

function nearestIdx(t: number[], target: number): number {
  let lo = 0, hi = t.length - 1;
  while (lo < hi) {
    const m = (lo + hi) >> 1;
    if (t[m] < target) lo = m + 1; else hi = m;
  }
  return lo > 0 && Math.abs(t[lo - 1] - target) < Math.abs(t[lo] - target) ? lo - 1 : lo;
}

function smooth(v: (number | null)[], k: number): (number | null)[] {
  return v.map((_, i) => {
    let s = 0, n = 0;
    for (let j = Math.max(0, i - k); j <= Math.min(v.length - 1, i + k); j++) { const x = v[j]; if (x != null) { s += x; n++; } }
    return n ? s / n : null;
  });
}

const RPE_WORDS = ['', 'очень легко', 'очень легко', 'легко', 'легко', 'средне', 'средне', 'тяжело', 'тяжело', 'очень тяжело', 'предельно'];

function EffortCard({ d, onSaved }: { d: ActivityDetail; onSaved: () => void }) {
  const [rpe, setRpe] = useState<number | null>(d.activity.rpe);
  const [notes, setNotes] = useState(d.notes ?? '');
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const counts = RPE_SPORTS.includes(d.activity.sport);
  const dirty = rpe !== d.activity.rpe || notes !== (d.notes ?? '');
  async function save() {
    try {
      await api.activityUpdate(d.activity.id, rpe, notes.trim() || null);
      setMsg({ ok: true, text: counts && rpe ? 'Сохранено — нагрузка пересчитана по ощущениям.' : 'Сохранено.' });
      onSaved();
    } catch (e) { setMsg({ ok: false, text: errText(e) }); }
  }
  return (
    <section className="card" aria-label="Ощущения">
      <span className="label">Как ощущалось · 1–10</span>
      <div className="rpe" role="group" aria-label="Оценка тяжести">
        {Array.from({ length: 10 }, (_, i) => i + 1).map((n) => (
          <button key={n} type="button" aria-pressed={rpe === n} onClick={() => setRpe(rpe === n ? null : n)}
            style={rpe === n ? { background: n >= 8 ? '#FF6B6B' : n >= 5 ? '#F5B642' : '#3DDC97' } : undefined}>{n}</button>
        ))}
      </div>
      <span className="muted small">{rpe ? `${rpe} — ${RPE_WORDS[rpe]}` : 'Не оценено'}</span>
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>
        {counts
          ? 'Для силовых, физиотерапии и йоги пульс почти не растёт, поэтому нагрузка считается по ощущениям: минуты × оценка × 0,45. Без оценки — по пульсу.'
          : 'Для бега и ходьбы нагрузка считается по пульсу; оценка сохранится для тренера.'}
      </span>
      <div className="field">
        <label htmlFor="notes">Заметки</label>
        <textarea id="notes" className="textarea" rows={3} value={notes} placeholder="Самочувствие, что болело, условия…" onChange={(e) => setNotes(e.target.value)} />
      </div>
      <div className="row">
        <button type="button" className="btn" disabled={!dirty} onClick={save}>Сохранить</button>
        {msg && <span className={msg.ok ? 'small' : 'small note-err'} style={msg.ok ? { color: 'var(--green)' } : undefined}>{msg.text}</span>}
      </div>
    </section>
  );
}

// ---------- the page ----------

/** Pace in s/km from the speed stream, or from distance when there is no speed. */
function paceSeries(c: NonNullable<ActivityDetail['charts']>): (number | null)[] | null {
  if (c.speed.some((x) => x != null)) return smooth(c.speed.map((v) => (v != null && v > 0.5 ? 1000 / v : null)), 5);
  if (!c.dist.some((x) => x != null)) return null;
  const k = 5;
  return smooth(c.t.map((_, i) => {
    const a = Math.max(0, i - k), b = Math.min(c.t.length - 1, i + k);
    const d0 = c.dist[a], d1 = c.dist[b], dt = c.t[b] - c.t[a];
    if (d0 == null || d1 == null || dt <= 0) return null;
    const v = (d1 - d0) / dt;
    return v > 0.5 ? 1000 / v : null;
  }), 3);
}

function Readout({ c, i, pace, zones }: { c: NonNullable<ActivityDetail['charts']>; i: number | null; pace: (number | null)[] | null; zones: number[] }) {
  if (i == null) return <div className="readout muted small">Наведи на график или маршрут либо двигай ползунок — здесь появятся данные этого момента.</div>;
  const hr = c.hr[i] ?? null;
  const zi = hr == null ? -1 : zones.slice(0, 5).reduce((acc, z, k) => (hr >= z ? k : acc), -1);
  const items: [string, string, string?][] = [
    ['время', fmtDur(c.t[i] - c.t[0])],
    ['дистанция', c.dist[i] != null ? fmtKm(c.dist[i], 2) : '—'],
    ['пульс', hr != null ? `${Math.round(hr)}${zi >= 0 ? ` · ${ZONE_NAMES[zi]}` : ''}` : '—', zi >= 0 ? ZONE_COLORS[zi] : undefined],
    ['темп', pace?.[i] != null ? `${fmtPace(pace[i])}/км` : '—'],
    ['высота', c.alt[i] != null ? `${Math.round(c.alt[i]!)} м` : '—'],
    ['каденс', c.cad[i] != null ? `${Math.round(c.cad[i]!)}` : '—'],
  ];
  return (
    <div className="readout">
      {items.map(([l, v, col]) => (
        <span key={l} className="stack" style={{ gap: 0 }}><span className="muted tiny">{l}</span><b className="num small" style={col ? { color: col } : undefined}>{v}</b></span>
      ))}
    </div>
  );
}

function ShoePick({ d, onSaved }: { d: ActivityDetail; onSaved: () => void }) {
  const [list, setList] = useState<Shoe[] | null>(null);
  useEffect(() => { api.shoesGet().then(setList).catch(() => setList([])); }, []);
  if (!list || list.length === 0) return null;
  const val = d.shoeId ?? '';
  return (
    <label className="row small" style={{ gap: 10 }}>
      <span className="muted">Кроссовки</span>
      <select className="select" value={val} onChange={(e) => api.activitySetShoe(d.activity.id, e.target.value || null).then(onSaved).catch(() => {})}>
        <option value="">не выбраны — пробег не считается</option>
        {list.filter((s) => !s.retired || s.id === val).map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}
        <option value="none">другие / без кроссовок</option>
      </select>
    </label>
  );
}

export function Workout({ id, onBack, backLabel = 'Тренировки' }: { id: string; onBack: () => void; backLabel?: string }) {
  const [d, setD] = useState<ActivityDetail | null>(null);
  const [prof, setProf] = useState<AthleteProfile | null>(null);
  const [online, setOnline] = useState(false);
  const [cur, setCur] = useState<number | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [delAsk, setDelAsk] = useState(false);
  const load = useCallback(() => { api.activityGet(id).then((v) => { setD(v); setErr(null); }).catch((e) => setErr(errText(e))); }, [id]);
  useEffect(load, [load]);
  useEffect(() => {
    api.profileGet().then(setProf).catch(() => {});
    api.mapCacheInfo().then((m) => setOnline(m.enabled)).catch(() => {});
  }, []);
  useDataChanged(load);
  useEffect(() => { window.scrollTo(0, 0); }, [id]);
  const c = d?.charts ?? null;
  const pace = useMemo(() => (c ? paceSeries(c) : null), [c]);
  const hrS = useMemo(() => (c ? smooth(c.hr, 2) : []), [c]);

  const back = <button type="button" className="backbtn" onClick={onBack}>← {backLabel}</button>;
  if (err) return <>{back}<div className="note-err">{err}</div></>;
  if (!d) return <>{back}<div className="muted">Загружаю…</div></>;

  const a = d.activity;
  const sp = sportOf(a.sport);
  const zones = prof?.zones ?? [110, 129, 147, 160, 171, 184];
  const moving = a.movingS ?? a.elapsedS;
  const paceOk = !!a.distanceM && a.distanceM > 300 && !!moving;
  const isStrength = STRENGTH.includes(a.sport) || d.sets.length > 0;
  const showRoute = !!c && !RPE_SPORTS.includes(a.sport) && c.lat.filter((x) => x != null).length >= 10;
  const zoneS = d.zoneS ?? [];
  const zTotal = zoneS.reduce((x, y) => x + y, 0);
  const hrOver = Boolean(prof && a.maxHr && a.maxHr > prof.hrMax + 2);
  const hasHr = !!c && c.hr.some((x) => x != null);
  const hasCharts = !!c && (hasHr || !!pace);
  const n = c ? c.t.length : 0;
  const slider = c && n > 1 && (
    <input type="range" className="range" min={0} max={n - 1} value={cur ?? 0} aria-label="Момент тренировки"
      onChange={(e) => setCur(Number(e.target.value))} />
  );

  return (
    <>
      <div className="page-head">
        <div className="stack" style={{ gap: 6, minWidth: 0, flex: '1 1 480px' }}>
          {back}
          <div className="row" style={{ gap: 16, flexWrap: 'nowrap', minWidth: 0 }}>
            <SportIcon sport={sportKey(a)} size={64} />
            <div className="stack" style={{ gap: 6, minWidth: 0 }}>
          <h1 className="ellipsis" style={{ maxWidth: '100%' }}>{a.name || sp.name}</h1>
          <p className="subtitle">
            {sportKey(a) === 'treadmill' ? 'Беговая дорожка' : sp.name} · {fmtWhen(a.startUtc)} · {d.sourceName ?? (a.source === 'strava_api' ? 'Strava' : a.source === 'strava_archive' ? 'архив Strava' : 'файл')}{!d.sourceName && a.device ? ` · ${a.device}` : ''}
          </p>
            </div>
          </div>
          {d.detailText && <span className="muted tiny">Данные: {d.detailText}</span>}
        </div>
        <div className="stack" style={{ gap: 0, alignItems: 'flex-end' }}>
          <span className="muted tiny">нагрузка</span>
          <span className="num" style={{ fontSize: 32, fontWeight: 700, color: '#4FA3FF' }}>{d.load ?? '—'}</span>
          <span className="muted tiny">{d.loadMethod ? METHOD[d.loadMethod] ?? d.loadMethod : 'ещё не посчитана'}</span>
        </div>
      </div>

      {d.hidden === 1 && <div className="note-warn">Эта тренировка удалена — Kouros её не считает. Вернуть можно в «Все тренировки» → «Удалённые».</div>}
      {(d.hidden === 2 || d.hidden === 3) && <div className="note-warn">Эта запись сейчас не считается — выбрана другая запись этой же тренировки.</div>}
      {(d.alternatives ?? []).length > 0 && (
        <section className="card" aria-label="Другие записи этой тренировки">
          <span className="label">{d.dupState === 'pending' ? 'Ждёт выбора · есть вторая запись' : 'Есть другая запись этой тренировки'}</span>
          {d.alternatives!.map((x) => (
            <div key={x.id} className="list-row small" style={{ flexWrap: 'wrap' }}>
              <span style={{ flex: 1, minWidth: 0 }}><b>{x.sourceName}</b> <span className="muted">· {x.min} мин{x.km ? ` · ${String(x.km).replace('.', ',')} км` : ''}{x.avgHr ? ` · пульс ${x.avgHr}` : ''} · {x.detail}</span></span>
              {!x.counted && <button type="button" className="btn2" onClick={() => api.workoutChoose(x.id).then(load)}>Взять её вместо этой</button>}
              {x.counted && <span className="chip chip-ok">считается</span>}
            </div>
          ))}
          <span className="muted tiny">Ничего не удаляется — переключаться можно туда и обратно. Нагрузка, мышцы, восстановление и рекорды пересчитаются.</span>
        </section>
      )}

      <SportTiles a={a} x={{ weightLiftedKg: d.weightLiftedKg, setsCount: d.sets.filter((r) => r.kind === 'active').length + (d.ofp ?? []).reduce((n, b) => n + b.rounds * b.items.filter((it) => !it.skip).length, 0) || null }} />

      {d.muscle && (
        <section className="card" aria-label="Мышечная нагрузка и восстановление">
          <div className="between">
            <span className="label">Мышцы и восстановление</span>
            {d.muscle.rec && <span className="chip" style={{ background: '#1B1F25', color: '#F5B642' }}>восстановление ≈ {d.muscle.rec.hours} ч</span>}
          </div>
          <div className="row" style={{ gap: '8px 24px', alignItems: 'baseline', flexWrap: 'wrap' }}>
            <span className="small"><span className="muted">мышечная нагрузка </span><b className="num">{d.muscle.total}</b></span>
            {(d.muscle.up ?? 0) > 0 && <span className="small"><span className="muted">вверх </span><b className="num">{d.muscle.up} м</b></span>}
            {(d.muscle.down ?? 0) > 0 && <span className="small"><span className="muted">вниз </span><b className="num">{d.muscle.down} м</b></span>}
            {d.muscle.rec && <span className="small muted">сердце {d.muscle.rec.heartH} ч · мышцы {d.muscle.rec.muscleH} ч</span>}
          </div>
          {(d.muscle.free ?? 0) > 0 && (
            <span className="small muted" style={{ lineHeight: 1.5 }}>
              {d.muscle.free === d.muscle.total ? 'Упражнения не внесены' : 'Часть подходов без узнанных упражнений'} — {d.muscle.free} из {d.muscle.total} учтены в общей мышечной нагрузке и восстановлении, но не на карте мышц. Внеси подходы или ОФП ниже.
            </span>
          )}
          {d.muscle.bad && <span className="small" style={{ color: '#F5B642', lineHeight: 1.5 }}>Высота записана со сбоем (скачки датчика) — для мышечной нагрузки взят твой обычный рельеф на таких тренировках.</span>}
          <div className="row" style={{ gap: 8, flexWrap: 'wrap' }}>
            {d.muscle.groups.slice(0, 6).map((g) => <span key={g.key} className="chip" style={{ background: '#1B1F25' }}>{g.name} · {g.pct}%</span>)}
          </div>
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>Мышечная нагрузка — отдельная от пульсовой: дистанция и темп, подъёмы и особенно спуски (они тормозят квадрицепсы и нагружают колени), подходы по упражнениям. Она не складывается с главной нагрузкой. Восстановление — по более долгому из двух; на «Сегодня» оно уточняется по утренней готовности.</span>
        </section>
      )}

      {hrOver && <div className="note-warn">Максимум {Math.round(a.maxHr!)} выше максимального пульса из теста ({prof!.hrMax}). Если это не сбой датчика — возможно, тест занизил максимум.</div>}
      {d.description && <section className="card"><span className="label">Описание</span><span style={{ whiteSpace: 'pre-wrap', lineHeight: 1.5 }}>{d.description}</span></section>}

      {showRoute && c && (
        <section className="card" aria-label="Маршрут">
          <div className="between">
            <span className="label">Маршрут</span>
            <span className="row tiny muted" style={{ gap: 10 }}>
              {hasHr && ZONE_NAMES.map((z, k) => <span key={z} className="row" style={{ gap: 4 }}><span className="dot" style={{ background: ZONE_COLORS[k] }} />{z}</span>)}
              <span>● старт · ○ финиш</span>
            </span>
          </div>
          <RouteMap lat={c.lat} lng={c.lng} hr={c.hr} zones={zones} cur={cur} onCur={setCur} online={online} />
          <Readout c={c} i={cur} pace={pace} zones={zones} />
          {slider}
          {!online && <span className="muted tiny">Карта выключена — трек показан без подложки. Включить: Настройки → Вид и карта.</span>}
        </section>
      )}

      <div className="grid2">
        <div className="stack" style={{ gap: 20 }}>
          {hasCharts && c && (
            <section className="card" aria-label="Графики">
              <span className="label">Графики</span>
              {!showRoute && <><Readout c={c} i={cur} pace={pace} zones={zones} />{slider}</>}
              {hasHr && <LineChart t={c.t} v={hrS} color="#FF6B6B" label="Пульс" fmt={(x) => String(Math.round(x))} cur={cur} onCur={setCur} />}
              {pace && !isStrength && <LineChart t={c.t} v={pace} color="#4FA3FF" label="Темп, мин/км" fmt={fmtPace} invert cur={cur} onCur={setCur} />}
              {!isStrength && c.alt.some((x) => x != null) && <LineChart t={c.t} v={c.alt} color="#9AA3AE" label="Высота, м" fmt={(x) => String(Math.round(x))} cur={cur} onCur={setCur} />}
            </section>
          )}
          {d.splits.length > 0 && !isStrength && (
            <section className="card" aria-label="Отрезки">
              <HrBars splits={d.splits} zones={zones} />
              <div className="table">
                <div className="tr tr-km th"><span>Км</span><span className="r">Время</span><span className="r">Темп</span><span className="r">Пульс</span></div>
                {d.splits.map((s) => {
                  const p = s.full ? s.timeS : s.distM ? s.timeS / (s.distM / 1000) : null;
                  return (
                    <div key={s.km} className="tr tr-km small">
                      <span className="num">{s.full ? s.km : `${s.km} (${Math.round(s.distM ?? 0)} м)`}</span>
                      <span className="r num">{fmtDur(s.timeS)}</span>
                      <span className="r num">{fmtPace(p)}</span>
                      <span className="r num" style={{ color: zoneColor(s.hr, zones) }}>{s.hr ?? '—'}</span>
                    </div>
                  );
                })}
              </div>
            </section>
          )}
          {isStrength && <StrengthCard key={d.sets.length + ':' + a.id} d={d} onSaved={load} />}
          <OfpCard key={'ofp:' + a.id + ':' + (d.ofp?.length ?? 0)} d={d} onSaved={load} />
          {!c && !isStrength && <section className="card"><span className="muted small">У этой тренировки нет графиков — только итоги.{a.hasStreams ? '' : ' Strava догрузит их при следующей синхронизации, если они есть.'}</span></section>}
        </div>
        <div className="stack" style={{ gap: 20 }}>
          <EffortCard key={a.id + String(a.rpe)} d={d} onSaved={load} />
          {['run', 'walk', 'hike'].includes(a.sport) && <section className="card"><ShoePick d={d} onSaved={load} /></section>}
          {zTotal > 0 && (
            <section className="card" aria-label="Зоны">
              <span className="label">Время в зонах</span>
              <div style={{ display: 'flex', height: 14, borderRadius: 999, overflow: 'hidden', background: '#22262D' }}>
                {zoneS.slice(1).map((s, i) => <div key={i} style={{ width: `${(s / zTotal) * 100}%`, background: ZONE_COLORS[i] }} />)}
              </div>
              <div>
                {ZONE_NAMES.map((nm, i) => (
                  <div key={nm} className="list-row small">
                    <span className="row" style={{ gap: 8, flex: 1 }}><span className="dot" style={{ background: ZONE_COLORS[i] }} /><b>{nm}</b><span className="muted tiny">{zones[i]}–{zones[i + 1]}</span></span>
                    <span className="num">{fmtDur(zoneS[i + 1] ?? 0)}</span>
                    <span className="muted tiny num" style={{ width: 40, textAlign: 'right' }}>{Math.round(((zoneS[i + 1] ?? 0) / zTotal) * 100)}%</span>
                  </div>
                ))}
                {zoneS[0] > 0 && <div className="list-row small muted"><span style={{ flex: 1 }}>Ниже Z1</span><span className="num">{fmtDur(zoneS[0])}</span><span className="tiny num" style={{ width: 40, textAlign: 'right' }}>{Math.round((zoneS[0] / zTotal) * 100)}%</span></div>}
              </div>
            </section>
          )}
          {a.totalSets != null && !d.sets.length && <span className="muted small">{plural(a.totalSets, 'подход', 'подхода', 'подходов')} по данным Strava.</span>}
          {d.hidden !== 1 && (
            <section className="card" aria-label="Удалить тренировку">
              {delAsk ? (
                <>
                  <span className="small">Удалить «{a.name || sp.name}»? Нагрузка недели, восстановление, рекорды и километраж обуви пересчитаются. Тренировка уйдёт в «Удалённые» — её можно вернуть.</span>
                  <div className="row" style={{ gap: 10 }}>
                    <button type="button" className="btn2 btn-danger" onClick={() => api.workoutsDelete([a.id]).then(onBack).catch((e) => setErr(errText(e)))}>Удалить</button>
                    <button type="button" className="linkbtn small" onClick={() => setDelAsk(false)}>Отмена</button>
                  </div>
                </>
              ) : (
                <button type="button" className="linkbtn small" style={{ alignSelf: 'flex-start', color: 'var(--muted)' }} onClick={() => setDelAsk(true)}>Удалить тренировку</button>
              )}
            </section>
          )}
        </div>
      </div>
    </>
  );
}
