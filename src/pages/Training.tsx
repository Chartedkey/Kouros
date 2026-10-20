import { useCallback, useEffect, useState } from 'react';
import {
  api, errText, fmtDur, fmtKm, fmtPace, fmtWhen, plural, sportOf,
  type ActivityDetail, type ActivityPage, type ActivityStats, type EngineSummary, type ListFilter, type Shoe, type Split, type TrainingOverview,
} from '../lib/api';
import { useDataChanged } from '../lib/events';
import { DeletedList, DupCards } from '../components/Workouts';
import { IoTab } from './TrainingIo';
import { HrBars, Workout } from './Workout';
import { MuscleMap } from '../components/MuscleMap';
import { History, IntensityWeeks, LoadDual, RecordsFull, RecoveryTile, ShoeQuestion, Tolerance, ZonesCard } from './TrainingParts';

export type TrainingSub = 'overview' | 'all' | 'records' | 'body' | 'gear' | 'io';

const SUBS: [TrainingSub, string][] = [
  ['overview', 'Обзор'], ['all', 'Все тренировки'], ['records', 'Рекорды и цели'], ['body', 'Тело и зоны'], ['gear', 'Инвентарь'],
];

export const ZONE_NAMES = ['Z1 · восстановление', 'Z2 · аэробная база', 'Z3 · темп', 'Z4 · порог', 'Z5 · максимум'];
export const ZONE_COLORS = ['#7FD3FF', '#3DDC97', '#F5B642', '#FF8A65', '#FF6B6B'];

const n0 = (x: number | null | undefined) => (x == null ? '—' : Math.round(x).toLocaleString('ru-RU'));
const n1 = (x: number | null | undefined) => (x == null ? '—' : x.toFixed(1).replace('.', ','));
const dayLabel = (d: string) => new Date(d + 'T12:00:00').toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' });
function clock(s: number) {
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), sec = Math.round(s % 60);
  return h ? `${h}:${String(m).padStart(2, '0')}:${String(sec).padStart(2, '0')}` : `${m}:${String(sec).padStart(2, '0')}`;
}

function useSummary() {
  const [s, setS] = useState<EngineSummary | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(() => { api.engineSummary().then((v) => { setS(v); setErr(null); }).catch((e) => setErr(errText(e))); }, []);
  useEffect(load, [load]);
  useDataChanged(load);
  return { s, err, reload: load };
}

function useOverview() {
  const [o, setO] = useState<TrainingOverview | null>(null);
  const load = useCallback(() => { api.trainingOverview().then(setO).catch(() => {}); }, []);
  useEffect(load, [load]);
  useDataChanged(load);
  return { o, reloadO: load };
}

export function Training({ sub, onSub, open, onOpenDone }: { sub: TrainingSub; onSub: (s: TrainingSub) => void; open?: string | null; onOpenDone?: () => void }) {
  const [openId, setOpenId] = useState<string | null>(open ?? null);
  useEffect(() => { if (open) { setOpenId(open); onOpenDone?.(); } }, [open, onOpenDone]);
  const { s, err, reload } = useSummary();
  const { o, reloadO } = useOverview();

  if (openId) return <Workout id={openId} onBack={() => setOpenId(null)} />;

  return (
    <>
      <div className="page-head">
        <div className="stack">
          <h1>Тренировки</h1>
          <p className="subtitle">Все активности из Strava — бег, ходьба, силовые и физиотерапия</p>
        </div>
        <button type="button" className="btn2" aria-pressed={sub === 'io'} onClick={() => onSub(sub === 'io' ? 'overview' : 'io')}>⇅ Импорт / экспорт</button>
      </div>
      <nav className="subtabs" aria-label="Разделы тренировок">
        {SUBS.map(([id, name]) => (
          <button key={id} type="button" aria-pressed={sub === id} onClick={() => onSub(id)}>{name}</button>
        ))}
      </nav>
      {err && sub !== 'io' && sub !== 'all' && sub !== 'gear' && <div className="note-err">{err}</div>}
      {sub === 'overview' && <Overview s={s} o={o} reloadO={reloadO} onOpen={setOpenId} onAll={() => onSub('all')} onImport={() => onSub('io')} />}
      {sub === 'all' && <AllTab onOpen={setOpenId} />}
      {sub === 'records' && (s ? <RecordsTab s={s} onOpen={setOpenId} /> : <div className="muted">Считаю…</div>)}
      {sub === 'body' && (s ? <BodyTab s={s} onSaved={() => { reload(); reloadO(); }} onOpen={setOpenId} /> : <div className="muted">Считаю…</div>)}
      {sub === 'gear' && <GearTab />}
      {sub === 'io' && <IoTab onDone={() => onSub('overview')} />}
    </>
  );
}

// ---------- Рекомендации на сегодня ----------

function readFold(): boolean {
  try { return localStorage.getItem('kouros.recFolded') === '1'; } catch { return false; }
}
function saveFold(v: boolean) {
  try { localStorage.setItem('kouros.recFolded', v ? '1' : '0'); } catch { /* not important */ }
}

const LEVEL_COLOR: Record<string, string> = { green: '#3DDC97', yellow: '#F5B642', red: '#FF6B6B', unknown: '#9AA3AE' };
const OPT_ICON: Record<string, string> = { run: 'Б', walk: 'Х', strength: 'С', mobility: 'М', done: '✓' };

function RecPlaque({ s }: { s: EngineSummary }) {
  const [folded, setFolded] = useState(readFold);
  const d = s.day;
  const t = d.target ?? { lo: d.targetLoad * 0.67, hi: d.targetLoad, headroom: null, capped: false, cap: 1.3, left: 0 };
  const c = LEVEL_COLOR[s.readiness.level] ?? LEVEL_COLOR.unknown;
  const opts = d.options ?? [];
  const toggle = () => { setFolded(!folded); saveFold(!folded); };
  const short = `${n0(t.lo)}–${n0(t.hi)} · сделано ${n0(d.todayLoad)}${opts[0] ? ` · ${opts[0].title}` : ''}`;
  return (
    <section className="card rec" aria-label="Рекомендации на сегодня" style={{ borderColor: '#2a3038' }}>
      <button type="button" className="collapse" aria-expanded={!folded} onClick={toggle}>
        <span className="row" style={{ gap: 14, flexWrap: 'nowrap', minWidth: 0, flex: '1 1 420px' }}>
          <span className="chev" style={{ transform: folded ? 'none' : 'rotate(180deg)' }}>⌄</span>
          <span className="stack" style={{ gap: 4, minWidth: 0 }}>
            <span className="row" style={{ gap: 10 }}><span className="dot" style={{ width: 10, height: 10, background: c }} /><span className="label">Рекомендации на сегодня</span></span>
            {folded && <span className="small ellipsis" style={{ color: 'var(--text-2)' }}>{short}</span>}
          </span>
        </span>
        {!folded && (
          <span className="row" style={{ gap: 18 }}>
            <span className="stack" style={{ gap: 0, alignItems: 'flex-end' }}><span className="muted tiny">цель дня</span><span className="num" style={{ fontSize: 22, fontWeight: 700, color: '#4FA3FF' }}>{n0(t.lo)}–{n0(t.hi)}</span></span>
            <span className="stack" style={{ gap: 0, alignItems: 'flex-end' }}><span className="muted tiny">сделано</span><span className="num" style={{ fontSize: 22, fontWeight: 700 }}>{n0(d.todayLoad)}</span></span>
          </span>
        )}
      </button>
      {!folded && (
        <>
          <div className="opts">
            {opts.map((o) => (
              <div key={o.kind + o.title} className="inset" style={{ flexDirection: 'row', alignItems: 'center', gap: 12 }}>
                <span className="badge" style={{ color: o.kind === 'done' ? '#3DDC97' : '#4FA3FF' }}>{OPT_ICON[o.kind] ?? '•'}</span>
                <span className="stack" style={{ gap: 2, minWidth: 0 }}><b className="small">{o.title}</b><span className="muted tiny">{o.text}</span></span>
              </div>
            ))}
            {opts.length === 0 && <span className="muted small">Пока мало своих тренировок, чтобы перевести цель в минуты.</span>}
          </div>
          <span className="small" style={{ lineHeight: 1.55, color: 'var(--text-2)' }}>
            <b>Почему: </b>{d.advice.text}
            {t.capped ? ` Верх ограничен запасом недели: за 7 дней нагрузка не должна превысить обычную неделю × ${String(t.cap).replace('.', ',')}.` : ''}
          </span>
          <span className="muted tiny">Варианты — на остаток до середины цели, по твоей средней нагрузке в минуту за 90 дней. Можно сочетать: например, силовая + прогулка.</span>
        </>
      )}
    </section>
  );
}

// ---------- Обзор ----------

function loadPhase(acwr: number | null): [number, string, string] {
  if (acwr == null) return [-1, 'Мало истории', '#6B737E'];
  if (acwr < 0.8) return [0, 'Восстановление', '#4FA3FF'];
  if (acwr < 1.0) return [1, 'Поддержка формы', '#9AA3AE'];
  if (acwr <= 1.3) return [2, 'Форма растёт', '#3DDC97'];
  return [3, 'Перегрузка', '#FF6B6B'];
}

function Spark({ series }: { series: [string, number, number, number, number][] }) {
  if (series.length < 2) return null;
  const W = 520, H = 110, pad = 4;
  const ys = series.flatMap((d) => [d[1], d[2], d[3]]);
  const max = Math.max(...ys, 1);
  const x = (i: number) => pad + (i / (series.length - 1)) * (W - 2 * pad);
  const y = (v: number) => H - pad - (v / max) * (H - 2 * pad);
  const line = (k: 2 | 3) => series.map((d, i) => `${x(i).toFixed(1)},${y(d[k]).toFixed(1)}`).join(' ');
  const bw = (W - 2 * pad) / series.length;
  return (
    <svg viewBox={`0 0 ${W} ${H}`} width="100%" height={H} preserveAspectRatio="none" aria-hidden="true">
      {series.map((d, i) => d[1] > 0 && <rect key={i} x={x(i) - bw / 2} y={y(d[1])} width={Math.max(1, bw - 1)} height={H - pad - y(d[1])} fill="#2a3a4a" />)}
      <polyline points={line(3)} fill="none" stroke="#9DB8FF" strokeWidth="2" />
      <polyline points={line(2)} fill="none" stroke="#FF8A65" strokeWidth="2" />
    </svg>
  );
}

export function SplitBar({ s }: { s: Split | null }) {
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
        <span><b style={{ color: '#F5B642' }}>{Math.round(s.mid)}%</b> темп (Z3)</span>
        <span><b style={{ color: '#FF6B6B' }}>{Math.round(s.high)}%</b> тяжёлая (Z4–Z5)</span>
        <span className="muted">{n1(s.hours)} ч с пульсом</span>
      </div>
    </div>
  );
}

function Overview({ s, o, reloadO, onOpen, onAll, onImport }: { s: EngineSummary | null; o: TrainingOverview | null; reloadO: () => void; onOpen: (id: string) => void; onAll: () => void; onImport: () => void }) {
  const [stats, setStats] = useState<ActivityStats | null>(null);
  const [last, setLast] = useState<ActivityDetail | null>(null);
  const [recent, setRecent] = useState<ActivityPage | null>(null);
  const load = useCallback(() => {
    api.activityStats().then(setStats).catch(() => {});
    api.activities({}, 8, 0).then((p) => {
      setRecent(p);
      const first = p.items.find((a) => a.hasStreams) ?? p.items[0];
      if (first) api.activityGet(first.id).then(setLast).catch(() => setLast(null));
    }).catch(() => {});
  }, []);
  useEffect(load, [load]);
  useDataChanged(load);
  // «Сегодня» → «Мышцы и связки» opens this tab scrolled to the load card.
  useEffect(() => {
    let want = false;
    try { want = sessionStorage.getItem('kouros_scroll') === 'load-dual'; } catch { /* ignore */ }
    if (!want || !s) return;
    try { sessionStorage.removeItem('kouros_scroll'); } catch { /* ignore */ }
    setTimeout(() => document.getElementById('load-dual')?.scrollIntoView({ behavior: 'smooth', block: 'start' }), 150);
  }, [s]);

  if (stats && stats.count === 0) {
    return (
      <section className="card" style={{ alignItems: 'flex-start' }}>
        <span className="label">Пока нет тренировок</span>
        <span style={{ fontSize: 15, lineHeight: 1.55, color: 'var(--text-2)', maxWidth: 640 }}>
          Начни с архива Strava: он загрузится за пару минут и сразу даст всю историю с пульсом и темпом.
          Потом подключи Strava в Настройках — Kouros будет забирать только новые тренировки.
        </span>
        <button type="button" className="btn btn-green" onClick={onImport}>Импортировать архив Strava</button>
      </section>
    );
  }

  const [phaseIdx, phaseText, phaseColor] = loadPhase(s?.load.acwr ?? null);
  const la = last?.activity;
  const lsp = la ? sportOf(la.sport) : null;
  const moving = la ? la.movingS ?? la.elapsedS : null;
  return (
    <>
      {s && <RecPlaque s={s} />}
      {o?.shoeQuestion && <ShoeQuestion q={o.shoeQuestion} onDone={reloadO} />}

      <div className="grid2">
        <section className="card" aria-label="Последняя тренировка">
          <div className="between"><span className="label">Последняя тренировка{la ? ` · ${fmtWhen(la.startUtc)}` : ''}</span>{la && <button type="button" className="linkbtn small" onClick={() => onOpen(la.id)}>Разбор →</button>}</div>
          {la && lsp ? (
            <>
              <button type="button" className="plain" onClick={() => onOpen(la.id)}>
                <span className="row" style={{ gap: 12, flexWrap: 'nowrap' }}>
                  <span className="badge" style={{ color: lsp.color }}>{lsp.letter}</span>
                  <span className="stack" style={{ gap: 0, minWidth: 0 }}>
                    <span style={{ fontFamily: 'var(--display)', fontSize: 20, fontWeight: 700 }} className="ellipsis">{la.name || lsp.name}</span>
                    <span className="muted small">{lsp.name}{last?.load != null ? ` · нагрузка ${last.load}` : ''}{la.relativeEffort ? ` · Relative Effort ${Math.round(la.relativeEffort)}` : ''}</span>
                  </span>
                </span>
              </button>
              <div className="tiles4">
                <div className="tile"><span className="num tile-v2">{fmtKm(la.distanceM, 2)}</span><span className="muted tiny">дистанция</span></div>
                <div className="tile"><span className="num tile-v2">{fmtDur(moving)}</span><span className="muted tiny">в движении</span></div>
                <div className="tile"><span className="num tile-v2">{la.distanceM && moving ? fmtPace(moving / (la.distanceM / 1000)) : '—'}</span><span className="muted tiny">темп, /км</span></div>
                <div className="tile"><span className="num tile-v2">{la.avgHr ? Math.round(la.avgHr) : '—'} / {la.maxHr ? Math.round(la.maxHr) : '—'}</span><span className="muted tiny">пульс ср / макс</span></div>
              </div>
              {last && last.splits.length > 1 && s && <HrBars splits={last.splits} zones={s.profile.zones} />}
              {last && last.sets.length > 0 && <span className="muted small">{plural(last.sets.filter((x) => x.kind === 'active').length, 'подход', 'подхода', 'подходов')} — открой разбор, чтобы вписать упражнения и веса.</span>}
            </>
          ) : <span className="muted">Загружаю…</span>}
        </section>

        <section className="card" aria-label="Нагрузка и восстановление">
          <span className="label">Нагрузка и восстановление</span>
          {s ? (
            <>
              <span style={{ fontFamily: 'var(--display)', fontSize: 22, fontWeight: 700, color: phaseColor }}>{phaseText}</span>
              <div className="phase">
                {['#4FA3FF', '#9AA3AE', '#3DDC97', '#FF6B6B'].map((c, i) => <div key={c} style={{ background: c, opacity: i === phaseIdx ? 1 : 0.25 }} />)}
              </div>
              <div className="tiles4">
                <RecoveryTile s={s} />
                <div className="tile"><span className="num tile-v2">{n0(s.load.week)}</span><span className="muted tiny">за 7 дней · обычно {n0(s.load.ctl * 7)}</span></div>
                <div className="tile"><span className="num tile-v2">{s.load.acwr != null ? n1(s.load.acwr) : '—'}</span><span className="muted tiny">соотношение (норма 0,8–1,3)</span></div>
                <div className="tile"><span className="num tile-v2">{s.load.tsb > 0 ? '+' : ''}{n0(s.load.tsb)}</span><span className="muted tiny">свежесть</span></div>
              </div>
              <Spark series={s.load.series} />
              <span className="muted tiny">Восстановление — сколько часов осталось после последних тренировок (по сердцу и мышцам, берётся более долгое), с поправкой на сегодняшнюю готовность. Столбцы — нагрузка дня, <span style={{ color: '#FF8A65' }}>усталость</span> — за неделю, <span style={{ color: '#9DB8FF' }}>форма</span> — за 6 недель. 90 дней.</span>
            </>
          ) : <span className="muted">Считаю…</span>}
        </section>
      </div>

      <div className="grid2">
        <section className="card" aria-label="Недавние тренировки">
          <div className="between"><span className="label">Недавние</span><button type="button" className="linkbtn small" onClick={onAll}>Все тренировки →</button></div>
          <div>{recent?.items.map((a) => <ActRow key={a.id} a={a} onOpen={onOpen} compact />)}</div>
        </section>
        {s && <LoadDual s={s} onOpen={onOpen} />}
        {o ? <IntensityWeeks o={o} /> : <section className="card"><span className="muted">Считаю…</span></section>}
      </div>

      {o && (
        <div className="grid2">
          <History o={o} />
          <Tolerance t={o.tolerance} />
        </div>
      )}

      {stats && (
        <div className="tiles">
          <div className="tile"><span className="num tile-v">{stats.count.toLocaleString('ru-RU')}</span><span className="muted small">тренировок всего</span></div>
          <div className="tile"><span className="num tile-v">{fmtKm(stats.bySport.find((x) => x.sport === 'run')?.distanceM ?? 0, 0).replace(' км', '').replace('—', '0')}</span><span className="muted small">км бега</span></div>
          <div className="tile"><span className="num tile-v">{Math.round(stats.bySport.reduce((a, x) => a + x.seconds, 0) / 3600).toLocaleString('ru-RU')}</span><span className="muted small">часов</span></div>
          <div className="tile"><span className="tile-v2">{stats.firstUtc ? new Date(stats.firstUtc * 1000).toLocaleDateString('ru-RU', { month: 'short', year: 'numeric' }) : '—'} — {stats.lastUtc ? new Date(stats.lastUtc * 1000).toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' }) : '—'}</span><span className="muted small">период · с графиками {stats.withStreams}</span></div>
        </div>
      )}
    </>
  );
}

const METHOD: Record<string, string> = { hr: 'по пульсу', avg_hr: 'по среднему пульсу', rpe: 'по ощущениям', estimate: 'оценка по времени' };

function ActRow({ a, onOpen, compact, sel }: { a: ActivityPage['items'][number]; onOpen: (id: string) => void; compact?: boolean; sel?: boolean | null }) {
  const sp = sportOf(a.sport);
  const t = a.movingS ?? a.elapsedS;
  return (
    <button type="button" className={compact ? 'list-row plain' : 'tr tr-act plain'} onClick={() => onOpen(a.id)} style={sel ? { background: '#1a2230' } : undefined}>
      {!compact && <span className="muted small">{sel != null && <input type="checkbox" readOnly checked={sel} style={{ marginRight: 8 }} />}{fmtWhen(a.startUtc)}</span>}
      <span className="row" style={{ flexWrap: 'nowrap', gap: 10, minWidth: 0, flex: compact ? 1 : undefined }}>
        <span className="badge" style={{ color: sp.color }}>{sp.letter}</span>
        <span className="stack" style={{ gap: 0, minWidth: 0 }}>
          <span className="ellipsis" style={{ fontWeight: 700 }}>{a.name || sp.name}</span>
          <span className="muted tiny ellipsis">
            {compact ? `${fmtWhen(a.startUtc)} · ` : ''}{sp.name}
            {a.totalSets ? ` · ${plural(a.totalSets, 'подход', 'подхода', 'подходов')}` : ''}
            {a.rpe ? ` · тяжесть ${a.rpe}` : ''}
          </span>
        </span>
      </span>
      {!compact && <span className="r num small">{fmtKm(a.distanceM, 2)}</span>}
      {!compact && <span className="r num small">{fmtDur(t)}</span>}
      {!compact && <span className="r num small">{a.avgHr ? Math.round(a.avgHr) : '—'}</span>}
      {compact && <span className="muted tiny num">{a.distanceM ? fmtKm(a.distanceM) : fmtDur(t)}</span>}
      <span className="r num small" style={{ fontWeight: 700, color: '#4FA3FF' }}>{a.load != null ? Math.round(a.load) : '—'}</span>
    </button>
  );
}

// ---------- Все тренировки ----------

const SPORT_F: [string | null, string][] = [
  [null, 'Все'], ['run', 'Бег'], ['walk', 'Ходьба'], ['strength', 'Силовые'], ['workout', 'Физио'], ['bike', 'Вело'], ['other', 'Другое'],
];
const PERIODS: [number | null, string][] = [[7, '7 дней'], [30, '30 дней'], [90, '3 месяца'], [365, 'Год'], [null, 'Всё время']];
const SORTS: [NonNullable<ListFilter['sort']>, string][] = [['date', 'Дата'], ['distance', 'Дистанция'], ['duration', 'Время'], ['load', 'Нагрузка']];

function AllTab({ onOpen }: { onOpen: (id: string) => void }) {
  const [f, setF] = useState<ListFilter>({ sport: null, days: null, query: '', sort: 'date' });
  const [q, setQ] = useState('');
  const [limit, setLimit] = useState(50);
  const [page, setPage] = useState<ActivityPage | null>(null);
  const [err, setErr] = useState<string | null>(null);

  // Search waits a moment after typing.
  useEffect(() => { const t = setTimeout(() => setF((o) => ({ ...o, query: q })), 300); return () => clearTimeout(t); }, [q]);
  const load = useCallback(() => { api.activities(f, limit, 0).then((p) => { setPage(p); setErr(null); }).catch((e) => setErr(errText(e))); }, [f, limit]);
  useEffect(load, [load]);
  useDataChanged(load);
  const set = (o: Partial<ListFilter>) => { setF({ ...f, ...o }); setLimit(50); };
  const [pick, setPick] = useState<Set<string> | null>(null);
  const [ask, setAsk] = useState(false);
  const toggle = (id: string) => { if (!pick) return; const n = new Set(pick); if (n.has(id)) n.delete(id); else n.add(id); setPick(n); };
  const del = async () => {
    if (!pick || !pick.size) return;
    try { await api.workoutsDelete([...pick]); setPick(null); setAsk(false); load(); } catch (e) { setErr(errText(e)); }
  };

  return (
    <>
    <DupCards onOpen={onOpen} />
    <section className="card" aria-label="Все тренировки">
      <div className="filters">
        <div role="group" aria-label="Вид спорта" className="row">
          {SPORT_F.map(([id, name]) => <button key={name} type="button" className="pill" aria-pressed={f.sport === id} onClick={() => set({ sport: id })}>{name}</button>)}
        </div>
        <input className="input" style={{ maxWidth: 320 }} type="search" placeholder="Поиск по названию" aria-label="Поиск по названию" value={q} onChange={(e) => setQ(e.target.value)} />
      </div>
      <div className="filters">
        <div role="group" aria-label="Период" className="row">
          {PERIODS.map(([d, name]) => <button key={name} type="button" className="pill" aria-pressed={f.days === d} onClick={() => set({ days: d })}>{name}</button>)}
        </div>
        <label className="row small" style={{ gap: 8 }}>
          <span className="muted">Сортировка</span>
          <select className="select" value={f.sort} onChange={(e) => set({ sort: e.target.value as ListFilter['sort'] })}>
            {SORTS.map(([id, name]) => <option key={id} value={id}>{name}</option>)}
          </select>
        </label>
      </div>
      {err && <div className="note-err">{err}</div>}
      <div className="between" style={{ flexWrap: 'wrap', gap: 10 }}>
        {page ? <span className="muted small">{plural(page.total, 'тренировка', 'тренировки', 'тренировок')}</span> : <span />}
        {pick ? (
          <span className="row" style={{ gap: 10, flexWrap: 'wrap' }}>
            <span className="small">Выбрано: {pick.size}</span>
            {ask ? (
              <>
                <span className="small" style={{ color: '#FF6B6B' }}>Удалить? Нагрузка, восстановление, рекорды и километраж обуви пересчитаются.</span>
                <button type="button" className="btn2 btn-danger" onClick={del}>Удалить</button>
                <button type="button" className="linkbtn small" onClick={() => setAsk(false)}>Нет</button>
              </>
            ) : (
              <button type="button" className="btn2" disabled={!pick.size} onClick={() => setAsk(true)}>Удалить выбранные</button>
            )}
            <button type="button" className="linkbtn small" onClick={() => { setPick(null); setAsk(false); }}>Отмена</button>
          </span>
        ) : (
          <button type="button" className="linkbtn small" onClick={() => setPick(new Set())}>Выбрать и удалить</button>
        )}
      </div>
      <div className="table" role="table">
        <div className="tr tr-act th" role="row">
          <span>Дата</span><span>Тренировка</span><span className="r">Дистанция</span><span className="r">Время</span><span className="r">Пульс</span><span className="r">Нагрузка</span>
        </div>
        {page?.items.map((a) => <ActRow key={a.id} a={a} onOpen={pick ? toggle : onOpen} sel={pick ? pick.has(a.id) : null} />)}
        {page && page.items.length === 0 && <div className="list-row muted small">Ничего не нашлось — поменяй фильтры</div>}
      </div>
      {page && page.items.length < page.total && (
        <button type="button" className="btn2" style={{ alignSelf: 'center' }} onClick={() => setLimit((l) => l + 100)}>Показать ещё</button>
      )}
      <span className="muted tiny">Нагрузка — {Object.values(METHOD).join(', ')}: в разборе тренировки видно, как она посчитана.</span>
    </section>
    <DeletedList />
    </>
  );
}

// ---------- Рекорды и цели ----------

const DIST: Record<number, string> = { 1000: '1 км', 5000: '5 км', 10000: '10 км', 21097: 'Полумарафон', 21097.5: 'Полумарафон', 42195: 'Марафон' };

function RecordsTab({ s, onOpen }: { s: EngineSummary; onOpen: (id: string) => void }) {
  return (
    <div className="grid">
      <RecordsFull s={s} onOpen={onOpen} />
      <section className="card" aria-label="Прогноз">
        <span className="label">Прогноз результатов</span>
        {s.predictions.length > 0 ? (
          <>
            <div>
              {s.predictions.map((p) => (
                <div key={p.distanceM} className="list-row small">
                  <span style={{ width: 110, fontWeight: 600 }}>{DIST[p.distanceM] ?? p.distanceM}</span>
                  <span className="muted tiny" style={{ flex: 1 }}>{fmtPace(p.timeS / (p.distanceM / 1000))}/км</span>
                  <b className="num">≈ {clock(p.timeS)}</b>
                </div>
              ))}
            </div>
            <span className="muted tiny" style={{ lineHeight: 1.5 }}>
              Формула Ригеля по лучшему отрезку {DIST[s.predictions[0].fromDistanceM] ?? ''} за {clock(s.predictions[0].fromTimeS)} ({dayLabel(s.predictions[0].fromDate)}) за 120 дней.
              Чем длиннее дистанция, тем сильнее результат зависит от объёма — марафон без подготовки будет медленнее прогноза.
            </span>
          </>
        ) : <span className="muted small">Нужна пробежка от 5 км с графиком дистанции за последние 120 дней.</span>}
      </section>
      <section className="card" aria-label="Цели">
        <div className="between"><span className="label">Старты и цели</span><span className="chip chip-off">этап 6</span></div>
        <span className="muted small" style={{ lineHeight: 1.5 }}>
          Здесь появятся твои старты с обратным отсчётом и нужным темпом, а тренер построит под них план. Рекорды в силовых —
          после того как в разборе силовой будут вписаны упражнения и веса.
        </span>
      </section>
    </div>
  );
}

// ---------- Тело и зоны ----------

function BodyTab({ s, onSaved, onOpen }: { s: EngineSummary; onSaved: () => void; onOpen?: (id: string) => void }) {
  const z = s.profile.zones;
  const mins = s.intensity.zones28Min;
  const total = mins.reduce((a, b) => a + b, 0) || 1;
  const age = s.profile.zonesDate ? Math.round((Date.parse(s.today) - Date.parse(s.profile.zonesDate)) / 86400000) : null;
  const w = s.body.weight;
  return (
    <div className="grid">
      {s.muscleMap && <MuscleMap groups={s.muscleMap} onOpen={onOpen} />}
      <ZonesCard today={s.today} onSaved={onSaved} />
      <section className="card" aria-label="Время в зонах">
        <span className="label">Время в зонах · 28 дней</span>
        <div className="stack" style={{ gap: 10 }}>
          {['Ниже Z1', ...ZONE_NAMES].map((n, i) => (
            <div key={n} className="zrow">
              <span className="small">{n}</span>
              <div style={{ height: 10, borderRadius: 999, background: '#22262D', overflow: 'hidden' }}>
                <div style={{ width: `${(mins[i] / total) * 100}%`, height: '100%', background: i === 0 ? '#6B737E' : ZONE_COLORS[i - 1] }} />
              </div>
              <span className="num tiny" style={{ textAlign: 'right' }}>{fmtMin(mins[i] ?? 0)}</span>
            </div>
          ))}
        </div>
        <SplitBar s={s.intensity.d28} />
      </section>
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
        </span>
        <span className="muted tiny">Карта мышц появится после того, как в силовых будут вписаны упражнения.</span>
      </section>
    </div>
  );
}

function fmtMin(m: number) {
  const h = Math.floor(m / 60);
  return h ? `${h} ч ${Math.round(m % 60)} мин` : `${Math.round(m)} мин`;
}

// ---------- Инвентарь ----------

const SHOE_KIND: [Shoe['kind'], string, number][] = [['train', 'Тренировочные', 700], ['race', 'Соревновательные', 400], ['trail', 'Трейловые', 600], ['walk', 'Для ходьбы', 900]];

function GearTab() {
  const [list, setList] = useState<Shoe[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [form, setForm] = useState<{ name: string; kind: Shoe['kind']; startDate: string; initialKm: string; walks: boolean } | null>(null);
  const load = useCallback(() => { api.shoesGet().then(setList).catch((e) => setErr(errText(e))); }, []);
  useEffect(load, [load]);
  useDataChanged(load);

  async function save(next: Shoe[]) {
    try { await api.shoesSet(next.map(({ km: _k, workouts: _w, ...x }) => x)); setErr(null); load(); } catch (e) { setErr(errText(e)); }
  }
  function add() {
    if (!form || !list || !form.name.trim()) return;
    const limit = SHOE_KIND.find((k) => k[0] === form.kind)?.[2] ?? 700;
    const km = Number(form.initialKm.replace(',', '.')) || 0;
    save([...list, { id: String(Date.now()), name: form.name.trim(), kind: form.kind, startDate: form.startDate, initialKm: km, limitKm: limit, walks: form.walks, retired: false }]);
    setForm(null);
  }
  const today = new Date().toISOString().slice(0, 10);

  return (
    <div className="grid">
      <section className="card" aria-label="Кроссовки">
        <div className="between"><span className="label">Кроссовки</span><span className="muted tiny">{list ? plural(list.filter((x) => !x.retired).length, 'пара', 'пары', 'пар') : ''}</span></div>
        {err && <div className="note-err">{err}</div>}
        {list?.map((sh) => {
          const frac = Math.min(1, (sh.km ?? 0) / sh.limitKm);
          const col = frac > 0.9 ? '#FF6B6B' : frac > 0.7 ? '#F5B642' : '#3DDC97';
          return (
            <div key={sh.id} className="inset" style={{ opacity: sh.retired ? 0.55 : 1 }}>
              <div className="between">
                <span className="stack" style={{ gap: 2 }}><b>{sh.name}</b><span className="muted tiny">{SHOE_KIND.find((k) => k[0] === sh.kind)?.[1]} · с {dayLabel(sh.startDate)}{sh.walks ? ' · бег и ходьба' : ' · только бег'} · {plural(sh.workouts ?? 0, 'тренировка', 'тренировки', 'тренировок')}</span></span>
                <span className="row" style={{ gap: 6 }}>
                  {!sh.retired && (sh.main
                    ? <span className="chip chip-ok">основные</span>
                    : <button type="button" className="btn2" onClick={() => save(list.map((x) => ({ ...x, main: x.id === sh.id })))}>Сделать основными</button>)}
                  <button type="button" className="btn2" onClick={() => save(list.map((x) => (x.id === sh.id ? { ...x, retired: !x.retired } : x)))}>{sh.retired ? 'Вернуть' : 'В архив'}</button>
                  <button type="button" className="xbtn" aria-label="Удалить" onClick={() => save(list.filter((x) => x.id !== sh.id))}>×</button>
                </span>
              </div>
              <div style={{ height: 8, borderRadius: 999, background: '#22262D', overflow: 'hidden' }}><div style={{ width: `${frac * 100}%`, height: '100%', background: col }} /></div>
              <span className="small"><b className="num">{n1(sh.km ?? 0)}</b> <span className="muted">из ≈ {sh.limitKm} км{frac > 0.9 ? ' · пора менять' : frac > 0.7 ? ' · скоро замена' : ''}</span></span>
            </div>
          );
        })}
        {list && list.length === 0 && <span className="muted small" style={{ lineHeight: 1.5 }}>В Strava кроссовки не добавлены. Внеси свои — Kouros будет считать их пробег по тренировкам с даты начала.</span>}
        {list && list.length > 0 && <span className="muted tiny" style={{ lineHeight: 1.5 }}>Пробег — тренировки, где выбраны эти кроссовки (в «Обзоре» Kouros спрашивает после пробежки или ходьбы), плюс тренировки без ответа с даты начала — их получают основные.</span>}
        {!form && <button type="button" className="btn2" style={{ alignSelf: 'flex-start' }} onClick={() => setForm({ name: '', kind: 'train', startDate: today, initialKm: '0', walks: false })}>+ Добавить кроссовки</button>}
      </section>
      {form && (
        <section className="card" aria-label="Новые кроссовки">
          <span className="label">Новые кроссовки</span>
          <div className="field"><label htmlFor="sh-name">Модель</label><input id="sh-name" style={{ width: '100%' }} value={form.name} placeholder="Модель кроссовок" onChange={(e) => setForm({ ...form, name: e.target.value })} /></div>
          <div className="row" role="group" aria-label="Тип">
            {SHOE_KIND.map(([k, n]) => <button key={k} type="button" className="pill" aria-pressed={form.kind === k} onClick={() => setForm({ ...form, kind: k })}>{n}</button>)}
          </div>
          <div className="row" style={{ gap: 16, alignItems: 'flex-end' }}>
            <div className="field"><label htmlFor="sh-date">Ношу с</label><input id="sh-date" type="date" style={{ width: 170 }} value={form.startDate} onChange={(e) => setForm({ ...form, startDate: e.target.value })} /></div>
            <div className="field"><label htmlFor="sh-km">Пробег до этой даты, км</label><input id="sh-km" inputMode="decimal" value={form.initialKm} onChange={(e) => setForm({ ...form, initialKm: e.target.value })} /></div>
          </div>
          <label className="row small" style={{ gap: 10, cursor: 'pointer' }}>
            <input type="checkbox" checked={form.walks} onChange={(e) => setForm({ ...form, walks: e.target.checked })} /> Ношу и для ходьбы — считать её пробег тоже
          </label>
          <span className="muted tiny">Ресурс: тренировочные ≈ 700 км, соревновательные ≈ 400, трейловые ≈ 600, для ходьбы ≈ 900.</span>
          <div className="row">
            <button type="button" className="btn" disabled={!form.name.trim()} onClick={add}>Добавить</button>
            <button type="button" className="btn2" onClick={() => setForm(null)}>Отмена</button>
          </div>
        </section>
      )}
    </div>
  );
}
