import { StressCard } from '../components/Stress';
import { MedNotes, MedRow } from './Meds';
import { Vital, pointsOf, fmtPts } from '../components/Readiness';
import { useCallback, useEffect, useState } from 'react';
import { CheckinCard, MorningPain } from '../components/Checkin';
import { DupCards, StravaGoneCards } from '../components/Workouts';
import { DeviceChangeCard, ManualEntryCard } from '../components/DataSources';
import { DayPlanCard } from '../components/DayPlan';
import { ShoeTodayCard } from '../components/Shoes';
import type { Go } from '../lib/nav';
import { api, errText, type EngineSummary, type HealthSummary, type MedsSummary, type CoachReport, type PlanState, type ShoeAdvice, type BodyState, type PsychSummary, type Split } from '../lib/api';
import { SportIcon } from '../components/SportIcon';
import { useDataChanged } from '../lib/events';

const LEVEL: Record<string, [string, string]> = {
  green: ['#3DDC97', 'Готов к нагрузке'],
  yellow: ['#F5B642', 'Средняя готовность'],
  red: ['#FF6B6B', 'Нужно восстановление'],
  unknown: ['#6B737E', 'Мало данных'],
};

const LOAD: Record<string, string> = { unknown: '#6B737E', low: '#7FD3FF', optimal: '#3DDC97', high: '#F5B642', very_high: '#FF6B6B' };

const n0 = (x: number | null | undefined) => (x == null ? '—' : Math.round(x).toLocaleString('ru-RU'));
const n1 = (x: number | null | undefined) => (x == null ? '—' : x.toFixed(1).replace('.', ','));
function hm(s: number) {
  const h = Math.floor(s / 3600), m = Math.round((s % 3600) / 60);
  return h ? `${h} ч ${m} мин` : `${m} мин`;
}
function clock(s: number) {
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), sec = Math.round(s % 60);
  return h ? `${h}:${String(m).padStart(2, '0')}:${String(sec).padStart(2, '0')}` : `${m}:${String(sec).padStart(2, '0')}`;
}
const DIST: Record<number, string> = { 1000: '1 км', 5000: '5 км', 10000: '10 км', 21097: 'Полумарафон', 21097.5: 'Полумарафон', 42195: 'Марафон' };
const dayLabel = (d: string) => new Date(d + 'T12:00:00').toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' });

function Ring({ value, color }: { value: number | null; color: string }) {
  const r = 70, c = 2 * Math.PI * r, frac = value == null ? 0 : Math.max(0.02, value / 100);
  return (
    <div style={{ position: 'relative', width: 168, height: 168, flex: 'none' }}>
      <svg viewBox="0 0 168 168" width="168" height="168" aria-hidden="true">
        <circle cx="84" cy="84" r={r} fill="none" stroke="#22262D" strokeWidth="14" />
        <circle cx="84" cy="84" r={r} fill="none" stroke={color} strokeWidth="14" strokeLinecap="round"
          strokeDasharray={`${c * frac} ${c}`} transform="rotate(-90 84 84)" />
      </svg>
      {/* The number is HTML on top of the ring — text inside SVG is avoided on purpose. */}
      <div style={{ position: 'absolute', inset: 0, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center' }}>
        <span className="num" style={{ fontSize: 40, fontWeight: 700 }}>{value ?? '—'}</span>
        <span className="muted tiny">из 100</span>
      </div>
    </div>
  );
}

function Bar({ v, max, color }: { v: number; max: number; color: string }) {
  return (
    <div style={{ height: 8, borderRadius: 999, background: '#22262D', overflow: 'hidden' }}>
      <div style={{ width: `${Math.max(0, Math.min(100, (v / Math.max(max, 1)) * 100))}%`, height: '100%', background: color, borderRadius: 999 }} />
    </div>
  );
}

function SplitBar({ s }: { s: Split | null }) {
  if (!s) return <span className="muted small">Мало тренировок с пульсом</span>;
  return (
    <div className="stack" style={{ gap: 6 }}>
      <div style={{ display: 'flex', height: 14, borderRadius: 999, overflow: 'hidden', background: '#22262D' }}>
        <div style={{ width: `${s.low}%`, background: '#3DDC97' }} />
        <div style={{ width: `${s.mid}%`, background: '#F5B642' }} />
        <div style={{ width: `${s.high}%`, background: '#FF6B6B' }} />
      </div>
      <div className="row tiny" style={{ gap: 14 }}>
        <span><b style={{ color: '#3DDC97' }}>{Math.round(s.low)}%</b> лёгкая (Z1–Z2)</span>
        <span><b style={{ color: '#F5B642' }}>{Math.round(s.mid)}%</b> темповая (Z3)</span>
        <span><b style={{ color: '#FF6B6B' }}>{Math.round(s.high)}%</b> тяжёлая (Z4–Z5)</span>
        <span className="muted">{n1(s.hours)} ч с пульсом</span>
      </div>
    </div>
  );
}

function Spark({ series }: { series: [string, number, number, number, number][] }) {
  if (series.length < 2) return null;
  const W = 520, H = 90, pad = 4;
  const ys = series.flatMap((d) => [d[2], d[3]]);
  const max = Math.max(...ys, 1), min = Math.min(...ys, 0);
  const x = (i: number) => pad + (i / (series.length - 1)) * (W - 2 * pad);
  const y = (v: number) => H - pad - ((v - min) / (max - min || 1)) * (H - 2 * pad);
  const line = (k: 2 | 3) => series.map((d, i) => `${x(i).toFixed(1)},${y(d[k]).toFixed(1)}`).join(' ');
  return (
    <svg viewBox={`0 0 ${W} ${H}`} width="100%" height={H} preserveAspectRatio="none" aria-hidden="true">
      <polyline points={line(3)} fill="none" stroke="#9DB8FF" strokeWidth="2" />
      <polyline points={line(2)} fill="none" stroke="#FF8A65" strokeWidth="2" />
    </svg>
  );
}

function Details({ s, onSettings }: { s: EngineSummary; onSettings: () => void }) {
  const night = s.sleep[0];
  const fed = s.nutrition.days.slice().reverse().find((d) => d.kcal != null && d.date < s.today);
  const tg = s.nutrition.targetToday;
  const w = s.body.weight;
  return (
    <>
      <div className="grid">
        <section className="card" aria-label="Сон">
          <span className="label">Сон</span>
          {night ? (
            <>
              <div className="between">
                <span><span className="num" style={{ fontSize: 28, fontWeight: 700 }}>{hm(night.totalS)}</span> <span className="muted small">ночь на {dayLabel(night.date)}</span></span>
                <span className="chip" style={{ background: '#1B1F25', color: night.score >= 80 ? '#3DDC97' : night.score >= 60 ? '#F5B642' : '#FF6B6B' }}>оценка {night.score}</span>
              </div>
              <div style={{ display: 'flex', height: 14, borderRadius: 999, overflow: 'hidden' }}>
                <div style={{ width: `${(night.deepS / night.totalS) * 100}%`, background: '#5B7CFA' }} />
                <div style={{ width: `${(night.coreS / night.totalS) * 100}%`, background: '#7FD3FF' }} />
                <div style={{ width: `${(night.remS / night.totalS) * 100}%`, background: '#C792EA' }} />
              </div>
              <div className="row tiny" style={{ gap: 14 }}>
                <span><b style={{ color: '#5B7CFA' }}>глубокий</b> {hm(night.deepS)}</span>
                <span><b style={{ color: '#7FD3FF' }}>основной</b> {hm(night.coreS)}</span>
                <span><b style={{ color: '#C792EA' }}>быстрый</b> {hm(night.remS)}</span>
                <span className="muted">пробуждения {hm(night.awakeS)}</span>
              </div>
              <div className="row" style={{ gap: 6, alignItems: 'flex-end', height: 70 }}>
                {s.sleep.slice(0, 10).reverse().map((n) => (
                  <div key={n.date} title={`${dayLabel(n.date)}: ${hm(n.totalS)}`} style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 4 }}>
                    <div style={{ width: '100%', height: `${Math.min(1, n.totalS / 36000) * 52}px`, borderRadius: 6, background: n.totalS >= 25200 ? '#3DDC97' : n.totalS >= 21600 ? '#F5B642' : '#FF6B6B' }} />
                    <span className="tiny muted">{n.date.slice(8)}</span>
                  </div>
                ))}
              </div>
            </>
          ) : <span className="muted">Нет данных сна из Apple Health</span>}
        </section>

        <section className="card" aria-label="Питание">
          <div className="between"><span className="label">Питание</span><span className="muted tiny">цели на сегодня · вес {n1(s.nutrition.weightUsed)} кг</span></div>
          {fed ? (
            <div className="stack" style={{ gap: 10 }}>
              <span className="small muted">Съедено {dayLabel(fed.date)} против цели того дня:</span>
              {([['Калории', fed.kcal, fed.target.kcal, 'ккал', '#F5B642'], ['Белок', fed.protein, fed.target.protein, 'г', '#3DDC97'], ['Углеводы', fed.carbs, fed.target.carbs, 'г', '#7FD3FF'], ['Жиры', fed.fat, fed.target.fat, 'г', '#FF8A65'], ['Клетчатка', fed.fiber, fed.target.fiber, 'г', '#C792EA'], ['Вода', fed.waterMl, fed.target.waterMl, 'мл', '#9DB8FF']] as [string, number | null, number, string, string][]).map(([n, v, t, u, c]) => (
                <div key={n} className="stack" style={{ gap: 4 }}>
                  <div className="between small"><span>{n}</span><span className="muted tiny">{n0(v)} из {n0(t)} {u}</span></div>
                  <Bar v={v ?? 0} max={t} color={c} />
                </div>
              ))}
            </div>
          ) : <span className="muted">Нет записей еды за последние дни</span>}
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>
            Сегодня: {n0(tg.kcal)} ккал · белок {n0(tg.protein)} г · углеводы {n0(tg.carbs)} г · жиры {n0(tg.fat)} г · вода {n1(tg.waterMl / 1000)} л.
            Цель по весу (поддержание, снижение или набор) меняется в «Питании» → «Моя цель».
          </span>
        </section>
      </div>

      <div className="grid">
        <section className="card" aria-label="Интенсивность">
          <span className="label">Распределение интенсивности</span>
          <div className="stack" style={{ gap: 6 }}><span className="small" style={{ fontWeight: 600 }}>7 дней</span><SplitBar s={s.intensity.d7} /></div>
          <div className="stack" style={{ gap: 6 }}><span className="small" style={{ fontWeight: 600 }}>28 дней</span><SplitBar s={s.intensity.d28} /></div>
          <span className="muted tiny">Зоны пульса: {s.profile.zones.join(' · ')} уд/мин. Изменить — Настройки → Профиль и зоны.</span>
        </section>

        <section className="card" aria-label="Рекорды и прогнозы">
          <span className="label">Рекорды и прогнозы</span>
          <div>
            {s.records.map((r) => (
              <div key={r.distanceM} className="list-row small"><span>{DIST[r.distanceM] ?? `${r.distanceM} м`}</span><span className="muted tiny">{dayLabel(r.date)}</span><b className="num">{clock(r.timeS)}</b></div>
            ))}
            {s.records.length === 0 && <div className="list-row muted small">Нет пробежек с графиком дистанции</div>}
          </div>
          {s.predictions.length > 0 && (
            <>
              <span className="small" style={{ fontWeight: 600 }}>Прогноз (формула Ригеля)</span>
              <div className="row" style={{ gap: 10 }}>
                {s.predictions.map((p) => (
                  <div key={p.distanceM} className="tile" style={{ flex: 1, minWidth: 110 }}><span className="num" style={{ fontSize: 18, fontWeight: 700 }}>{clock(p.timeS)}</span><span className="muted tiny">{DIST[p.distanceM] ?? p.distanceM}</span></div>
                ))}
              </div>
              <span className="muted tiny">По лучшему отрезку {DIST[s.predictions[0].fromDistanceM] ?? ''} за {clock(s.predictions[0].fromTimeS)} ({dayLabel(s.predictions[0].fromDate)}) за последние 120 дней.</span>
            </>
          )}
        </section>
      </div>

      <div className="grid">
        <section className="card" aria-label="Тело">
          <span className="label">Тело</span>
          <div className="tiles4" style={{ gridTemplateColumns: 'repeat(3, minmax(0, 1fr))' }}>
            <div className="tile"><span className="num tile-v">{n1(w.smooth)}</span><span className="muted tiny">вес, кг (тренд)</span></div>
            <div className="tile"><span className="num tile-v">{w.change7d == null ? '—' : `${w.change7d > 0 ? '+' : ''}${n1(w.change7d)}`}</span><span className="muted tiny">за 7 дней</span></div>
            <div className="tile"><span className="num tile-v">{w.change30d == null ? '—' : `${w.change30d > 0 ? '+' : ''}${n1(w.change30d)}`}</span><span className="muted tiny">за 30 дней</span></div>
          </div>
          <span className="muted tiny">
            Последнее взвешивание: {w.latest ? `${n1(w.latest[1])} кг, ${dayLabel(w.latest[0])}` : '—'}
            {s.body.fatPct.smooth != null ? ` · жир ${n1(s.body.fatPct.smooth > 1 ? s.body.fatPct.smooth : s.body.fatPct.smooth * 100)}%` : ''}
            {s.body.lean.smooth != null ? ` · безжировая масса ${n1(s.body.lean.smooth)} кг` : ''}
          </span>
        </section>
        <section className="card" aria-label="Недели">
          <span className="label">Недели</span>
          <div>
            {s.weeks.slice().reverse().slice(0, 8).map((wk) => (
              <div key={wk.week} className="list-row small">
                <span className="muted" style={{ width: 70 }}>{dayLabel(wk.week)}</span>
                <div style={{ flex: 1 }}><Bar v={wk.load} max={Math.max(...s.weeks.map((x) => x.load))} color="#3DDC97" /></div>
                <span className="num tiny" style={{ width: 150, textAlign: 'right' }}>{n0(wk.load)} · {n1(wk.hours)} ч · бег {n1(wk.runKm)} км</span>
              </div>
            ))}
          </div>
        </section>
      </div>

      <p className="muted tiny" style={{ textAlign: 'center', margin: 0 }}>
        Тренировок посчитано: {s.meta.activities} — по графику пульса {s.meta.byHr}, по среднему пульсу {s.meta.byAvgHr}, оценкой по времени {s.meta.estimated} · {s.meta.ms} мс ·{' '}
        <button type="button" className="linkbtn" onClick={onSettings}>профиль и зоны</button>
      </p>
    </>
  );
}

// ---------- the day, as in the concept ----------

const WD = ['Пн', 'Вт', 'Ср', 'Чт', 'Пт', 'Сб', 'Вс'];
const SPORT_RU: Record<string, string> = {
  run: 'бег', walk: 'ходьба', hike: 'поход', ride: 'вело', ride_virtual: 'вело', strength: 'силовая',
  workout: 'физио', yoga: 'йога', swim: 'плавание', elliptical: 'эллипс', stepper: 'степпер', other: 'другое',
};

function MiniRing({ frac, color, label }: { frac: number; color: string; label: string }) {
  const r = 54, c = 2 * Math.PI * r;
  return (
    <div style={{ position: 'relative', flex: 'none', width: 132, height: 132 }}>
      <svg width="132" height="132" viewBox="0 0 132 132" aria-hidden="true">
        <circle cx="66" cy="66" r={r} fill="none" stroke="#22262D" strokeWidth="12" />
        <circle cx="66" cy="66" r={r} fill="none" stroke={color} strokeWidth="12" strokeLinecap="round" transform="rotate(-90 66 66)"
          strokeDasharray={`${c * Math.max(0.015, Math.min(1, frac))} ${c}`} />
      </svg>
      <span className="num" style={{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 28, fontWeight: 700 }}>{label}</span>
    </div>
  );
}

function loadPhase(acwr: number | null): [number, string, string] {
  if (acwr == null) return [-1, 'Мало истории', '#6B737E'];
  if (acwr < 0.8) return [0, 'Восстановление', '#4FA3FF'];
  if (acwr < 1.0) return [1, 'Поддержка формы', '#9AA3AE'];
  if (acwr <= 1.3) return [2, 'Форма растёт', '#3DDC97'];
  return [3, 'Перегрузка', '#FF6B6B'];
}

function injuryRisk(acwr: number | null): [string, string] {
  if (acwr == null) return ['—', '#9AA3AE'];
  if (acwr > 1.5) return ['высокий', '#FF6B6B'];
  if (acwr > 1.3) return ['повышенный', '#F5B642'];
  return ['низкий', '#3DDC97'];
}

const HIDE_KEY = 'kouros_hidden_reminders';
const daysAgo = (n: number) => { const m10 = n % 10, m100 = n % 100; return `${n} ${m10 === 1 && m100 !== 11 ? 'день' : m10 >= 2 && m10 <= 4 && (m100 < 10 || m100 >= 20) ? 'дня' : 'дней'} назад`; };
/** Reminders closed with × stay hidden for 7 days (0.21, owner). */
function loadHidden(): Record<string, number> {
  try {
    const v = JSON.parse(localStorage.getItem(HIDE_KEY) ?? '{}') as Record<string, number>;
    const now = Date.now();
    return Object.fromEntries(Object.entries(v).filter(([, t]) => typeof t === 'number' && t > now));
  } catch { return {}; }
}

/** Where «Подробнее →» of a reminder leads (1.0). */
const SIGNAL_GO: Record<string, Go> = {
  rhr_up: { tab: 'health', card: 'Ранние сигналы' }, hrv_down: { tab: 'health', card: 'Ранние сигналы' },
  low_rhr_low_hrv: { tab: 'health', card: 'Ранние сигналы' }, breath_up: { tab: 'health', card: 'Ранние сигналы' },
  sleep_short: { tab: 'health', card: 'План сна' }, sleep_chronic: { tab: 'health', card: 'План сна' },
  load_high: { tab: 'training', card: 'Нагрузка: сердце и мышцы' }, load_spike: { tab: 'training', card: 'Нагрузка: сердце и мышцы' },
  food_missing: { tab: 'nutrition' }, weight_trend: { tab: 'health', sub: 'history', card: 'Состав тела' },
  hr_over_max: { tab: 'settings', sub: 'prof' }, zones_old: { tab: 'settings', sub: 'prof' },
};

export function Today({ onSettings, onTraining, onHealth, onPsych, onHealthDay, onMeds, onCoach, onProfile, onGear, go }: { go?: (g: Go) => void; onGear?: () => void; onProfile?: () => void; onSettings: () => void; onTraining: () => void; onHealth: () => void; onPsych?: () => void; onHealthDay?: (d: string) => void; onMeds?: () => void; onCoach?: () => void }) {
  const [s, setS] = useState<EngineSummary | null>(null);
  const [hs, setHs] = useState<HealthSummary | null>(null);
  const [ps, setPs] = useState<PsychSummary | null>(null);
  const [meds, setMeds] = useState<MedsSummary | null>(null);
  const [coach, setCoach] = useState<CoachReport | null>(null);
  const [plan, setPlan] = useState<PlanState | null>(null);
  const [body, setBody] = useState<BodyState | null>(null);
  const [shoe, setShoe] = useState<ShoeAdvice | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [fOpen, setFOpen] = useState(true);
  const [allOpen, setAllOpen] = useState(false);
  const [hidden, setHidden] = useState<Record<string, number>>(loadHidden);
  const hide = (code: string) => {
    const next = { ...hidden, [code]: Date.now() + 7 * 86400000 };
    setHidden(next);
    try { localStorage.setItem(HIDE_KEY, JSON.stringify(next)); } catch { /* storage unavailable */ }
  };
  const load = useCallback(() => {
    api.engineSummary().then((v) => { setS(v); setErr(null); }).catch((e) => setErr(errText(e)));
    api.healthSummary().then(setHs).catch(() => {});
    api.psychSummary().then(setPs).catch(() => {});
    api.medsSummary().then(setMeds).catch(() => {});
    api.planState().then((p) => { setPlan(p); api.shoeAdvice().then(setShoe).catch(() => {}); }).catch(() => {});
    api.bodyState().then(setBody).catch(() => {});
    api.coachState().then((c) => setCoach(c.reports.find((r) => r.kind !== 'claude' && r.body.headline) ?? null)).catch(() => {});
  }, []);
  useEffect(load, [load]);
  useDataChanged(load);

  if (err) return <div className="note-err">{err}</div>;
  if (!s) return <div className="muted">Считаю…</div>;

  // After midnight, until tonight's sleep is recorded, the ring shows a night phrase instead of a number.
  const ns = hs?.nightState;
  const useY = !!ns?.useYesterday;
  // In the evening (8 h after waking, 18:00–22:00) the ring shows the evening score: the frozen morning one ± what the day did.
  const ev = s.readiness.evening;
  const eve = !useY && !!ev && !!s.readiness.eveningOn;
  const rScore = useY ? null : eve ? ev!.score : s.readiness.score;
  const [lc, lt] = useY ? ['#A78BFA', 'Ночь — время восстановления'] : LEVEL[eve ? ev!.level : s.readiness.level] ?? LEVEL.unknown;
  const day = s.day;
  const tg = day.target ?? { lo: day.targetLoad * 0.67, hi: day.targetLoad, headroom: null, capped: false, cap: 1.3, left: 0 };
  const night = s.sleep.find((n) => n.date === s.today) ?? s.sleep[0];
  const nightIsToday = night?.date === s.today;
  const [phaseIdx, phaseText, phaseColor] = loadPhase(s.load.acwr);
  // Injury risk: the worse of heart and muscles, with the reason.
  const mus = s.load.muscle;
  const mAcwr = mus?.acwr ?? null;
  const riskFrom = mAcwr != null && (s.load.acwr == null || mAcwr > s.load.acwr) ? 'muscle' : 'heart';
  const [riskText, riskColor] = injuryRisk(riskFrom === 'muscle' ? mAcwr : s.load.acwr);
  const rec = s.recovery;
  const weekday = (d: string) => (new Date(d + 'T12:00:00').getDay() + 6) % 7;
  // Survey reminders (0.21): BRUMS and stress/recovery once a week.
  const surveyRem: { level: 'info'; code: string; text: string; psych: true }[] = [];
  if (ps) {
    const b = ps.due.brumsDays, r = ps.due.stressrecDays;
    if (b == null || b >= 7) surveyRem.push({ level: 'info', code: 'survey_brums', text: b == null ? 'Пройди профиль настроения BRUMS — 2–3 минуты, раз в неделю.' : `Профиль настроения BRUMS проходил ${daysAgo(b)} — пора повторить (2–3 минуты).`, psych: true });
    if (r == null || r >= 7) surveyRem.push({ level: 'info', code: 'survey_stressrec', text: r == null ? 'Пройди анкету «Баланс стресса и восстановления» — 1 минута, раз в неделю.' : `Анкету «Баланс стресса и восстановления» проходил ${daysAgo(r)} — пора повторить (1 минута).`, psych: true });
  }
  const now = Date.now();
  const reminders: { level: string; code: string; text: string; psych?: boolean }[] = [...s.signals, ...surveyRem].filter((g) => !(hidden[`${g.code}:${g.level}`] > now));
  const dateTitle = new Date(s.today + 'T12:00:00').toLocaleDateString('ru-RU', { day: 'numeric', month: 'long' });
  const weekTitle = `${dayLabel(day.week[0].date)} — ${dayLabel(day.week[6].date)}`;
  const recNote = s.readiness.level === 'unknown'
    ? 'Нужно несколько ночей с часами, чтобы понять твою норму'
    : (s.readiness.components.filter((c) => c.score != null).sort((a, b) => Math.abs((b.score ?? 50) - 50) - Math.abs((a.score ?? 50) - 50))[0]?.note ?? '');

  // Factors: deviation from your norm, centred bar.
  const groups: { title: string; note: string; keys: string[] }[] = [
    { title: 'Восстановление', note: 'нервная система и сердце', keys: ['hrv', 'rhr'] },
    { title: 'Сон и дыхание', note: 'последняя ночь', keys: ['sleep', 'resp', 'breath'] },
    { title: 'Нагрузка', note: 'по тренировкам', keys: ['form'] },
  ];

  return (
    <>
      <div className="page-head">
        <div className="stack">
          <h1>Сегодня</h1>
          <p className="subtitle">{dateTitle} · {lt.toLowerCase()}</p>
        </div>
        <button type="button" className="btn2" onClick={load}>Обновить</button>
      </div>

      {day.alcoholYesterday != null && day.alcoholYesterday > 0 && (
        <button type="button" className="banner" onClick={() => (go ? go({ tab: 'health', card: 'Привычки' }) : onHealth())}>
          <span className="banner-ic">🍷</span>
          <span style={{ flex: '1 1 360px', display: 'flex', flexDirection: 'column', gap: 2, textAlign: 'left' }}>
            <b>Восстановление после алкоголя</b>
            <span className="small" style={{ color: 'var(--text-2)' }}>
              Вчера: {n1(day.alcoholYesterday)} {day.alcoholYesterday === 1 ? 'порция' : 'порции'}. Алкоголь снижает HRV и качество сна на 1–2 ночи — сегодня без интенсива, больше воды.
            </span>
          </span>
          <span style={{ fontWeight: 700, color: '#F5B642' }}>Подробнее →</span>
        </button>
      )}

      {ns && hs && ns.phase !== 'day' && <NightBanner hs={hs} onHealth={() => (go ? go({ tab: 'health', card: 'План сна' }) : onHealth())} />}

      <div className="rings">
        <section className="card ring-card" aria-label="Готовность">
          {useY ? (
            <>
              <div className="sleepy" aria-hidden="true">😴</div>
              <div className="stack">
                <span className="label">Готовность</span>
                <span style={{ fontSize: 19, fontWeight: 700, color: '#A78BFA', lineHeight: 1.35 }}>{nightJoke(s.today)}</span>
              </div>
            </>
          ) : (
            <>
              <MiniRing frac={(rScore ?? 0) / 100} color={lc} label={rScore != null ? String(rScore) : '—'} />
              <div className="stack">
                <span className="label">{eve ? 'Готовность · вечер' : 'Готовность'}</span>
                <span style={{ fontSize: 20, fontWeight: 700, color: lc }}>{lt}</span>
                <span className="muted small" style={{ lineHeight: 1.45 }}>
                  {eve ? `Утром ${ev!.morning} · за день ${ev!.delta > 0 ? '+' : ev!.delta < 0 ? '−' : '±'}${Math.abs(ev!.delta)}` : recNote}
                </span>
              </div>
            </>
          )}
        </section>
        <section className="card ring-card" aria-label="Нагрузка дня">
          <MiniRing frac={tg.hi ? day.todayLoad / tg.hi : 0} color="#4FA3FF" label={n0(day.todayLoad)} />
          <div className="stack">
            <span className="label">Нагрузка дня</span>
            <span style={{ fontSize: 20, fontWeight: 700, color: '#4FA3FF' }}>Цель {n0(tg.lo)}–{n0(tg.hi)}</span>
            <span className="muted small" style={{ lineHeight: 1.45 }}>
              {day.todayActivities.length
                ? `Учтено: ${day.todayActivities.map((a) => SPORT_RU[a.sport] ?? a.sport).join(', ')}`
                : `Тренировок сегодня пока нет · обычно ${n0(day.usualLoad)} в день`}
            </span>
            {tg.capped && (
              <span className="tiny" style={{ color: '#F5B642', lineHeight: 1.4 }}>
                Верх ограничен запасом недели: неделя не должна превышать обычную больше чем в {String(tg.cap).replace('.', ',')} раза
              </span>
            )}
          </div>
        </section>
        {s.steps && (s.steps.today != null || s.steps.usual != null) && (
          <section className="card ring-card" aria-label="Шаги">
            <MiniRing frac={s.steps.usual ? Math.min(1, (s.steps.today ?? 0) / s.steps.usual) : 0} color="#7FD3FF" label={s.steps.today != null ? n0(s.steps.today) : '—'} />
            <div className="stack">
              <span className="label">Шаги</span>
              <span style={{ fontSize: 20, fontWeight: 700, color: '#7FD3FF' }}>
                {s.steps.usual ? (s.steps.today != null ? `${Math.round((s.steps.today / s.steps.usual) * 100)}% от обычного` : 'пока нет данных') : 'норма набирается'}
              </span>
              <span className="muted small" style={{ lineHeight: 1.45 }}>
                {s.steps.usual ? `Обычно ${n0(s.steps.usual)} в день` : ''}{s.steps.avg7 ? `${s.steps.usual ? ' · ' : ''}за 7 дней в среднем ${n0(s.steps.avg7)}` : ''}
              </span>
            </div>
          </section>
        )}
        <section className="card ring-card" aria-label="Сон">
          <MiniRing frac={(night?.score ?? 0) / 100} color="#A78BFA" label={night ? `${night.score}%` : '—'} />
          <div className="stack">
            <span className="label">Сон{night && !nightIsToday ? ` · ${dayLabel(night.date)}` : ''}</span>
            <span style={{ fontSize: 20, fontWeight: 700, color: '#A78BFA' }}>{night ? hm(night.totalS) : 'нет данных'}</span>
            <span className="muted small" style={{ lineHeight: 1.45 }}>
              {night ? `Потребность 8 ч · глубокий ${hm(night.deepS)} · быстрый ${hm(night.remS)}` : 'Apple Watch ещё не прислали сон'}
            </span>
          </div>
        </section>
      </div>

      {!useY && <StressCard st={s.stress?.today} />}
      {s.profile.filled === false && (
        <section className="card" aria-label="Заполни профиль" style={{ borderColor: '#3DDC9755' }}>
          <div className="between"><span className="label" style={{ color: '#3DDC97' }}>Добро пожаловать в Kouros</span>{onProfile && <button type="button" className="btn" onClick={onProfile}>Заполнить профиль</button>}</div>
          <span className="small" style={{ lineHeight: 1.5 }}>Расскажи о себе: имя, год рождения, рост, максимальный пульс и пороговый темп. Пока профиль не заполнен, зоны, нагрузка и калории считаются по общим формулам.</span>
        </section>
      )}
      <DupCards max={1} />
      <StravaGoneCards />
      {(s.deviceChange?.length ?? 0) > 0 && <DeviceChangeCard changes={s.deviceChange!} onDone={load} />}
      {s.manualEntry && <ManualEntryCard key={s.manualEntry.today + String(s.manualEntry.values.hrv)} m={s.manualEntry} onDone={load} />}
      {body && body.pending[0] && <CheckinCard key={body.pending[0].id} a={body.pending[0]} areas={body.areas} suggest={shoe?.picks[0]?.id ?? null} onDone={() => api.bodyState().then(setBody).catch(() => {})} />}
      {/* 1.3.2 (owner): asked after the night is in (morning readiness frozen) or from 15:00, not at midnight. */}
      {body && (s.readiness.frozen || new Date().getHours() >= 15) && <MorningPain b={body} today={s.today} onDone={() => api.bodyState().then(setBody).catch(() => {})} />}
      {meds && meds.enabled !== false && (meds.items.some((m) => m.active) || (meds.notes?.length ?? 0) > 0) && (
        <section className="card" aria-label="Витамины и лекарства сегодня">
          <div className="between"><span className="label">Витамины и лекарства сегодня</span>{onMeds && <button type="button" className="linkbtn small" onClick={onMeds}>Все →</button>}</div>
          {meds.items.filter((m) => m.active && m.schedule !== 'asneeded').map((m) => <MedRow key={m.id} m={m} compact onChanged={() => api.medsSummary().then(setMeds).catch(() => {})} />)}
          <MedNotes s={meds} todayOnly />
          {meds.warnings.length > 0 && <span className="small" style={{ color: '#F5B642' }}>За неделю {meds.warnings.length === 1 ? 'одно вещество выше' : 'несколько веществ выше'} безопасного предела — подробности в разделе.</span>}
        </section>
      )}

      <section className="card" aria-label="Что повлияло на готовность">
        <button type="button" className="collapse" aria-expanded={fOpen} onClick={() => setFOpen(!fOpen)}>
          <span className="row" style={{ gap: 14, flexWrap: 'nowrap', minWidth: 0, flex: '1 1 420px' }}>
            <span className="chev" style={{ transform: fOpen ? 'rotate(180deg)' : 'none' }}>⌄</span>
            <span className="stack" style={{ gap: 4, minWidth: 0 }}>
              <span className="label">Что повлияло на готовность</span>
              <span className="muted small">Сравнение с твоей нормой за 30 дней</span>
            </span>
          </span>
          <span className="row" style={{ gap: 12 }}>
            <span className="stack" style={{ gap: 0, alignItems: 'flex-end' }}><span className="muted tiny">обычно</span><span className="num" style={{ fontSize: 22, fontWeight: 700, color: 'var(--muted)' }}>{day.usualReadiness ?? '—'}</span></span>
            <span className="muted">→</span>
            <span className="stack" style={{ gap: 0 }}><span className="muted tiny">{eve ? 'вечер' : 'сегодня'}</span><span className="num" style={{ fontSize: 22, fontWeight: 700, color: lc }}>{rScore ?? '—'}</span></span>
          </span>
        </button>
        <div className="vitals">
          {day.vitals.filter((v) => v.value != null).map((v) => <Vital key={v.key} v={v} />)}
        </div>
        {fOpen && (
          <>
            {eve && ev!.items.length > 0 && (
              <div className="inset" style={{ gap: 8 }}>
                <div className="between"><b className="small">За день · от утренних {ev!.morning}</b><span className="muted tiny">не больше ±15</span></div>
                <div className="eve-items">
                  {ev!.items.map((it) => (
                    <div key={it.text} className="small">
                      <span>{it.text}</span>
                      <b className="num" style={{ color: it.points > 0 ? '#3DDC97' : it.points < 0 ? '#FF6B6B' : 'var(--muted)' }}>{it.points > 0 ? '+' : it.points < 0 ? '−' : ''}{String(Math.abs(it.points)).replace('.', ',')}</b>
                    </div>
                  ))}
                </div>
              </div>
            )}
            <div className="factor-groups">
              {groups.map((g) => (
                <div key={g.title} className="inset" style={{ gap: 4 }}>
                  <div className="between" style={{ paddingBottom: 6 }}><b className="small">{g.title}</b><span className="muted tiny">{g.note}</span></div>
                  {g.keys.map((k) => s.readiness.components.find((c) => c.key === k)).filter(Boolean).map((c) => {
                    const pts = pointsOf(c!, s.readiness.components);
                    const w = pts == null ? 0 : Math.min(50, (Math.abs(pts) / 15) * 50);
                    return (
                      <div key={c!.key} className="factor">
                        <div style={{ minWidth: 0 }}><div className="small" style={{ fontWeight: 600 }}>{c!.label}</div><div className="muted tiny">{c!.note}</div></div>
                        <div className="fbar">
                          <div className="fmid" />
                          {pts != null && Math.abs(pts) >= 0.5 && <div style={{ position: 'absolute', top: 0, bottom: 0, borderRadius: 999, background: pts >= 0 ? '#3DDC97' : '#FF6B6B', left: pts >= 0 ? '50%' : `${50 - w}%`, width: `${w}%` }} />}
                        </div>
                        <span className="num small" style={{ textAlign: 'right', fontWeight: 700, color: pts == null ? 'var(--muted)' : Math.abs(pts) < 0.5 ? 'var(--muted)' : pts > 0 ? '#3DDC97' : '#FF6B6B' }}>{fmtPts(pts)}</span>
                      </div>
                    );
                  })}
                </div>
              ))}
              {s.readiness.penalties.length > 0 && (
                <div className="inset" style={{ gap: 4 }}>
                  <div className="between" style={{ paddingBottom: 6 }}><b className="small">Привычки</b><span className="muted tiny">вчера</span></div>
                  {s.readiness.penalties.map((p) => (
                    <div key={p.text} className="factor"><div className="small" style={{ fontWeight: 600 }}>{p.text}</div><span className="chip chip-warn" style={{ justifySelf: 'start' }}>снижает</span><span className="num small" style={{ textAlign: 'right', fontWeight: 700, color: '#FF6B6B' }}>−{p.points}</span></div>
                  ))}
                </div>
              )}
            </div>
            {(s.readiness.calibrating ?? []).map((c) => (
              <span key={c.group} className="small" style={{ color: '#F5B642', lineHeight: 1.5 }}>
                {c.name}: новый источник {c.provider} с {new Date(`${c.from}T12:00:00`).toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' })} — норма набирается, ещё ≈{c.daysLeft} дн. Пока нормы нет, показатель меньше влияет на оценку.
              </span>
            ))}
            <p className="muted small" style={{ margin: 0, lineHeight: 1.5 }}>
              Обычный день — 50 баллов. Цифра справа — сколько баллов показатель прибавил или отнял от этих 50 с учётом его веса. Внутри коридора нормы показатель двигает оценку чуть-чуть (HRV — до 5 баллов), за коридором — заметно сильнее. Алкоголь снимает от 3 до 10 баллов сверху — к тому, что уже видно по HRV и пульсу.
            </p>
          </>
        )}
      </section>

      <section className="card coach" aria-label="Рекомендация на день">
        <div className="stack" style={{ gap: 12, flex: '999 1 520px', minWidth: 0 }}>
          <span className="row" style={{ gap: 10 }}><span className="dot" style={{ width: 10, height: 10, background: lc }} /><span className="label">Тренировка дня · по расчётам Kouros</span></span>
          <h2 className="coach-title">{day.advice.title}</h2>
          <p style={{ margin: 0, fontSize: 16, lineHeight: 1.55, color: 'var(--text-2)' }}>{day.advice.text}</p>
          {(day.advice.reasons?.length ?? 0) > 0 && (
            <span className="small" style={{ color: '#F5B642', lineHeight: 1.5 }}>
              Совет мягче, чем позволяет тело: {day.advice.reasons!.join('; ')}.
            </span>
          )}
          {day.advice.hint && <span className="small" style={{ color: '#9DB8FF', lineHeight: 1.5 }}>{day.advice.hint}</span>}
          {day.advice.evening && <span className="muted tiny">С {s.readiness.eveningFrom ?? 'вечера'} совет и варианты считаются от вечерней готовности.</span>}
          {plan && plan.proposals.some((p) => p.date !== plan.today) && (
            <button type="button" className="plain small" style={{ textAlign: 'left', lineHeight: 1.5, cursor: 'pointer' }} onClick={go ? () => go({ tab: 'coach', sub: 'plan' }) : onCoach}>
              <span style={{ color: '#F5B642' }}>Kouros предлагает изменить план на другие дни ({plan.proposals.filter((p) => p.date !== plan.today).length}) →</span>
            </button>
          )}
          {coach ? (
            <button type="button" className="plain small" style={{ textAlign: 'left', lineHeight: 1.5, cursor: 'pointer', color: 'var(--text-2)' }} onClick={onCoach}>
              <span className="muted tiny">Тренер · {coach.kind === 'week' ? 'анализ недели' : 'отчёт дня'} {new Date(coach.ts * 1000).toLocaleString('ru-RU', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })}: </span>{coach.body.headline} <span style={{ color: '#3DDC97' }}>Тренер →</span>
            </button>
          ) : (
            <button type="button" className="linkbtn small" style={{ alignSelf: 'flex-start' }} onClick={onCoach}>Спросить тренера «Как мой день?» →</button>
          )}
        </div>
      </section>

      {plan && <DayPlanCard plan={plan} reload={load} onCoach={go ? () => go({ tab: 'coach', sub: 'plan' }) : onCoach} />}
      {shoe && <ShoeTodayCard a={shoe} reload={load} onGear={onGear} />}

      <div className="grid2">
        <section className="card" aria-label="Неделя">
          <div className="between"><span className="label">Неделя · {weekTitle}</span><button type="button" className="linkbtn small" onClick={onTraining}>Тренировки →</button></div>
          <div className="week">
            {day.week.map((d) => {
              const future = d.date > s.today;
              const isToday = d.date === s.today;
              return (
                <div key={d.date} className={`wday${isToday ? ' today' : ''}`}>
                  <span className="tiny" style={{ fontWeight: 700 }}>{WD[weekday(d.date)]} {d.date.slice(8)}</span>
                  <span className="wdot" style={{ background: d.load > 0 ? '#3DDC97' : 'transparent', borderColor: d.load > 0 ? '#3DDC97' : future ? '#9AA3AE' : '#3A404A' }} />
                  <span className="tiny" style={{ lineHeight: 1.3, color: 'var(--text-2)' }}>
                    {d.sports.length ? Array.from(new Set(d.sports.map((x) => SPORT_RU[x] ?? x))).join(', ') : future ? '' : 'отдых'}
                  </span>
                  {d.minutes > 0 && <span className="muted tiny">{d.minutes} мин · {n0(d.load)}</span>}
                  {d.readiness != null && (onHealthDay && !future
                    ? <button type="button" className="plain" title="Открыть день в истории здоровья" onClick={() => onHealthDay(d.date)}><ReadyDot v={d.readiness} /></button>
                    : <ReadyDot v={d.readiness} />)}
                </div>
              );
            })}
          </div>
          <div className="row tiny" style={{ gap: 16 }}>
            <span className="row" style={{ gap: 6 }}><span className="wdot" style={{ background: '#3DDC97', borderColor: '#3DDC97' }} />сделано</span>
            <span className="row" style={{ gap: 6 }}><span className="wdot" style={{ borderColor: '#9AA3AE' }} />впереди</span>
            <span className="row" style={{ gap: 6 }}><ReadyDot v={null} />готовность</span>
          </div>
        </section>
        <section className="card" aria-label="Напоминания">
          <div className="between"><span className="label">Напоминания</span><span className="muted tiny">{reminders.length ? plural(reminders.length) : ''}</span></div>
          {reminders.map((r) => (
            <div key={r.code} className="list-row" style={{ alignItems: 'center' }}>
              <span className="dot" style={{ background: r.level === 'warn' ? '#F5B642' : '#9DB8FF', flex: 'none' }} />
              <span className="small" style={{ flex: 1, lineHeight: 1.45 }}>{r.text}</span>
              {r.psych && onPsych && <button type="button" className="linkbtn small" onClick={onPsych}>Пройти →</button>}
              {!r.psych && go && SIGNAL_GO[r.code] && <button type="button" className="linkbtn small" onClick={() => go(SIGNAL_GO[r.code])}>Подробнее →</button>}
              <button type="button" aria-label="Скрыть на неделю" title="Скрыть на неделю" className="xbtn" onClick={() => hide(`${r.code}:${r.level}`)}>×</button>
            </div>
          ))}
          {reminders.length === 0 && <span className="muted small">Всё в порядке — напоминаний нет.</span>}
          {reminders.length > 0 && <span className="muted tiny">× — скрыть на неделю. Если за это время станет хуже, напоминание вернётся.</span>}
        </section>
      </div>

      <div className="grid2">
        <section className="card" aria-label="Нагрузка и тренировки">
          <div className="between"><span className="label">Нагрузка и тренировки</span><button type="button" className="linkbtn small" onClick={() => (go ? go({ tab: 'training', sub: 'all' }) : onTraining())}>Все тренировки →</button></div>
          <div className="row" style={{ gap: '8px 24px', alignItems: 'baseline' }}>
            <span style={{ fontFamily: 'var(--display)', fontSize: 22, fontWeight: 700, color: phaseColor }}>{phaseText}</span>
            <span className="small"><span className="muted">риск травмы </span><b style={{ color: riskColor }}>{riskText}</b>
              {mAcwr != null && riskText !== 'низкий' && <span className="muted"> · {riskFrom === 'muscle' ? 'по мышцам и связкам' : 'по сердцу'}</span>}</span>
            {rec && <span className="small"><span className="muted">восстановление </span><b style={{ color: rec.leftH > 0 ? '#F5B642' : '#3DDC97' }}>{rec.leftH > 0 ? `ещё ≈${rec.leftH} ч` : 'готов'}</b></span>}
          </div>
          <div className="phase">
            {['#4FA3FF', '#9AA3AE', '#3DDC97', '#FF6B6B'].map((c, i) => <div key={c} style={{ background: c, opacity: i === phaseIdx ? 1 : 0.25 }} />)}
          </div>
          <div className="phase muted tiny" style={{ textAlign: 'center' }}>
            {['Отдых', 'Поддержка', 'Рост', 'Перегрузка'].map((t, i) => <span key={t} style={i === phaseIdx ? { color: 'var(--text)', fontWeight: 700 } : undefined}>{t}</span>)}
          </div>
          <span className="muted tiny">
            Неделя {n0(s.load.week)} при обычных {n0(s.load.ctl * 7)} · соотношение {s.load.acwr != null ? n1(s.load.acwr) : '—'} (норма 0,8–1,3)
          </span>
          {mus && (
            <button type="button" className="plain muted tiny" style={{ textAlign: 'left', cursor: 'pointer' }} title="Открыть историю нагрузки" onClick={() => (go ? go({ tab: 'training', card: 'Нагрузка: сердце и мышцы' }) : onTraining())}>
              Мышцы и связки: {mus.acwr == null ? `неделя ${n0(mus.week)} · норма набирается` : mus.acwr > 1.3 ? `неделя на ${Math.round((mus.acwr - 1) * 100)}% тяжелее обычной` : mus.acwr < 0.8 ? 'легче обычного' : 'как обычно'}
              {mus.acwr != null ? ` (${n1(mus.acwr)})` : ''} →
            </button>
          )}
          <div>
            {day.recent.slice(0, 5).map((a) => {
              return (
                <div key={a.id} className="list-row">
                  <SportIcon sport={a.sport} size={34} />
                  <span className="stack" style={{ gap: 0, flex: 1, minWidth: 0 }}>
                    <span className="ellipsis small" style={{ fontWeight: 700 }}>{a.name || SPORT_RU[a.sport]}</span>
                    <span className="muted tiny">{new Date(a.startUtc * 1000).toLocaleString('ru-RU', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })} · {hm(a.durationS)}{a.distanceM ? ` · ${n1(a.distanceM / 1000)} км` : ''}</span>
                  </span>
                  <span className="num small" style={{ fontWeight: 700, color: '#4FA3FF' }}>{a.load}</span>
                </div>
              );
            })}
          </div>
          <span className="muted tiny">Справа — нагрузка тренировки (TRIMP по пульсу)</span>
        </section>
        <section className="card" aria-label="Нагрузка за 90 дней">
          <div className="between"><span className="label">Форма и усталость</span><span className="muted tiny">90 дней</span></div>
          <div className="tiles4">
            <div className="tile"><span className="num tile-v" style={{ color: '#9DB8FF' }}>{n0(s.load.ctl)}</span><span className="muted tiny">форма</span></div>
            <div className="tile"><span className="num tile-v" style={{ color: '#FF8A65' }}>{n0(s.load.atl)}</span><span className="muted tiny">усталость</span></div>
            <div className="tile"><span className="num tile-v">{s.load.tsb > 0 ? '+' : ''}{n0(s.load.tsb)}</span><span className="muted tiny">свежесть</span></div>
            <div className="tile"><span className="num tile-v">{n0(s.load.week)}</span><span className="muted tiny">7 дней</span></div>
          </div>
          <Spark series={s.load.series} />
          <span className="muted tiny"><span style={{ color: '#9DB8FF' }}>Форма</span> — средняя нагрузка за 6 недель, <span style={{ color: '#FF8A65' }}>усталость</span> — за неделю. Свежесть = форма − усталость.</span>
        </section>
      </div>

      <section className="card" aria-label="Самочувствие">
        <div className="between"><span className="label">Самочувствие · утренний опрос</span>{onPsych && <button type="button" className="linkbtn small" onClick={onPsych}>Психология →</button>}</div>
        {(() => {
          const hq = ps?.hooper.find((x) => x.date === s.today);
          if (!hq) return (
            <div className="row" style={{ gap: 12 }}>
              <span className="muted small" style={{ flex: '1 1 300px', lineHeight: 1.5 }}>30 секунд: сон, стресс, усталость и мышцы. Ощущения показываются рядом с готовностью по часам — если они расходятся, Kouros подсветит.</span>
              {onPsych && <button type="button" className="btn2" onClick={onPsych}>Пройти опрос</button>}
            </div>
          );
          const feel = (hq.extra as { feel?: number } | null)?.feel ?? null;
          const diff = feel != null && rScore != null ? feel - rScore : null;
          return (
            <span className="small" style={{ lineHeight: 1.5 }}>
              Самочувствие <b className="num">{feel ?? '—'}</b> из 100 · по часам <b className="num">{rScore ?? '—'}</b>
              <span className="muted">{diff == null ? '' : Math.abs(diff) < 20 ? ' — совпадают' : diff < 0 ? ' — ощущения заметно хуже, чем показывают часы: возможно, стресс вне спорта' : ' — ощущения лучше, чем показывают часы'}</span>
            </span>
          );
        })()}
      </section>

      <section className="card" aria-label="Все расчёты">
        <button type="button" className="collapse" aria-expanded={allOpen} onClick={() => setAllOpen(!allOpen)}>
          <span className="row" style={{ gap: 14, flexWrap: 'nowrap' }}>
            <span className="chev" style={{ transform: allOpen ? 'rotate(180deg)' : 'none' }}>⌄</span>
            <span className="stack" style={{ gap: 4 }}><span className="label">Все расчёты</span><span className="muted small">Подробные цифры: сон, нагрузка, интенсивность, рекорды, тело, недели</span></span>
          </span>
        </button>
      </section>
      {allOpen && <Details s={s} onSettings={onProfile ?? onSettings} />}
    </>
  );
}

const JOKES = [
  'Готовность считается во сне. Буквально',
  'Цифры уже легли. Твоя очередь',
  'Восстановление не скачивается — оно высыпается',
  'Утром будет цифра, а сейчас — подушка',
  'HRV растёт ночью. Без тебя у него не получится',
  'Лучшая тренировка на ближайшие часы — сон',
  'Даже часы ждут, когда ты ляжешь',
];
function nightJoke(day: string) {
  const n = Number(day.replace(/-/g, '')) % JOKES.length;
  return JOKES[n];
}

function NightBanner({ hs, onHealth }: { hs: HealthSummary; onHealth: () => void }) {
  const ns = hs.nightState!;
  const now = new Date(ns.now * 1000).toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' });
  const toBed = ns.minToBed;
  const words = (m: number) => { const a = Math.abs(m); const h = Math.floor(a / 60), mm = a % 60; return h ? `${h} ч ${mm} мин` : `${mm} мин`; };
  const [title, text] = {
    before_sleep: [`Сейчас ${now} — до отбоя ${words(toBed)}`, `Отбой по плану в ${hs.plan.bed}, подъём в ${hs.plan.wake}. Тяжёлую еду и интенсивные тренировки лучше закончить заранее.`],
    evening: [`Скоро отбой — в ${hs.plan.bed}`, 'Пора заканчивать дела: тяжёлая еда, интенсивная нагрузка и яркие экраны сейчас мешают уснуть.'],
    lights: [`Время приглушить свет — отбой в ${hs.plan.bed}`, 'Экраны в сторону, тёплый свет, спокойные дела. Так быстрее уснёшь и глубже поспишь.'],
    sleep: [`Время сна — отбой был в ${hs.plan.bed}`, `Сейчас ${now}. Каждый час не во сне добавляет к долгу сна${ns.debtMin > 0 ? ` (сейчас ${words(ns.debtMin)} за неделю)` : ''}.`],
    day: ['', ''],
  }[ns.phase];
  return (
    <button type="button" className="banner night" onClick={onHealth}>
      <span className="banner-ic">🌙</span>
      <span style={{ flex: '1 1 360px', display: 'flex', flexDirection: 'column', gap: 2, textAlign: 'left' }}>
        <b>{title}</b>
        <span className="small" style={{ color: 'var(--text-2)' }}>{text}{hs.suggestion ? ` Сегодня лучше поспать на ${hs.suggestion.min} мин больше — подробности в плане сна.` : ''}</span>
      </span>
      <span style={{ fontWeight: 700, color: '#A78BFA' }}>План сна →</span>
    </button>
  );
}

const readyColor = (v: number) => (v >= 67 ? '#3DDC97' : v >= 40 ? '#F5B642' : '#FF6B6B');

/** Readiness in a small coloured circle (fits narrow week cards). */
function ReadyDot({ v }: { v: number | null }) {
  const c = v == null ? '#9AA3AE' : readyColor(v);
  return (
    <span className="rdot num" style={{ borderColor: c, color: c }} title={v == null ? 'готовность' : `готовность ${v}`}>
      {v ?? ''}
    </span>
  );
}

function plural(n: number) {
  const m10 = n % 10, m100 = n % 100;
  return `${n} ${m10 === 1 && m100 !== 11 ? 'напоминание' : m10 >= 2 && m10 <= 4 && (m100 < 10 || m100 >= 20) ? 'напоминания' : 'напоминаний'}`;
}
