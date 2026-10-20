import { StressCard, stressColor } from '../components/Stress';
import { Meds } from './Meds';
import { Vital, pointsOf, fmtPts } from '../components/Readiness';
import { Fragment, useCallback, useEffect, useMemo, useState, type PointerEvent as RPointerEvent, type ReactNode } from 'react';
import { api, errText, fmtDur, fmtKm, plural, sportOf, type Band, type HealthDay, type HealthNight, type HealthOverview, type HealthSummary, type Pt, type ReadinessDay, type StressDay } from '../lib/api';
import { useDataChanged } from '../lib/events';

type Sub = 'overview' | 'history' | 'meds';

const n0 = (x: number | null | undefined) => (x == null ? '—' : Math.round(x).toLocaleString('ru-RU'));
const n1 = (x: number | null | undefined) => (x == null ? '—' : x.toFixed(1).replace('.', ','));
const dayLabel = (d: string) => new Date(d + 'T12:00:00').toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' });
const WD = ['Пн', 'Вт', 'Ср', 'Чт', 'Пт', 'Сб', 'Вс'];
const wdOf = (d: string) => WD[(new Date(d + 'T12:00:00').getDay() + 6) % 7];
function hm(s: number) {
  const h = Math.floor(s / 3600), m = Math.round((s % 3600) / 60);
  return `${h}:${String(m).padStart(2, '0')}`;
}
function hmWords(min: number) {
  const h = Math.floor(min / 60), m = Math.round(min % 60);
  return h ? `${h} ч ${m} мин` : `${m} мин`;
}
const signed = (min: number) => `${min >= 0 ? '+' : '−'}${hm(Math.abs(min) * 60)}`;

/** Local clock for a unix time, using the athlete's time zone from the backend. */
function clockAt(t: number, tz: number) {
  const m = Math.floor((((t + tz) % 86400) + 86400) % 86400 / 60);
  return `${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`;
}

const STAGE: Record<string, [string, string, number]> = {
  awake: ['Бодрствование', '#FF8A65', 0], rem: ['REM', '#C792EA', 1], core: ['Лёгкий', '#7FD3FF', 2], deep: ['Глубокий', '#5B7CFA', 3],
};
const LVL = ['#3DDC97', '#F5B642', '#FF6B6B'];

export function Health({ onSettings, onOpenWorkout, day, onDayUsed, initialSub, onSubUsed }: { onSettings: () => void; onOpenWorkout?: (id: string) => void; day?: string | null; onDayUsed?: () => void; initialSub?: Sub | null; onSubUsed?: () => void }) {
  const [sub, setSub] = useState<Sub>(day ? 'history' : initialSub ?? 'overview');
  useEffect(() => { if (day) setSub('history'); }, [day]);
  useEffect(() => { if (initialSub) { setSub(initialSub); onSubUsed?.(); } }, [initialSub]);
  const [h, setH] = useState<HealthSummary | null>(null);
  const [ov, setOv] = useState<HealthOverview | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(() => {
    api.healthSummary().then((v) => { setH(v); setErr(null); }).catch((e) => setErr(errText(e)));
    api.healthOverview().then(setOv).catch(() => {});
  }, []);
  useEffect(load, [load]);
  useDataChanged(load);
  const [medsOn, setMedsOn] = useState(true);
  useEffect(() => { api.medsEnabled().then(setMedsOn).catch(() => {}); }, []);
  useEffect(() => { if (!medsOn && sub === 'meds') setSub('overview'); }, [medsOn, sub]);

  const empty = ov && ov.stats.rows === 0;
  return (
    <>
      <div className="page-head">
        <div className="stack">
          <h1>Здоровье</h1>
          <p className="subtitle">Сон, сердце и ранние сигналы — всё относительно твоей личной нормы</p>
        </div>
      </div>
      <nav className="subtabs" aria-label="Разделы здоровья">
        {([['overview', 'Обзор'], ['history', 'История'], ...(medsOn ? [['meds', 'Витамины и лекарства']] : [])] as [Sub, string][]).map(([id, n]) => (
          <button key={id} type="button" aria-pressed={sub === id} onClick={() => setSub(id)}>{n}</button>
        ))}
      </nav>
      {err && <div className="note-err">{err}</div>}
      {empty && (
        <section className="card" style={{ alignItems: 'flex-start' }}>
          <span className="label">Данных пока нет</span>
          <span style={{ lineHeight: 1.55, color: 'var(--text-2)' }}>
            Сохрани ключ freddy и нажми «Загрузить» в Настройках → Синхронизация. Первая загрузка заберёт всю доступную историю.
          </span>
          <button type="button" className="btn" onClick={onSettings}>Открыть настройки</button>
        </section>
      )}
      {!empty && !h && !err && <div className="muted">Считаю…</div>}
      {!empty && h && sub === 'overview' && <Overview h={h} reload={load} />}
      {!empty && h && sub === 'history' && <History h={h} ov={ov} onOpen={onOpenWorkout} day={day} onDayUsed={onDayUsed} />}
      {sub === 'meds' && medsOn && <Meds />}
      <p className="muted tiny" style={{ textAlign: 'center', margin: 0 }}>Данные Apple Watch и весов через Apple Health. Не является медицинской диагностикой.</p>
    </>
  );
}

// ---------- charts ----------

/** Source switches of a data group as chart marks. */
function marksOf(h: HealthSummary, group: string) {
  return (h.switches ?? []).filter((x) => x.group === group).map((x) => ({ date: x.date, label: `источник ${x.from} → ${x.to}` }));
}

export function LineBand({ pts, band, color, height = 110, fmt, invertGood, sel, onPick, trend, marks }: {
  pts: Pt[]; band: Band; color: string; height?: number; fmt: (v: number) => string; invertGood?: boolean;
  sel?: string | null; onPick?: (d: string) => void; trend?: Pt[]; marks?: { date: string; label: string }[];
}) {
  if (pts.length < 2) return <span className="muted small">Мало данных — нужно хотя бы несколько дней.</span>;
  const W = 600, H = height;
  const vals = pts.map((p) => p[1]).concat(band ? [band[1], band[2]] : []);
  const lo = Math.min(...vals) * 0.97, hi = Math.max(...vals) * 1.03;
  const x = (i: number) => (i / (pts.length - 1)) * W;
  const y = (v: number) => H - 6 - ((v - lo) / Math.max(1e-6, hi - lo)) * (H - 12);
  const line = pts.map((p, i) => `${x(i).toFixed(1)},${y(p[1]).toFixed(1)}`).join(' ');
  const out = (v: number) => band && (invertGood ? v > band[2] : v < band[1]);
  // x by calendar day, so gaps between measurements keep their real width.
  const t0 = Date.parse(pts[0][0]), t1 = Date.parse(pts[pts.length - 1][0]);
  const xd = (d: string) => (t1 > t0 ? ((Date.parse(d) - t0) / (t1 - t0)) * W : x(0));
  const trendLine = trend && trend.length > 1 ? trend.map((p) => `${xd(p[0]).toFixed(1)},${y(p[1]).toFixed(1)}`).join(' ') : null;
  const dotSize = pts.length <= 40 ? 8 : pts.length <= 120 ? 6 : 0;
  function pick(e: RPointerEvent<HTMLDivElement>) {
    if (!onPick) return;
    const r = e.currentTarget.getBoundingClientRect();
    const f = (e.clientX - r.left) / r.width;
    let best = pts[0][0], bd = Infinity;
    for (const p of pts) { const d = Math.abs(xd(p[0]) / W - f); if (d < bd) { bd = d; best = p[0]; } }
    onPick(best);
  }
  const selPt = sel ? pts.find((p) => p[0] === sel) : undefined;
  return (
    <div className="stack" style={{ gap: 4 }}>
      <div className="chart" style={{ cursor: onPick ? 'pointer' : 'default' }} onPointerDown={pick}>
        <svg viewBox={`0 0 ${W} ${H}`} width="100%" height={H} preserveAspectRatio="none" aria-hidden="true">
          {band && <rect x="0" width={W} y={y(band[2])} height={Math.max(1, y(band[1]) - y(band[2]))} fill={color} opacity="0.12" />}
          {sel && <line x1={xd(sel)} x2={xd(sel)} y1="0" y2={H} stroke="#f1f3f5" strokeOpacity="0.6" vectorEffect="non-scaling-stroke" />}
          {(marks ?? []).filter((m) => m.date > pts[0][0] && m.date <= pts[pts.length - 1][0]).map((m) => (
            <line key={m.date} x1={xd(m.date)} x2={xd(m.date)} y1="0" y2={H} stroke="#F5B642" strokeDasharray="4 4" strokeOpacity="0.8" vectorEffect="non-scaling-stroke"><title>{m.label}</title></line>
          ))}
          {trendLine
            ? <polyline points={trendLine} fill="none" stroke={color} strokeWidth="2.5" vectorEffect="non-scaling-stroke" />
            : <polyline points={pts.map((p) => `${xd(p[0]).toFixed(1)},${y(p[1]).toFixed(1)}`).join(' ')} fill="none" stroke={color} strokeWidth="2" vectorEffect="non-scaling-stroke" />}
        </svg>
        {/* Dots are HTML so they stay round however wide the chart is stretched. */}
        {pts.map((p) => {
          const on = p[0] === sel;
          if (!on && dotSize === 0) return null;
          const size = on ? 11 : dotSize;
          return (
            <span key={p[0]} className="cdot" title={`${dayLabel(p[0])}: ${fmt(p[1])}`}
              style={{ left: `${(xd(p[0]) / W) * 100}%`, top: y(p[1]), width: size, height: size, background: out(p[1]) ? '#F5B642' : color, boxShadow: on ? '0 0 0 2px #f1f3f5' : undefined }} />
          );
        })}
        {selPt && <span className="chart-val num tiny" style={{ left: `${(xd(selPt[0]) / W) * 100}%` }}>{fmt(selPt[1])}</span>}
        <span className="axis-top tiny muted num">{fmt(hi)}</span>
        <span className="axis-bot tiny muted num">{fmt(lo)}</span>
      </div>
      {(marks ?? []).filter((m) => m.date > pts[0][0] && m.date <= pts[pts.length - 1][0]).map((m) => <span key={m.date} className="tiny" style={{ color: '#F5B642' }}>┆ {dayLabel(m.date)}: {m.label}</span>)}
      <div className="between tiny muted"><span>{dayLabel(pts[0][0])}</span><span>{dayLabel(new Date((t0 + t1) / 2).toISOString().slice(0, 10))}</span><span>{dayLabel(pts[pts.length - 1][0])}</span></div>
    </div>
  );
}

function SleepBars({ nights, needMin, count, sel, onPick }: { nights: HealthNight[]; needMin: number; count: number; sel?: string | null; onPick?: (d: string) => void }) {
  const list = nights.slice(-count);
  if (!list.length) return <span className="muted small">Нет ночей с данными</span>;
  const max = Math.max(needMin * 60 * 1.15, ...list.map((n) => n.totalS));
  return (
    <div className="stack" style={{ gap: 4 }}>
      <div className="sleepbars" style={{ height: 130 }}>
        <div className="needline" style={{ bottom: `${((needMin * 60) / max) * 100}%` }}><span className="tiny muted">{hm(needMin * 60)}</span></div>
        {list.map((n) => (
          <div key={n.date} className={`sbar${n.date === sel ? ' sel' : ''}`} style={onPick ? { cursor: 'pointer' } : undefined} onClick={onPick ? () => onPick(n.date) : undefined} title={`${dayLabel(n.date)}: ${hm(n.totalS)} · глубокий ${hm(n.deepS)} · REM ${hm(n.remS)}`}>
            {count <= 14 && <span className="num tiny">{hm(n.totalS)}</span>}
            <div className="sstack" style={{ height: `${(n.totalS / max) * 100}%` }}>
              <div style={{ flex: n.remS, background: STAGE.rem[1] }} />
              <div style={{ flex: n.coreS, background: STAGE.core[1] }} />
              <div style={{ flex: n.deepS, background: STAGE.deep[1] }} />
            </div>
            {count <= 14 && <span className="muted tiny">{wdOf(n.date)}</span>}
          </div>
        ))}
      </div>
      {count > 14 && <div className="between tiny muted"><span>{dayLabel(list[0].date)}</span><span>{dayLabel(list[list.length - 1].date)}</span></div>}
    </div>
  );
}

function Hypnogram({ hyp, tz }: { hyp: NonNullable<HealthSummary['hypnogram']>; tz: number }) {
  const span = Math.max(1, hyp.end - hyp.start);
  const ticks = 5;
  return (
    <div className="stack" style={{ gap: 4 }}>
      <div className="hyp">
        <div className="hyp-labels tiny muted">{Object.values(STAGE).map(([n]) => <span key={n}>{n}</span>)}</div>
        <svg viewBox="0 0 1000 120" width="100%" height="120" preserveAspectRatio="none" aria-label="Стадии сна">
          {[0, 1, 2, 3].map((r) => <line key={r} x1="0" x2="1000" y1={r * 30 + 15} y2={r * 30 + 15} stroke="#1c2026" vectorEffect="non-scaling-stroke" />)}
          {hyp.segs.map(([st, a, b], i) => {
            const s = STAGE[st] ?? STAGE.core;
            return <rect key={i} x={((a - hyp.start) / span) * 1000} width={Math.max(1, ((b - a) / span) * 1000)} y={s[2] * 30 + 4} height="22" rx="3" fill={s[1]} />;
          })}
        </svg>
      </div>
      <div className="between tiny muted num" style={{ paddingLeft: 96 }}>
        {Array.from({ length: ticks }, (_, i) => <span key={i}>{clockAt(hyp.start + (span * i) / (ticks - 1), tz)}</span>)}
      </div>
    </div>
  );
}

// ---------- overview ----------

function Overview({ h, reload }: { h: HealthSummary; reload: () => void }) {
  const night = h.night;
  const needS = h.need.totalMin * 60;
  const alarms = h.signals.filter((s) => s.level > 0);
  const bed = h.hypnogram ? clockAt(h.hypnogram.start, h.tz) : null;
  const wake = h.hypnogram ? clockAt(h.hypnogram.end, h.tz) : null;
  const pctNeed = night ? Math.round((night.totalS / (h.need.baseMin * 60)) * 100) : null;
  const eff = night ? Math.round((night.totalS / (night.totalS + night.awakeS)) * 100) : null;
  const deepPct = night ? night.deepS / night.totalS : 0, remPct = night ? night.remS / night.totalS : 0;
  const stagesWord = !night ? '—' : deepPct < 0.12 ? 'мало глубокого' : remPct < 0.17 ? 'мало REM' : 'в норме';
  const lastOf = <T,>(a: T[]): T | undefined => a[a.length - 1];
  const hrvNow = lastOf(h.hrv.last30), rhrNow = lastOf(h.rhr.last30);
  const hrvWord = !hrvNow || !h.hrv.band ? '' : hrvNow[1] > h.hrv.band[2] ? 'выше нормы' : hrvNow[1] < h.hrv.band[1] ? 'ниже нормы' : 'в норме';
  const rhrWord = !rhrNow || !h.rhr.band ? '' : rhrNow[1] > h.rhr.band[2] ? 'выше нормы' : rhrNow[1] < h.rhr.band[1] ? 'ниже нормы' : 'в норме';
  const vo2 = lastOf(h.vo2.series);
  const w = h.body.weighings[0];

  return (
    <>
      <section className={`card sigbar${alarms.length ? ' warn' : ''}`} aria-label="Ранние сигналы">
        <span className="stack" style={{ gap: 2, flex: '1 1 320px' }}>
          <b>{alarms.length ? `Ранние сигналы: ${alarms.map((a) => a.name.toLowerCase()).join(', ')} вне нормы` : 'Ранние сигналы: всё спокойно'}</b>
          <span className="muted small">{alarms.length >= 2 ? 'Несколько отклонений сразу — так часто начинается простуда или перегруз. Сегодня полегче, больше сна и воды.' : 'Дыхание, температура, пульс покоя и SpO₂ относительно твоей нормы за 30 дней'}</span>
        </span>
        <span className="row">
          {h.signals.map((s) => (
            <span key={s.key} className="chip" style={{ background: '#1B1F25', color: s.level < 0 ? 'var(--muted)' : LVL[s.level] }}>
              {s.name.split(' ')[0]} {s.level < 0 ? '—' : s.level === 0 ? '✓' : '!'}
            </span>
          ))}
        </span>
      </section>

      <div className="grid2">
        <section className="card" aria-label="Сон">
          <div className="between">
            <span className="label">Сон · прошлая ночь{night ? ` · ${dayLabel(night.date)}` : ''}</span>
            {bed && wake && <span className="muted small num">{bed} — {wake}</span>}
          </div>
          {night ? (
            <>
              <div className="row" style={{ gap: 20, alignItems: 'center', flexWrap: 'nowrap' }}>
                <div className="score-ring" style={{ borderColor: night.score >= 80 ? '#3DDC97' : night.score >= 60 ? '#F5B642' : '#FF6B6B' }}>
                  <span className="num" style={{ fontSize: 30, fontWeight: 700 }}>{night.score}</span><span className="muted tiny">оценка</span>
                </div>
                <div className="stack" style={{ gap: 2 }}>
                  <span className="num" style={{ fontSize: 26, fontWeight: 700 }}>{hm(night.totalS)}</span>
                  <span className="muted small">из {hm(h.need.baseMin * 60)} обычной потребности · {pctNeed}%</span>
                </div>
              </div>
              <div className="tiles4">
                <div className="tile"><span className="num tile-v2">{pctNeed}%</span><span className="muted tiny">длительность</span></div>
                <div className="tile"><span className="num tile-v2">{eff}%</span><span className="muted tiny">эффективность</span></div>
                <div className="tile"><span className="tile-v2">{stagesWord}</span><span className="muted tiny">стадии</span></div>
                <div className="tile"><span className="num tile-v2">{h.regularityMin != null ? `±${h.regularityMin} мин` : '—'}</span><span className="muted tiny">режим отбоя, 7 дней</span></div>
              </div>
              <div className="row small" style={{ gap: 18 }}>
                <span><b style={{ color: STAGE.deep[1] }}>Глубокий</b> {hm(night.deepS)}</span>
                <span><b style={{ color: STAGE.rem[1] }}>REM</b> {hm(night.remS)}</span>
                <span><b style={{ color: STAGE.core[1] }}>Лёгкий</b> {hm(night.coreS)}</span>
                {h.wakeups && <span className="muted">Пробуждений {h.wakeups.count} · {h.wakeups.min} мин</span>}
              </div>
              {h.hypnogram ? <Hypnogram hyp={h.hypnogram} tz={h.tz} /> : <span className="muted tiny">Стадии по времени появятся после следующей синхронизации freddy.</span>}
              <div className="between"><span className="small" style={{ fontWeight: 600 }}>Неделя</span><span className="muted tiny">долг сна за 7 дней <b style={{ color: h.need.debtMin > 120 ? '#F5B642' : 'var(--text)' }}>{hmWords(h.need.debtMin)}</b></span></div>
              <SleepBars nights={h.nights} needMin={h.need.baseMin} count={7} />
            </>
          ) : <span className="muted">Apple Watch ещё не прислали сон.</span>}
        </section>

        <SleepPlan h={h} reload={reload} needS={needS} />
      </div>

      <div className="grid2">
        <section className="card" aria-label="HRV">
          <div className="between"><span className="label">{h.hrv.nightly ? 'HRV ночью · 30 дней' : 'HRV · 30 дней'}</span><span className="chip" style={{ background: '#1B1F25', color: hrvWord === 'ниже нормы' ? '#F5B642' : '#3DDC97' }}>{hrvWord || '—'}</span></div>
          <span className="small"><b className="num" style={{ fontSize: 22 }}>{hrvNow ? n0(hrvNow[1]) : '—'}</b> мс сегодня{h.hrv.band ? ` · норма ${n0(h.hrv.band[1])}–${n0(h.hrv.band[2])}` : ''}</span>
          {h.hrv.nightly && <span className="muted tiny">Медиана замеров часов во время сна, без пробуждений</span>}
          <LineBand pts={h.hrv.last30} band={h.hrv.band} color="#3DDC97" fmt={(v) => String(Math.round(v))} marks={marksOf(h, "hrv")} />
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>Полоса — твой коридор нормы за 30 дней. Выше неё — нервная система хорошо восстановилась; жёлтые точки — утра ниже нормы.</span>
        </section>
        <section className="card" aria-label="Пульс покоя">
          <div className="between"><span className="label">{h.rhr.nightly ? 'Пульс покоя ночью · 30 дней' : 'Пульс покоя · 30 дней'}</span><span className="chip" style={{ background: '#1B1F25', color: rhrWord === 'выше нормы' ? '#F5B642' : '#3DDC97' }}>{rhrWord || '—'}</span></div>
          <span className="small"><b className="num" style={{ fontSize: 22 }}>{rhrNow ? n0(rhrNow[1]) : '—'}</b> уд/мин{h.rhr.band ? ` · норма ${n0(h.rhr.band[1])}–${n0(h.rhr.band[2])}` : ''}</span>
          {h.rhr.nightly && <span className="muted tiny">Средний пульс за самые спокойные 30 минут сна</span>}
          <LineBand pts={h.rhr.last30} band={h.rhr.band} color="#FF8A65" fmt={(v) => String(Math.round(v))} invertGood marks={marksOf(h, "rhr")} />
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>Плавное снижение за месяцы — признак растущей аэробной формы. Рост на 5+ ударов выше нормы — повод для лёгкого дня.</span>
        </section>
      </div>

      <StressCard st={h.stress?.today} />

      <div className="grid">
        <section className="card" aria-label="Ранние сигналы ночи">
          <span className="label">Ранние сигналы · ночь</span>
          <div>
            {h.signals.map((s) => (
              <div key={s.key} className="list-row small">
                <span className="row" style={{ gap: 8, flex: 1 }}><span className="dot" style={{ background: s.level < 0 ? '#3A404A' : LVL[s.level] }} />{s.name}</span>
                <b className="num">{s.value == null ? '—' : `${s.key === 'temp' && s.value > 0 ? '+' : ''}${n1(s.value)} ${s.unit}`}</b>
                <span className="muted tiny" style={{ width: 120, textAlign: 'right' }}>{s.norm ? `норма ${n1(s.norm[1])}–${n1(s.norm[2])}` : 'норма набирается'}</span>
              </div>
            ))}
          </div>
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>Отклонение одного показателя — обычное дело. Два и больше сразу, особенно температура вместе с пульсом, часто опережают простуду на 1–2 дня.</span>
        </section>
        <section className="card" aria-label="VO2max">
          <span className="label">Кардиоформа · VO₂max</span>
          <div className="tiles4" style={{ gridTemplateColumns: 'repeat(2, minmax(0, 1fr))' }}>
            <div className="tile"><span className="num tile-v">{vo2 ? n1(vo2[1]) : '—'}</span><span className="muted tiny">часы · {vo2 ? dayLabel(vo2[0]) : '—'}</span></div>
            <div className="tile"><span className="num tile-v">{h.vo2.lab ? n1(h.vo2.lab.value) : '—'}</span><span className="muted tiny">{h.vo2.lab ? `лаборатория · ${h.vo2.lab.where ? `${h.vo2.lab.where}, ` : ''}${dayLabel(h.vo2.lab.date)}` : 'лабораторный тест — внеси в Настройки → Профиль'}</span></div>
          </div>
          {vo2 && h.vo2.lab && <span className="small" style={{ lineHeight: 1.5, color: 'var(--text-2)' }}>Часы показывают на {n0(Math.abs((vo2[1] / h.vo2.lab!.value - 1) * 100))}% {vo2[1] >= h.vo2.lab!.value ? 'выше' : 'ниже'} лаборатории — для трендов им можно доверять, а точку отсчёта даёт тест.</span>}
          <span className="small"><b>Шаги</b> <span className="num">{n0(h.body.steps7)}</span> <span className="muted">в день, среднее за 7 дней</span></span>
          <span className="small"><b>Вес</b> <span className="num">{w ? `${n1(w.weight)} кг` : '—'}</span> <span className="muted">{w ? `весы · ${dayLabel(w.date)}${w.fat != null ? ` · жир ${n1(w.fat)}%` : ''}` : ''}</span></span>
        </section>
      </div>

      <div className="grid">
        <Habits h={h} />
        <AlcoholCalc h={h} />
      </div>
      <Diary h={h} reload={reload} />
      <section className="card" aria-label="Тренер">
        <div className="between"><span className="label">Итоги недели от тренера</span><span className="chip chip-off">этап 5</span></div>
        <span className="muted small">Тренер Claude будет разбирать неделю: что помогло восстановлению, что мешало, и как поменять режим.</span>
      </section>
    </>
  );
}

function SleepPlan({ h, reload, needS }: { h: HealthSummary; reload: () => void; needS: number }) {
  const [edit, setEdit] = useState<{ weekday: string; weekend: string; base: string; mode: 'auto' | 'manual' } | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const n = h.need;
  const items: [string, number][] = [
    [n.needMode === 'manual' ? 'Базовая норма (вручную)' : 'Твоя потребность', n.baseMin], ['Нагрузка дня', n.loadMin], ['Возврат долга сна', n.repayMin], ['Дневной сон', -n.napMin], ['Сигналы болезни', n.illMin],
  ];
  async function save() {
    if (!edit) return;
    const base = Math.round(Number(edit.base.replace(',', '.')) * 60);
    try {
      await api.healthSettingSet('sleep_schedule', { ...h.schedule, weekday: edit.weekday, weekend: edit.weekend, baseMin: base > 300 ? base : h.schedule.baseMin, needMode: edit.mode });
      setEdit(null); setMsg(null); reload();
    } catch (e) { setMsg(errText(e)); }
  }
  // Timeline from 3 h before lights-off to 1 h after waking; times before 15:00 count as "after midnight".
  const toMin = (s: string) => { const [a, b] = s.split(':').map(Number); const m = a * 60 + b; return m < 15 * 60 ? m + 1440 : m; };
  const t0 = toMin(h.plan.lights) - 180, t1 = toMin(h.plan.wake) + 60;
  const pos = (s: string) => `${((toMin(s) - t0) / (t1 - t0)) * 100}%`;
  const tick = (m: number) => { const x = ((m % 1440) + 1440) % 1440; return `${String(Math.floor(x / 60)).padStart(2, '0')}:${String(x % 60).padStart(2, '0')}`; };
  const sug = h.suggestion;
  const acc = h.plan.accepted;
  async function accept(mode: 'earlier' | 'later' | null) {
    try { await api.healthSettingSet('sleep_tonight', mode && sug ? { date: sug.date, mode, min: sug.min } : null); reload(); } catch (e) { setMsg(errText(e)); }
  }
  const rh = h.rhythm;
  const far = (a: string, b: string) => Math.abs(toMin(a) - toMin(b)) > 60;
  async function useRhythm() {
    if (!rh) return;
    try { await api.healthSettingSet('sleep_schedule', { ...h.schedule, weekday: rh.wake, weekend: rh.wake }); reload(); } catch (e) { setMsg(errText(e)); }
  }
  return (
    <section className="card" aria-label="План сна">
      <span className="label">План сна на сегодня</span>
      <span className="muted small">
        Подъём по расписанию {h.plan.weekend ? 'выходного' : 'будней'}
        {h.scheduleSource === 'watch' && rh ? ` · расписание взято из часов: обычно ты спишь ${rh.bed}–${rh.wake}` : ''}
      </span>
      {sug && (
        <div className="note-warn stack" style={{ gap: 8 }}>
          <b>Сегодня лучше поспать на {sug.min} минут больше</b>
          <span>Почему: {sug.reasons.length ? sug.reasons.join(', ') : 'нагрузка и долг сна'}.</span>
          <div className="row">
            <button type="button" className="btn2" onClick={() => accept('earlier')}>Лечь в {sug.earlierBed}</button>
            <button type="button" className="btn2" onClick={() => accept('later')}>Встать в {sug.laterWake}</button>
            <button type="button" className="linkbtn small" onClick={() => accept(null)}>Оставить как обычно</button>
          </div>
          <span className="tiny" style={{ opacity: 0.8 }}>Только на эту ночь — расписание не меняется. Если можешь, лучше лечь раньше: постоянное время подъёма держит ритм.</span>
        </div>
      )}
      {acc && <span className="small" style={{ color: 'var(--green)' }}>На эту ночь: {acc.mode === 'earlier' ? `отбой на ${acc.min} мин раньше` : `подъём на ${acc.min} мин позже`} · <button type="button" className="linkbtn small" onClick={() => accept(null)}>отменить</button></span>}
      {rh && h.scheduleSource === 'user' && far(rh.wake, h.schedule.weekday) && (
        <div className="inset" style={{ flexDirection: 'row', flexWrap: 'wrap', alignItems: 'center', gap: 10 }}>
          <span className="small" style={{ flex: '1 1 260px' }}>По часам ты обычно спишь {rh.bed}–{rh.wake} ({plural(rh.nights, 'ночь', 'ночи', 'ночей')}), а в расписании подъём {h.schedule.weekday}.</span>
          <button type="button" className="btn2" onClick={useRhythm}>Сделать подъём {rh.wake}</button>
        </div>
      )}
      <div className="tiles4" style={{ gridTemplateColumns: 'repeat(3, minmax(0, 1fr))' }}>
        <div className="tile"><span className="num tile-v2">{h.plan.lights}</span><span className="muted tiny">свет и экраны · начать отходить</span></div>
        <div className="tile"><span className="num tile-v2" style={{ color: '#A78BFA' }}>{h.plan.bed}</span><span className="muted tiny">отбой — обычная норма сна</span></div>
        <div className="tile"><span className="num tile-v2">{h.plan.wake}</span><span className="muted tiny">подъём</span></div>
      </div>
      <div className="planbar">
        <div className="seg" style={{ left: pos(h.plan.lights), width: `calc(${pos(h.plan.bed)} - ${pos(h.plan.lights)})`, background: '#3a3256' }} />
        <div className="seg" style={{ left: pos(h.plan.bed), width: `calc(${pos(h.plan.wake)} - ${pos(h.plan.bed)})`, background: '#A78BFA' }} />
      </div>
      <div className="between tiny muted num">{[0, 1, 2, 3, 4].map((k) => <span key={k}>{tick(Math.round(t0 + ((t1 - t0) * k) / 4))}</span>)}</div>
      <span className="small"><b>Сон {hm(needS)}</b> <span className="muted">+ 15 мин на засыпание</span></span>
      <div className="stack" style={{ gap: 0 }}>
        <span className="small" style={{ fontWeight: 700, paddingBottom: 4 }}>Из чего сложилась потребность</span>
        {items.map(([t, m]) => (
          <div key={t} className="list-row small"><span className="muted">{t}</span><span className="num" style={{ color: m > 0 && t !== items[0][0] ? '#F5B642' : undefined }}>{t === items[0][0] ? hm(m * 60) : m === 0 ? '0:00' : signed(m)}</span></div>
        ))}
        <div className="list-row small"><b>Потребность сегодня</b><b className="num">{hm(needS)}</b></div>
      </div>
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>
        {n.needMode === 'manual'
          ? 'Потребность задана вручную.'
          : n.personalMin != null
            ? `Твоя потребность посчитана по ${plural(n.personalNights ?? 0, 'ночи', 'ночам', 'ночам')}, после которых HRV и пульс покоя были в норме (в пределах 7–9 ч).`
            : `Пока мало данных, чтобы посчитать твою потребность (нужно 10 «хороших» ночей, есть ${n.personalNights ?? 0}) — взята базовая норма.`}
        {n.bandMin ? ` Колебания ±${n.bandMin} мин вокруг неё долгом не считаются, а долгая ночь гасит половину долга.` : ''}
      </span>
      {n.chronic && <div className="note-warn">{n.chronic}</div>}
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>
        Кофеин лучше до {h.plan.caffeineCut}. Тяжёлую тренировку вечером закончи хотя бы за 3 часа до отбоя. Долг сна возвращаем постепенно — не больше 45 минут за ночь.
      </span>
      {edit ? (
        <div className="inset">
          <div className="row" style={{ gap: 12, alignItems: 'flex-end' }}>
            <div className="field"><label htmlFor="sw">Подъём в будни</label><input id="sw" value={edit.weekday} onChange={(e) => setEdit({ ...edit, weekday: e.target.value })} /></div>
            <div className="field"><label htmlFor="se">В выходные</label><input id="se" value={edit.weekend} onChange={(e) => setEdit({ ...edit, weekend: e.target.value })} /></div>
            {edit.mode === 'manual' && <div className="field"><label htmlFor="sb">Потребность, ч</label><input id="sb" value={edit.base} onChange={(e) => setEdit({ ...edit, base: e.target.value })} /></div>}
          </div>
          <div className="row" style={{ gap: 8 }}>
            <span className="small">Потребность во сне</span>
            <button type="button" className="pill" aria-pressed={edit.mode === 'auto'} onClick={() => setEdit({ ...edit, mode: 'auto' })}>По моим данным</button>
            <button type="button" className="pill" aria-pressed={edit.mode === 'manual'} onClick={() => setEdit({ ...edit, mode: 'manual' })}>Вручную</button>
          </div>
          {msg && <div className="note-err">{msg}</div>}
          <div className="row"><button type="button" className="btn" onClick={save}>Сохранить</button><button type="button" className="btn2" onClick={() => setEdit(null)}>Отмена</button></div>
        </div>
      ) : (
        <button type="button" className="btn2" style={{ alignSelf: 'flex-start' }} onClick={() => setEdit({ weekday: h.schedule.weekday, weekend: h.schedule.weekend, base: String((n.savedBaseMin ?? h.schedule.baseMin) / 60).replace('.', ','), mode: n.needMode ?? 'auto' })}>Изменить расписание</button>
      )}
    </section>
  );
}

function Habits({ h }: { h: HealthSummary }) {
  const a = h.alcohol, c = h.caffeine;
  const max = Math.max(1, ...a.days.map((d) => d[1]));
  const drinkDays = a.days.filter((d) => d[1] > 0).length;
  const total = a.days.reduce((s, d) => s + d[1], 0);
  return (
    <section className="card" aria-label="Привычки">
      <span className="label">Алкоголь и кофеин · что влияет на восстановление</span>
      <div className="between small"><b>Алкоголь, порций в день</b><span className="muted tiny">{drinkDays} из {a.days.length} дней · ≈ {n0(total)} порций</span></div>
      <div className="hbars" style={{ height: 60 }}>
        {a.days.map((d) => <div key={d[0]} className="hbar" title={`${dayLabel(d[0])}: ${n1(d[1])}`}><div style={{ height: `${(d[1] / max) * 100}%`, background: '#F5B642' }} /></div>)}
      </div>
      <div className="tiles4" style={{ gridTemplateColumns: 'repeat(2, minmax(0, 1fr))' }}>
        <div className="tile"><span className="num tile-v2" style={{ color: a.hrvAfterPct != null && a.hrvAfterPct < 0 ? '#F5B642' : undefined }}>{a.hrvAfterPct == null ? '—' : `${a.hrvAfterPct > 0 ? '+' : '−'}${n0(Math.abs(a.hrvAfterPct))}%`}</span><span className="muted tiny">HRV наутро после алкоголя</span></div>
        <div className="tile"><span className="num tile-v2">{a.rhrAfter == null ? '—' : `${a.rhrAfter > 0 ? '+' : ''}${n1(a.rhrAfter)}`}</span><span className="muted tiny">пульс покоя наутро, уд/мин</span></div>
      </div>
      {a.known < 21 && <span className="muted tiny">Предварительно: данных за {plural(a.known, 'день', 'дня', 'дней')}. Через 3–4 недели оценка станет надёжной.</span>}
      <div className="between small divider" style={{ paddingTop: 12 }}><b>Кофеин</b><span className="muted tiny">Waterllama</span></div>
      <div className="tiles4" style={{ gridTemplateColumns: 'repeat(3, minmax(0, 1fr))' }}>
        <div className="tile"><span className="num tile-v2">{c.avg != null ? `${n0(c.avg)} мг` : '—'}</span><span className="muted tiny">в среднем в день</span></div>
        <div className="tile"><span className="num tile-v2">{c.latest ?? '—'}</span><span className="muted tiny">самый поздний</span></div>
        <div className="tile"><span className="num tile-v2">{c.lateDays} из {c.known}</span><span className="muted tiny">дней позже {h.plan.caffeineCut}</span></div>
      </div>
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>Отсечка — примерно за 6,5 часов до отбоя ({h.plan.bed}). Перед темповой кофе за 45–60 минут может помочь — если тренировка до отсечки.</span>
    </section>
  );
}

const DRINKS: { name: string; abv: number; vol: number }[] = [
  { name: 'Пиво', abv: 5, vol: 500 }, { name: 'Вино', abv: 12, vol: 150 }, { name: 'Игристое', abv: 11, vol: 150 },
  { name: 'Крепкое', abv: 40, vol: 50 }, { name: 'Коктейль', abv: 10, vol: 250 }, { name: 'Сидр', abv: 5, vol: 330 },
];

function AlcoholCalc({ h }: { h: HealthSummary }) {
  const ses = h.alcohol.session;
  const pre = useMemo(() => {
    if (!ses.length) return null;
    const vol = ses[0].volumeMl ?? 300;
    const abv = Math.round(((ses[0].units * 10) / vol) * 1000) / 10; // UK unit = 10 ml of pure alcohol
    return { vol, abv, count: ses.length, first: ses[0].ts, last: ses[ses.length - 1].ts };
  }, [ses]);
  const [vol, setVol] = useState(pre?.vol ?? 500);
  const [abv, setAbv] = useState(pre?.abv ?? 5);
  const [count, setCount] = useState(pre?.count ?? 2);
  const [first, setFirst] = useState(pre ? clockAt(pre.first, h.tz) : '20:00');
  const [last, setLast] = useState(pre ? clockAt(pre.last, h.tz) : '22:00');
  const weight = h.alcohol.weightKg ?? 110;
  const grams = count * vol * (abv / 100) * 0.789;
  const r = 0.68; // body water share, men
  const peak = grams / (weight * r); // ‰ if drunk at once
  const toM = (s: string) => { const [a, b] = s.split(':').map(Number); return (a || 0) * 60 + (b || 0); };
  const fm = toM(first), lm = toM(last) < fm ? toM(last) + 1440 : toM(last);
  const clear = (beta: number) => Math.max(fm + (peak / beta) * 60, lm + 45);
  const fmt = (m: number) => `${String(Math.floor((m % 1440) / 60)).padStart(2, '0')}:${String(Math.round(m % 60)).padStart(2, '0')}`;
  const units = grams / 8;
  return (
    <section className="card" aria-label="Восстановление после алкоголя">
      <span className="label">Восстановление после алкоголя</span>
      {pre ? (
        <span className="muted small" style={{ lineHeight: 1.5 }}>
          Из Waterllama: {plural(pre.count, 'напиток', 'напитка', 'напитков')} по {pre.vol} мл, {clockAt(pre.first, h.tz)}–{clockAt(pre.last, h.tz)}. По крепости похоже на {pre.abv}% — если пили другое, поправь ниже.
        </span>
      ) : <span className="muted small">Внеси, что и когда пили, — Kouros прикинет сроки.</span>}
      <div className="row" role="group" aria-label="Что пили">
        {DRINKS.map((d) => <button key={d.name} type="button" className="pill" aria-pressed={abv === d.abv && vol === d.vol} onClick={() => { setAbv(d.abv); setVol(d.vol); }}>{d.name}</button>)}
      </div>
      <div className="row" style={{ gap: 12, alignItems: 'flex-end' }}>
        <div className="field"><label htmlFor="av">Порция, мл</label><input id="av" inputMode="numeric" value={vol} onChange={(e) => setVol(Number(e.target.value) || 0)} /></div>
        <div className="field"><label htmlFor="aa">Крепость, %</label><input id="aa" inputMode="decimal" value={abv} onChange={(e) => setAbv(Number(e.target.value.replace(',', '.')) || 0)} /></div>
        <div className="field"><label>Сколько</label><span className="row" style={{ gap: 6, flexWrap: 'nowrap' }}><button type="button" className="xbtn" onClick={() => setCount(Math.max(1, count - 1))}>−</button><b className="num">{count}</b><button type="button" className="xbtn" onClick={() => setCount(count + 1)}>+</button></span></div>
        <div className="field"><label htmlFor="af">Первый</label><input id="af" type="time" style={{ width: 110 }} value={first} onChange={(e) => setFirst(e.target.value)} /></div>
        <div className="field"><label htmlFor="al">Последний</label><input id="al" type="time" style={{ width: 110 }} value={last} onChange={(e) => setLast(e.target.value)} /></div>
      </div>
      <div className="tiles4" style={{ gridTemplateColumns: 'repeat(3, minmax(0, 1fr))' }}>
        <div className="tile"><span className="num tile-v2">≈ {n0(grams)} г</span><span className="muted tiny">чистый алкоголь · {n1(units)} ед.{grams > 60 ? ' · большая доза' : ''}</span></div>
        <div className="tile"><span className="num tile-v2">≈ {fmt(clear(0.15))}</span><span className="muted tiny">выведение · диапазон {fmt(clear(0.2))}–{fmt(clear(0.1))}</span></div>
        <div className="tile"><span className="num tile-v2">36–48 ч</span><span className="muted tiny">HRV и пульс вернутся к норме после последнего бокала</span></div>
      </div>
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>
        Формула Видмарка с весом {n1(weight)} кг. Скорость выведения у разных людей и в разные дни отличается в 1,5–2 раза — это оценка для планирования тренировок. Не используй её, чтобы решить, можно ли садиться за руль.
      </span>
    </section>
  );
}

function Diary({ h, reload }: { h: HealthSummary; reload: () => void }) {
  const [name, setName] = useState('');
  async function mark(key: string, cur: number | null) {
    const next = cur == null ? 1 : cur === 1 ? 0 : null;
    try { await api.habitMark(h.today, key, next); reload(); } catch { /* shown on reload */ }
  }
  async function addCustom() {
    const n = name.trim();
    if (!n) return;
    const custom = h.habits.filter((x) => x.key.startsWith('c:')).map((x) => x.name);
    if (!custom.includes(n)) await api.healthSettingSet('habits_custom', [...custom, n]).catch(() => {});
    setName(''); reload();
  }
  return (
    <section className="card" aria-label="Дневник дня">
      <div className="between"><span className="label">Дневник дня · {dayLabel(h.today)}</span><span className="muted tiny">влияние на HRV следующего утра</span></div>
      <span className="muted small">Нажми на привычку, чтобы отметить «да» → «нет» → сбросить. Алкоголь, поздний кофеин и поздний ужин подтягиваются сами.</span>
      <div>
        {h.habits.map((x) => {
          const need = Math.max(0, 5 - x.effect.with) + Math.max(0, 5 - x.effect.without);
          return (
            <div key={x.key} className="list-row small">
              {x.auto ? (
                <span className="chip" style={{ background: '#1B1F25', color: x.today ? '#F5B642' : 'var(--muted)', minWidth: 54, justifyContent: 'center' }}>{x.today ? 'да' : x.today === 0 ? 'нет' : '—'}</span>
              ) : (
                <button type="button" className="pill" style={{ minWidth: 54, justifyContent: 'center', height: 30 }} aria-pressed={x.today != null} onClick={() => mark(x.key, x.today)}>{x.today === 1 ? 'да' : x.today === 0 ? 'нет' : '—'}</button>
              )}
              <span style={{ flex: 1 }}>{x.name}{x.auto && <span className="muted tiny"> · {x.auto}</span>}</span>
              <span className="num tiny" style={{ color: x.effect.pct == null ? 'var(--muted)' : x.effect.pct < 0 ? '#F5B642' : '#3DDC97', textAlign: 'right' }}>
                {x.effect.pct != null ? `HRV ${x.effect.pct > 0 ? '+' : '−'}${n0(Math.abs(x.effect.pct))}%` : `нужно ещё ${plural(need, 'отметка', 'отметки', 'отметок')}`}
              </span>
            </div>
          );
        })}
      </div>
      <div className="row">
        <input className="input" style={{ maxWidth: 280 }} placeholder="Своя привычка" value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter') addCustom(); }} />
        <button type="button" className="btn2" disabled={!name.trim()} onClick={addCustom}>+ Добавить</button>
      </div>
      <span className="muted tiny">Оценка появляется, когда набирается хотя бы 5 дней с привычкой и 5 без неё. Так видно, что именно помогает восстановлению, а что мешает.</span>
    </section>
  );
}

// ---------- history ----------

const PERIODS: [number, string][] = [[7, 'Неделя'], [30, 'Месяц'], [90, '3 месяца']];
const addDay = (d: string, k: number) => { const x = new Date(d + 'T12:00:00Z'); x.setUTCDate(x.getUTCDate() + k); return x.toISOString().slice(0, 10); };

/** Time-weighted trend (exponential smoothing with a 10-day half-life). */
function smoothTrend(p: Pt[]): Pt[] {
  const out: Pt[] = [];
  let v: number | null = null, t: number | null = null;
  for (const [d, x] of p) {
    const tt = Date.parse(d);
    if (v == null || t == null) v = x;
    else { const k = 1 - Math.pow(0.5, (tt - t) / 86400000 / 10); v = v + (x - v) * k; }
    t = tt;
    out.push([d, Math.round(v * 10) / 10]);
  }
  return out;
}

const BODY_M: [('weight' | 'fat' | 'lean'), string, string, string][] = [['weight', 'Вес', 'кг', '#9DB8FF'], ['fat', 'Жир', '%', '#F5B642'], ['lean', 'Безжировая масса', 'кг', '#3DDC97']];
const BODY_P: [number, string][] = [[30, 'Месяц'], [90, '3 месяца'], [365, 'Год'], [0, 'Всё']];

function BodyChart({ h, sel, onPick }: { h: HealthSummary; sel: string | null; onPick: (d: string) => void }) {
  const [m, setM] = useState<'weight' | 'fat' | 'lean'>('weight');
  const [per, setPer] = useState(90);
  const [all, setAll] = useState(false);
  const [, name, unit, color] = BODY_M.find((x) => x[0] === m)!;
  const series = h.body[m] ?? [];
  const trendAll = useMemo(() => smoothTrend(series), [series]);
  const from = per ? addDay(h.today, -per) : '0000';
  const pts = series.filter((p) => p[0] >= from);
  const tr = trendAll.filter((p) => p[0] >= from);
  const ch = tr.length > 1 ? tr[tr.length - 1][1] - tr[0][1] : null;
  return (
    <section className="card" aria-label="Состав тела">
      <div className="between"><span className="label">Состав тела · умные весы</span><span className="muted tiny">{plural(pts.length, 'замер', 'замера', 'замеров')}</span></div>
      <div className="filters">
        <div className="row" role="group" aria-label="Показатель">{BODY_M.map(([k, n]) => <button key={k} type="button" className="pill" aria-pressed={m === k} onClick={() => setM(k)}>{n}</button>)}</div>
        <div className="row" role="group" aria-label="Период">{BODY_P.map(([d, n]) => <button key={d} type="button" className="pill" aria-pressed={per === d} onClick={() => setPer(d)}>{n}</button>)}</div>
      </div>
      <span className="small">Тренд сейчас <b className="num">{tr.length ? `${n1(tr[tr.length - 1][1])} ${unit}` : '—'}</b>{ch != null && <span className="muted"> · за период {ch > 0 ? '+' : '−'}{n1(Math.abs(ch))} {unit}</span>}</span>
      <LineBand pts={pts} band={null} color={color} height={150} fmt={(v) => n1(v)} trend={tr} sel={sel} onPick={onPick} marks={marksOf(h, "body")} />
      <span className="muted tiny">Точки — каждое взвешивание, линия — тренд. Биоимпедансные весы дают разброс 1–2% от замера к замеру, поэтому важнее направление линии, чем отдельные цифры.</span>
      <button type="button" className="collapse" aria-expanded={all} onClick={() => setAll(!all)}>
        <span className="row" style={{ gap: 10 }}><span className="chev sm" style={{ transform: all ? 'rotate(180deg)' : 'none' }}>⌄</span><span className="small" style={{ fontWeight: 600 }}>Все замеры · {name}</span></span>
      </button>
      {all && (
        <div className="table" style={{ maxHeight: 360, overflowY: 'auto' }}>
          <div className="tr tr-body th"><span>Дата</span><span className="r">Вес</span><span className="r">Жир</span><span className="r">Безжировая масса</span></div>
          {[...h.body.weight].filter((p) => p[0] >= from).reverse().map(([d, w]) => {
            const f = h.body.fat.find((x) => x[0] === d), l = h.body.lean.find((x) => x[0] === d);
            return (
              <button key={d} type="button" className={`tr tr-body small plain${d === sel ? ' on' : ''}`} onClick={() => onPick(d)}>
                <span>{new Date(d + 'T12:00:00').toLocaleDateString('ru-RU', { day: 'numeric', month: 'short', year: 'numeric' })}</span><span className="r num">{n1(w)} кг</span><span className="r num">{f ? `${n1(f[1])}%` : '—'}</span><span className="r num">{l ? `${n1(l[1])} кг` : '—'}</span>
              </button>
            );
          })}
        </div>
      )}
    </section>
  );
}

function DayCard({ date, today, ready, onDate, onOpen }: { date: string; today: string; ready: number | null; onDate: (d: string) => void; onOpen?: (id: string) => void }) {
  const [d, setD] = useState<HealthDay | null>(null);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => { setD(null); api.healthDay(date).then((v) => { setD(v); setErr(null); }).catch((e) => setErr(errText(e))); }, [date]);
  const title = new Date(date + 'T12:00:00').toLocaleDateString('ru-RU', { weekday: 'long', day: 'numeric', month: 'long' });
  const item = (label: string, v: string | null | undefined, color?: string) => (v == null ? null : (
    <span key={label + v} className="stack" style={{ gap: 0 }}><span className="muted tiny">{label}</span><b className="num small" style={color ? { color } : undefined}>{v}</b></span>
  ));
  const groups: [string, ReactNode[]][] = d ? [
    ['Сердце', [
      ...(d.readiness ? [] : [
        item('готовность', ready != null ? String(ready) : null, ready == null ? undefined : ready >= 67 ? '#3DDC97' : ready >= 40 ? '#F5B642' : '#FF6B6B'),
        item('HRV', d.hrv != null ? `${d.hrv} мс` : null), item('пульс покоя', d.rhr != null ? `${d.rhr}` : null),
      ]),
      item('стресс днём', d.stress?.score != null ? `${d.stress.score} · ${d.stress.level}` : null, d.stress?.score != null ? stressColor(d.stress.score) : undefined),
      item('восстановление пульса', d.hrRecovery != null ? `${d.hrRecovery} уд/мин` : null), item('пульс при ходьбе', d.walkHr != null ? `${d.walkHr}` : null),
      item('VO₂max', d.vo2 != null ? n1(d.vo2) : null),
    ]],
    ['Ночь', [
      ...(d.readiness ? [] : [item('частота дыхания', d.resp != null ? `${n1(d.resp)} /мин` : null), item('SpO₂', d.spo2 != null ? `${n1(d.spo2)}%` : null)]),
      item('нарушения дыхания', d.breath != null ? n1(d.breath) : null),
      item('температура запястья', d.tempShift != null ? `${d.tempShift > 0 ? '+' : ''}${n1(d.tempShift)} °C` : null),
    ]],
    ['Активность и тело', [
      item('шаги', d.steps != null ? n0(d.steps) : null), item('активные калории', d.activeKcal != null ? `${n0(d.activeKcal)} ккал` : null),
      item('вес', d.weight != null ? `${n1(d.weight)} кг` : null), item('жир', d.fatPct != null ? `${n1(d.fatPct)}%` : null),
    ]],
    ['Питание и привычки', [
      item('съедено', d.kcal != null ? `${n0(d.kcal)} ккал` : null), item('белок', d.protein != null ? `${n0(d.protein)} г` : null),
      item('углеводы', d.carbs != null ? `${n0(d.carbs)} г` : null), item('жиры', d.fatG != null ? `${n0(d.fatG)} г` : null),
      item('вода', d.waterMl != null ? `${n1(d.waterMl / 1000)} л` : null),
      item('алкоголь', d.alcohol ? `${n1(d.alcohol)} порц.` : null, '#F5B642'),
      item('кофеин', d.caffeine != null ? `${n0(d.caffeine)} мг${d.caffeineLast ? ` · до ${d.caffeineLast}` : ''}` : null),
      ...d.habits.filter((x) => x[1] === 1).map(([k]) => item('дневник', HABIT_NAMES[k] ?? k.replace(/^c:/, ''))),
      ...(d.meds ?? []).map((m) => item(m.kind === 'medicine' ? 'лекарство' : 'добавка', `${m.name}${m.servings !== 1 ? ` ×${String(m.servings).replace('.', ',')}` : ''}`)),
    ]],
  ] : [];
  return (
    <section className="card daycard" aria-label="День">
      <div className="between">
        <span className="row" style={{ gap: 8 }}>
          <button type="button" className="xbtn" aria-label="Предыдущий день" onClick={() => onDate(addDay(date, -1))}>←</button>
          <b style={{ fontFamily: 'var(--display)', fontSize: 18 }}>{title.charAt(0).toUpperCase() + title.slice(1)}</b>
          <button type="button" className="xbtn" aria-label="Следующий день" disabled={date >= today} onClick={() => onDate(addDay(date, 1))}>→</button>
        </span>
        <span className="row" style={{ gap: 8 }}>
          <input type="date" className="select" value={date} max={today} aria-label="Выбрать день" onChange={(e) => e.target.value && onDate(e.target.value)} />
          {date !== today && <button type="button" className="btn2" onClick={() => onDate(today)}>Сегодня</button>}
        </span>
      </div>
      {err && <div className="note-err">{err}</div>}
      {!d && !err && <span className="muted small">Загружаю…</span>}
      {d && (
        <>
          {d.readiness && <DayReady r={d.readiness} />}
          {d.stress?.score != null && <StressMini st={d.stress} />}
          {d.sleep ? (
            <div className="stack" style={{ gap: 6 }}>
              <span className="small"><b>Сон {hm(d.sleep.totalS)}</b>
                <span className="muted"> · глубокий {hm(d.sleep.deepS ?? 0)} · REM {hm(d.sleep.remS ?? 0)} · лёгкий {hm(d.sleep.coreS ?? 0)}
                  {d.hypnogram ? ` · ${clockAt(d.hypnogram.start, d.tz)} — ${clockAt(d.hypnogram.end, d.tz)}` : ''}</span></span>
              {d.hypnogram && <Hypnogram hyp={d.hypnogram} tz={d.tz} />}
            </div>
          ) : <span className="muted small">Сна за эту ночь нет.</span>}
          <div className="daygrid">
            {groups.map(([g, items]) => {
              const shown = items.filter(Boolean);
              return shown.length ? <div key={g} className="inset" style={{ gap: 8 }}><b className="tiny muted" style={{ textTransform: 'uppercase', letterSpacing: '.06em' }}>{g}</b><div className="dayitems">{shown}</div></div> : null;
            })}
          </div>
          {d.activities.length > 0 ? (
            <div className="stack" style={{ gap: 0 }}>
              <span className="small" style={{ fontWeight: 700, paddingBottom: 4 }}>Тренировки</span>
              {d.activities.map((a) => {
                const sp = sportOf(a.sport);
                return (
                  <button key={a.id} type="button" className="list-row plain small" onClick={onOpen ? () => onOpen(a.id) : undefined} disabled={!onOpen}>
                    <span className="badge" style={{ color: sp.color }}>{sp.letter}</span>
                    <span style={{ flex: 1 }}><b>{a.name || sp.name}</b> <span className="muted">· {clockAt(a.startUtc, d.tz)} · {fmtDur(a.durationS)}{a.distanceM ? ` · ${fmtKm(a.distanceM)}` : ''}{a.avgHr ? ` · пульс ${Math.round(a.avgHr)}` : ''}</span></span>
                    <span className="num" style={{ color: '#4FA3FF', fontWeight: 700 }}>{a.load ?? '—'}</span>
                  </button>
                );
              })}
            </div>
          ) : <span className="muted small">Тренировок не было.</span>}
        </>
      )}
    </section>
  );
}

const PSYCH: Record<string, [string, (p: ReadinessDay['psych'][number]) => string]> = {
  hooper: ['Самочувствие (Hooper)', (p) => (p.extra?.feel != null ? `${p.extra.feel} из 100` : `${p.score ?? '—'} баллов`)],
  tqr: ['Восстановление (TQR)', (p) => `${p.score ?? '—'} из 20`],
  brums: ['Настроение (BRUMS)', (p) => `индекс ${p.score ?? '—'}`],
  stressrec: ['Стресс и восстановление', (p) => (p.extra ? `стресс ${n1(p.extra.stress)} · восстановление ${n1(p.extra.recovery)}` : String(p.score ?? '—'))],
};
const rColor = (v: number | null | undefined) => (v == null ? 'var(--muted)' : v >= 67 ? '#3DDC97' : v >= 40 ? '#F5B642' : '#FF6B6B');

/** Morning and evening readiness of the selected day, what moved them, vitals and questionnaires. */
function DayReady({ r }: { r: ReadinessDay }) {
  const m = r.morning;
  const comps = m.components.filter((c) => c.score != null);
  return (
    <div className="inset" style={{ gap: 12 }}>
      <div className="row" style={{ gap: 24, alignItems: 'baseline', flexWrap: 'wrap' }}>
        <span className="stack" style={{ gap: 0 }}><span className="muted tiny">готовность утром{m.frozen ? '' : ' · пересчитана'}</span><b className="num" style={{ fontSize: 30, color: rColor(m.score) }}>{m.score ?? '—'}</b></span>
        <span className="stack" style={{ gap: 0 }}><span className="muted tiny">вечером</span><b className="num" style={{ fontSize: 30, color: rColor(r.evening?.score) }}>{r.evening ? r.evening.score : '—'}</b></span>
        {!r.evening && <span className="muted tiny" style={{ flex: '1 1 200px' }}>Вечерняя оценка появляется через 8 часов после пробуждения (между 18:00 и 22:00) и сохраняется начиная с версии 0.10.1.</span>}
      </div>
      <div className="vitals">{r.vitals.filter((v) => v.value != null).map((v) => <Vital key={v.key} v={v} />)}</div>
      <div className="daygrid">
        <div className="stack" style={{ gap: 6 }}>
          <b className="tiny muted" style={{ textTransform: 'uppercase', letterSpacing: '.06em' }}>Утро · от 50</b>
          <div className="eve-items">
            {comps.map((c) => (
              <div key={c.key} className="small"><span>{c.label} <span className="muted tiny">{c.note}</span></span><b className="num" style={{ color: ptsColor(pointsOf(c, m.components)) }}>{fmtPts(pointsOf(c, m.components))}</b></div>
            ))}
            {m.penalties.map((p) => <div key={p.text} className="small"><span>{p.text}</span><b className="num" style={{ color: '#FF6B6B' }}>−{p.points}</b></div>)}
            {!comps.length && <span className="muted small">Нет данных за эту ночь.</span>}
          </div>
        </div>
        {r.evening && (
          <div className="stack" style={{ gap: 6 }}>
            <b className="tiny muted" style={{ textTransform: 'uppercase', letterSpacing: '.06em' }}>За день · от утренних {r.evening.morning}</b>
            <div className="eve-items">
              {r.evening.items.map((it) => <div key={it.text} className="small"><span>{it.text}</span><b className="num" style={{ color: ptsColor(it.points) }}>{fmtPts(it.points)}</b></div>)}
            </div>
          </div>
        )}
        <div className="stack" style={{ gap: 6 }}>
          <b className="tiny muted" style={{ textTransform: 'uppercase', letterSpacing: '.06em' }}>Психология</b>
          <div className="eve-items">
            {r.psych.map((p) => <div key={p.kind + p.ts} className="small"><span>{PSYCH[p.kind]?.[0] ?? p.kind}</span><b className="num">{PSYCH[p.kind]?.[1](p) ?? p.score}</b></div>)}
            {!r.psych.length && <span className="muted small">В этот день опросы не заполнялись.</span>}
          </div>
        </div>
      </div>
    </div>
  );
}
const ptsColor = (p: number | null) => (p == null || Math.abs(p) < 0.5 ? 'var(--muted)' : p > 0 ? '#3DDC97' : '#FF6B6B');

function StressMini({ st }: { st: StressDay }) {
  const c = stressColor(st.score ?? 0);
  return (
    <div className="inset" style={{ gap: 8 }}>
      <div className="between"><span className="small"><b>Стресс днём <span className="num" style={{ color: c }}>{st.score}</span></b> <span className="muted">· {st.level}{st.norm != null ? ` · обычно ${st.norm}` : ''} · {st.from}–{st.to}</span></span></div>
      <div className="stress-line" style={{ height: 44 }} aria-hidden="true">
        {st.bins.map(([t, v]) => <span key={t} style={{ height: `${Math.max(6, v)}%`, background: stressColor(v) }} />)}
      </div>
    </div>
  );
}

/** Readiness per day: bar = morning score, ring = evening score. */
function ReadyBars({ from, to, morning, evening, sel, onPick, count }: { from: string; to: string; morning: Record<string, number>; evening: Record<string, number>; sel: string; onPick: (d: string) => void; count: number }) {
  const dates: string[] = [];
  for (let d = from; d <= to; d = addDay(d, 1)) dates.push(d);
  if (!dates.some((d) => morning[d] != null)) return <span className="muted small">Пока нет оценок за этот период.</span>;
  return (
    <div className="stack" style={{ gap: 4 }}>
      <div className="sleepbars" style={{ height: 130 }}>
        <div className="needline" style={{ bottom: '67%' }}><span className="tiny muted">67</span></div>
        <div className="needline" style={{ bottom: '40%' }}><span className="tiny muted">40</span></div>
        {dates.map((d) => {
          const m = morning[d], e = evening[d];
          return (
            <div key={d} className={`sbar${d === sel ? ' sel' : ''}`} style={{ cursor: 'pointer' }} onClick={() => onPick(d)}
              title={`${dayLabel(d)}: утро ${m ?? '—'}${e != null ? ` · вечер ${e}` : ''}`}>
              {count <= 14 && <span className="num tiny">{m ?? '—'}</span>}
              <div className="rbar">
                {m != null && <div className="sstack" style={{ height: `${Math.max(2, m)}%`, background: rColor(m) }} />}
                {e != null && <span className="edot" style={{ bottom: `${e}%`, borderColor: rColor(e) }} />}
              </div>
              {count <= 14 && <span className="muted tiny">{wdOf(d)}</span>}
            </div>
          );
        })}
      </div>
      {count > 14 && <div className="between tiny muted"><span>{dayLabel(dates[0])}</span><span>{dayLabel(dates[dates.length - 1])}</span></div>}
      <div className="row tiny muted" style={{ gap: 14 }}><span>столбик — утро</span><span>кружок — вечер</span><span>нажми на день, чтобы открыть его наверху</span></div>
    </div>
  );
}

const HABIT_NAMES: Record<string, string> = { screens: 'экраны в кровати', sauna: 'сауна или баня', breath: 'медитация, дыхание', stretch: 'растяжка вечером' };

function History({ h, ov, onOpen, day, onDayUsed }: { h: HealthSummary; ov: HealthOverview | null; onOpen?: (id: string) => void; day?: string | null; onDayUsed?: () => void }) {
  const [days, setDays] = useState(30);
  const [sel, setSel] = useState<string>(day ?? h.night?.date ?? h.today);
  const [ready, setReady] = useState<Record<string, number>>({});
  const [eve, setEve] = useState<Record<string, number>>({});
  useEffect(() => {
    api.engineSummary().then((s) => {
      setReady(Object.fromEntries(s.readiness.history.filter((x) => x[1] != null) as [string, number][]));
      setEve(Object.fromEntries(s.readiness.eveningHistory ?? []));
    }).catch(() => {});
  }, []);
  useEffect(() => {
    if (day) { setSel(day); onDayUsed?.(); }
  }, [day]);
  const from = useMemo(() => addDay(h.today, -days + 1), [h.today, days]);
  const nights = h.nights.filter((n) => n.date >= from);
  const hrv = h.hrv.series.filter((p) => p[0] >= from);
  const rhr = h.rhr.series.filter((p) => p[0] >= from);
  const vo2 = h.vo2.series.filter((p) => p[0] >= from);
  const avg = (v: number[]) => (v.length ? v.reduce((a, b) => a + b, 0) / v.length : null);
  const sleepAvg = avg(nights.map((n) => n.totalS));
  const debt = nights.reduce((a, n) => a + Math.max(0, 8 * 3600 - n.totalS), 0);
  const range = (p: Pt[]) => (p.length ? `от ${n0(Math.min(...p.map((x) => x[1])))} до ${n0(Math.max(...p.map((x) => x[1])))}` : '');
  const w = h.body.weighings[0];
  return (
    <>
      <DayCard date={sel} today={h.today} ready={ready[sel] ?? null} onDate={setSel} onOpen={onOpen} />
      <div className="row" role="group" aria-label="Период">
        {PERIODS.map(([d, n]) => <button key={d} type="button" className="pill" aria-pressed={days === d} onClick={() => setDays(d)}>{n}</button>)}
        <span className="muted small">{dayLabel(from)} — {dayLabel(h.today)} · нажми на любой день на графиках, чтобы открыть его наверху</span>
      </div>
      <section className="card" aria-label="Готовность по дням">
        <span className="label">Готовность по дням</span>
        <ReadyBars from={from} to={h.today} morning={ready} evening={eve} sel={sel} onPick={setSel} count={days} />
      </section>
      <div className="tiles">
        <div className="tile"><span className="num tile-v">{sleepAvg ? hm(sleepAvg) : '—'}</span><span className="muted small">сон в среднем · цель 8:00</span></div>
        <div className="tile"><span className="num tile-v">{hmWords(debt / 60)}</span><span className="muted small">долг сна относительно 8 ч в ночь</span></div>
        <div className="tile"><span className="num tile-v">{n0(avg(hrv.map((p) => p[1])))} мс</span><span className="muted small">HRV в среднем · {range(hrv)}</span></div>
        <div className="tile"><span className="num tile-v">{n0(avg(rhr.map((p) => p[1])))}</span><span className="muted small">пульс покоя · {range(rhr)}</span></div>
        <div className="tile"><span className="num tile-v">{vo2.length ? n1(vo2[vo2.length - 1][1]) : '—'}</span><span className="muted small">VO₂max{vo2.length > 1 ? ` · за период ${vo2[vo2.length - 1][1] >= vo2[0][1] ? '+' : '−'}${n1(Math.abs(vo2[vo2.length - 1][1] - vo2[0][1]))}` : ''}</span></div>
        <div className="tile"><span className="num tile-v">{w?.fat != null ? `${n1(w.fat)}%` : '—'}</span><span className="muted small">жир в теле{w?.lean != null ? ` · безжировая ${n1(w.lean)} кг` : ''}</span></div>
      </div>
      <section className="card" aria-label="Сон по ночам">
        <div className="between"><span className="label">Сон по ночам</span><span className="row tiny" style={{ gap: 12 }}>{(['deep', 'core', 'rem'] as const).map((k) => <span key={k} className="row" style={{ gap: 4 }}><span className="dot" style={{ background: STAGE[k][1] }} />{STAGE[k][0]}</span>)}</span></div>
        <SleepBars nights={nights} needMin={480} count={days} sel={sel} onPick={setSel} />
      </section>
      <div className="grid2">
        <section className="card" aria-label="HRV ночью"><span className="label">{h.hrv.nightly ? 'HRV ночью · мс' : 'HRV по утрам · мс'}</span><LineBand pts={hrv} band={h.hrv.band} color="#3DDC97" fmt={(v) => String(Math.round(v))} sel={sel} onPick={setSel} marks={marksOf(h, "hrv")} /></section>
        <section className="card" aria-label="Пульс покоя"><span className="label">{h.rhr.nightly ? 'Пульс покоя ночью · уд/мин' : 'Пульс покоя · уд/мин'}</span><LineBand pts={rhr} band={h.rhr.band} color="#FF8A65" fmt={(v) => String(Math.round(v))} invertGood sel={sel} onPick={setSel} marks={marksOf(h, "rhr")} /></section>
      </div>
      <section className="card" aria-label="Стресс по дням">
        <span className="label">Стресс днём · 0–100</span>
        <LineBand pts={(h.stress?.series ?? []).filter((p) => p[0] >= from)} band={h.stress?.norm != null ? [h.stress.norm, Math.max(0, h.stress.norm - 8), h.stress.norm + 8] : null} color="#F5B642" fmt={(v) => String(Math.round(v))} invertGood sel={sel} onPick={setSel} />
        <span className="muted tiny">Полоса — твой обычный уровень за 30 дней. Выше — день был напряжённее обычного.</span>
      </section>
      <div className="grid2">
        <section className="card" aria-label="Ранние сигналы по дням">
          <div className="between"><span className="label">Ранние сигналы · 14 дней</span><span className="row tiny" style={{ gap: 10 }}>{['норма', 'отклонение', 'сильное'].map((t, i) => <span key={t} className="row" style={{ gap: 4 }}><span className="dot" style={{ background: LVL[i] }} />{t}</span>)}</span></div>
          <div className="heat">
            <span />
            {h.signalMatrix[0]?.days.map((d) => <button key={d[0]} type="button" className={`plain tiny ${d[0] === sel ? '' : 'muted'}`} style={{ textAlign: 'center', fontWeight: d[0] === sel ? 700 : 400 }} onClick={() => setSel(d[0])}>{d[0].slice(8)}</button>)}
            {h.signalMatrix.map((r) => (
              <Fragment key={r.key}>
                <span className="small">{r.name}</span>
                {r.days.map((d) => <button key={d[0]} type="button" className={`hcell${d[0] === sel ? ' sel' : ''}`} title={`${dayLabel(d[0])}: ${d[1] ?? '—'}`} onClick={() => setSel(d[0])} style={{ background: d[2] < 0 ? '#1c2026' : LVL[d[2]], opacity: d[2] === 0 ? 0.55 : 1 }} />)}
              </Fragment>
            ))}
          </div>
        </section>
        <section className="card" aria-label="VO2max по часам">
          <span className="label">VO₂max по часам · мл/кг/мин</span>
          <LineBand pts={vo2} band={h.vo2.lab ? [h.vo2.lab.value, h.vo2.lab.value - 0.15, h.vo2.lab.value + 0.15] : null} color="#9DB8FF" fmt={(v) => n1(v)} sel={sel} onPick={setSel} />
          <span className="muted tiny">{h.vo2.lab ? `Узкая полоса — лаборатория ${n1(h.vo2.lab.value)} (${dayLabel(h.vo2.lab.date)}). ` : ''}Реальный рост формы виден на месячном и трёхмесячном графиках.</span>
        </section>
      </div>
      <BodyChart h={h} sel={sel} onPick={setSel} />
      {ov && (
        <section className="card" aria-label="Что загружено">
          <span className="label">Что загружено из Apple Health</span>
          <div className="row" style={{ gap: 24 }}>
            <span className="small"><span className="muted">Записей</span> <b>{ov.stats.rows.toLocaleString('ru-RU')}</b></span>
            <span className="small"><span className="muted">Показателей</span> <b>{ov.stats.metrics}</b></span>
            <span className="small"><span className="muted">Период</span> <b>{ov.stats.firstDate ?? '—'} — {ov.stats.lastDate ?? '—'}</b></span>
          </div>
          <span className="muted tiny">Тренировки из Apple Health не загружаются — они приходят только из Strava, чтобы не было двойного счёта.</span>
        </section>
      )}
    </>
  );
}
