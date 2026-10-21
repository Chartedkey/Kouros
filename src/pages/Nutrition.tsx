import { useCallback, useEffect, useMemo, useState } from 'react';
import { api, errText, plural, type AthleteProfile, type NutDay, type NutritionDay, type NutritionSummary, type Pt } from '../lib/api';
import { useDataChanged } from '../lib/events';
import { LineBand } from './Health';
import type { Go } from '../lib/nav';

type Sub = 'overview' | 'history';

const n0 = (x: number | null | undefined) => (x == null ? '—' : Math.round(x).toLocaleString('ru-RU'));
const n1 = (x: number | null | undefined) => (x == null ? '—' : x.toFixed(1).replace('.', ','));
const dayLabel = (d: string) => new Date(d + 'T12:00:00').toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' });
const longDay = (d: string) => { const s = new Date(d + 'T12:00:00').toLocaleDateString('ru-RU', { weekday: 'long', day: 'numeric', month: 'long' }); return s.charAt(0).toUpperCase() + s.slice(1); };
const addDay = (d: string, k: number) => { const x = new Date(d + 'T12:00:00Z'); x.setUTCDate(x.getUTCDate() + k); return x.toISOString().slice(0, 10); };
const C = { kcal: '#F5B642', protein: '#3DDC97', carbs: '#7FD3FF', fat: '#FF8A65', fiber: '#C792EA', water: '#9DB8FF' };

function Bar({ v, max, color }: { v: number; max: number; color: string }) {
  return (
    <div style={{ height: 8, borderRadius: 999, background: '#22262D', overflow: 'hidden' }}>
      <div style={{ width: `${Math.max(0, Math.min(100, (v / Math.max(max, 1)) * 100))}%`, height: '100%', background: color, borderRadius: 999 }} />
    </div>
  );
}

export function Nutrition({ onHealth, onLabs, go }: { onHealth: () => void; onLabs?: () => void; go?: (g: Go) => void }) {
  const [sub, setSub] = useState<Sub>('overview');
  const [n, setN] = useState<NutritionSummary | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(() => { api.nutritionSummary().then((v) => { setN(v); setErr(null); }).catch((e) => setErr(errText(e))); }, []);
  useEffect(load, [load]);
  useDataChanged(load);
  return (
    <>
      <div className="page-head">
        <div className="stack">
          <h1>Питание</h1>
          <p className="subtitle">Цели по калориям, белкам, жирам, углеводам и воде пересчитываются каждый день — под твою цель и нагрузку из Strava</p>
        </div>
      </div>
      <nav className="subtabs" aria-label="Разделы питания">
        {([['overview', 'Обзор'], ['history', 'История']] as [Sub, string][]).map(([id, t]) => (
          <button key={id} type="button" aria-pressed={sub === id} onClick={() => setSub(id)}>{t}</button>
        ))}
      </nav>
      {err && <div className="note-err">{err}</div>}
      {!n && !err && <div className="muted">Считаю…</div>}
      {n && sub === 'overview' && <Overview n={n} reload={load} onHealth={go ? () => go({ tab: 'health', sub: 'history', card: 'Состав тела' }) : onHealth} onLabs={onLabs} />}
      {n && sub === 'history' && <History n={n} />}
      <p className="muted tiny" style={{ textAlign: 'center', margin: 0 }}>Еда — записи YAZIO и Waterllama через Apple Health. Цели — ориентир, а не медицинская рекомендация.</p>
    </>
  );
}

// ---------- overview ----------

function Overview({ n, reload, onHealth, onLabs }: { n: NutritionSummary; reload: () => void; onHealth: () => void; onLabs?: () => void }) {
  const t = n.targets, e = n.eaten;
  const [day, setDay] = useState(n.today);
  const rows: [string, number | null, number, string, string, string][] = [
    ['Калории', e.kcal, t.kcal, 'ккал', C.kcal, e.kcal != null ? `осталось ${n0(Math.max(0, t.kcal - e.kcal))}` : 'записей пока нет'],
    ['Белки', e.protein, t.protein, 'г', C.protein, `${n1(t.protein / n.weight)} г/кг${n.goal.proteinManual == null ? ' · по цели' : ' · вручную'}`],
    ['Углеводы', e.carbs, t.carbs, 'г', C.carbs, `${n1(t.carbs / n.weight)} г/кг · заполняют остаток`],
    ['Жиры', e.fat, t.fat, 'г', C.fat, `${n1(t.fat / n.weight)} г/кг`],
    ['Клетчатка', e.fiber != null || e.fiberEst ? (e.fiber ?? 0) + (e.fiberEst ?? 0) : null, t.fiber, 'г', C.fiber,
      e.fiberEst ? `из них ≈${n1(e.fiberEst)} г оценено по продуктам без данных · 14 г на 1000 ккал` : '14 г на 1000 ккал'],
    ['Вода', e.waterMl != null ? e.waterMl / 1000 : null, t.waterMl / 1000, 'л', C.water, t.trainMin ? `с учётом ${n0(t.trainMin)} мин тренировок` : '35 мл на кг'],
  ];
  const parts: [string, number][] = [['Базовый обмен', t.bmr], ['Обычная активность за день', t.neat], ['Тренировки дня из Strava (80%)', t.train]];
  if (t.goalKcal) parts.push([t.goalKcal < 0 ? 'Дефицит под цель' : 'Профицит под цель', t.goalKcal]);
  return (
    <>
      <div className="grid2">
        <section className="card" aria-label="Цели на сегодня">
          <div className="between"><span className="label">Цели на сегодня · съедено</span><span className="muted tiny">вес {n1(n.weight)} кг (тренд)</span></div>
          <div className="stack" style={{ gap: 12 }}>
            {rows.map(([name, v, tg, u, c, note]) => (
              <div key={name} className="stack" style={{ gap: 5 }}>
                <div className="between small"><b>{name}</b><span className="num">{u === 'л' ? n1(v) : n0(v)} <span className="muted">из {u === 'л' ? n1(tg) : n0(tg)} {u}</span></span></div>
                <Bar v={v ?? 0} max={tg} color={c} />
                <span className="muted tiny">{note}</span>
              </div>
            ))}
          </div>
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>Белки и жиры считаются на килограмм веса, углеводы заполняют остаток калорий — поэтому в тяжёлые дни растут именно они.</span>
        </section>
        <div className="stack" style={{ gap: 20 }}>
          <section className="card" aria-label="Из чего сложились калории">
            <span className="label">Из чего сложились калории</span>
            <div>
              {parts.map(([name, v]) => <div key={name} className="list-row small"><span className="muted">{name}</span><span className="num">{v > 0 && name !== 'Базовый обмен' ? '+' : ''}{n0(v)}</span></div>)}
              <div className="list-row small"><b>Цель на сегодня</b><b className="num">{n0(t.kcal)} ккал</b></div>
            </div>
            <span className="muted tiny" style={{ lineHeight: 1.5 }}>Базовый обмен — по формуле Миффлина — Сан Жеора (рост, вес, возраст). Тренировки — калории из Strava, а если их нет — оценка по нагрузке.</span>
          </section>
          <GoalCard n={n} reload={reload} onHealth={onHealth} />
        </div>
      </div>

      <DayMeals date={day} today={n.today} onDate={setDay} target={n.days.find((d) => d.date === day)?.target ?? t} />

      <div className="grid2">
        <Quality n={n} />
        <Window n={n} />
      </div>
      <DeficitCard n={n} reload={reload} onLabs={onLabs} />
      <Micro n={n} />
    </>
  );
}

/** «Возможная нехватка» (0.20): 28 days of full food days, below 60% / 40% of the norm. */
function DeficitCard({ n, reload, onLabs }: { n: NutritionSummary; reload: () => void; onLabs?: () => void }) {
  const d = n.deficits;
  const [open, setOpen] = useState<string | null>(null);
  if (!d || d.items.length === 0) return null;
  const col = { bad: '#FF6B6B', warn: '#F5B642', info: '#7FD3FF' };
  return (
    <section className="card" aria-label="Возможная нехватка">
      <div className="between"><span className="label">Возможная нехватка в питании</span><span className="muted tiny">{plural(d.days, 'полный день', 'полных дня', 'полных дней')} из 28</span></div>
      <div className="stack" style={{ gap: 10 }}>
        {d.items.map((x) => (
          <div key={x.key} className="stack" style={{ gap: 6, padding: '10px 12px', borderRadius: 10, background: col[x.level] + '12', border: `1px solid ${col[x.level]}44` }}>
            <div className="between small" style={{ gap: 8 }}>
              <span><b style={{ color: col[x.level] }}>{x.name}</b> · {x.pct}% нормы</span>
              <button type="button" className="linkbtn tiny" onClick={() => setOpen(open === x.key ? null : x.key)}>{open === x.key ? 'Свернуть' : 'Подробнее'}</button>
            </div>
            <span className="small" style={{ lineHeight: 1.5 }}>{x.text} {x.advice}</span>
            {open === x.key && (
              <div className="stack small" style={{ gap: 4, lineHeight: 1.5 }}>
                <span><span className="muted">Как может проявляться:</span> {x.signs}.</span>
                <span><span className="muted">Где взять:</span> {x.foods}.</span>
                {x.test && <span><span className="muted">Анализ:</span> {x.test}.</span>}
                <div className="row" style={{ gap: 12, flexWrap: 'wrap' }}>
                  {x.test && onLabs && <button type="button" className="linkbtn tiny" onClick={onLabs}>Загрузить результаты анализов →</button>}
                  <button type="button" className="linkbtn tiny" onClick={() => api.deficitSnooze(x.key).then(reload).catch(() => {})}>Не напоминать 30 дней</button>
                </div>
              </div>
            )}
          </div>
        ))}
      </div>
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>Считается по дням, где записано не меньше 60% калорий: еда из YAZIO, оценка по продуктам без данных и отмеченные добавки. Это подсказка, а не диагноз: точно покажет только анализ.</span>
    </section>
  );
}

const GOALS: [('maintain' | 'lose' | 'gain'), string][] = [['maintain', 'Поддержание'], ['lose', 'Снижение'], ['gain', 'Набор']];
const RATES: [number, string][] = [[0.25, 'Осторожно · 0,25 кг/нед'], [0.5, 'Обычно · 0,5'], [0.75, 'Быстрее · 0,75']];

function GoalCard({ n, reload, onHealth }: { n: NutritionSummary; reload: () => void; onHealth: () => void }) {
  const [p, setP] = useState<(AthleteProfile & { hrRestAuto?: number }) | null>(null);
  const [tw, setTw] = useState('');
  const [msg, setMsg] = useState<string | null>(null);
  useEffect(() => { api.profileGet().then((v) => { setP(v); setTw(v.targetWeight != null ? String(v.targetWeight).replace('.', ',') : ''); }).catch(() => {}); }, []);
  async function save(next: Partial<AthleteProfile>) {
    if (!p) return;
    const merged = { ...p, ...next };
    try {
      const { hrRestAuto: _a, ...prof } = merged;
      await api.profileSet(prof);
      setP(merged); setMsg(null); reload();
    } catch (e) { setMsg(errText(e)); }
  }
  const g = n.goal;
  const kcalPerDay = Math.round((g.rate * 7700) / 7);
  return (
    <section className="card" aria-label="Моя цель">
      <span className="label">Моя цель</span>
      <div className="row" role="group" aria-label="Цель">
        {GOALS.map(([k, t]) => <button key={k} type="button" className="pill" aria-pressed={g.goal === k} onClick={() => save({ goal: k })}>{t}</button>)}
      </div>
      {g.goal !== 'maintain' && (
        <>
          <div className="row" role="group" aria-label="Темп">
            {RATES.map(([r, t]) => <button key={r} type="button" className="pill" aria-pressed={Math.abs(g.rate - r) < 0.01} onClick={() => save({ goalRate: r })}>{t}</button>)}
          </div>
          <div className="row" style={{ gap: 12, alignItems: 'flex-end' }}>
            <div className="field"><label htmlFor="tw">Целевой вес, кг</label><input id="tw" inputMode="decimal" value={tw} onChange={(e) => setTw(e.target.value)} onBlur={() => { const v = Number(tw.replace(',', '.')); save({ targetWeight: v > 40 ? v : null }); }} /></div>
            <span className="small muted" style={{ paddingBottom: 10 }}>{g.etaWeeks ? `≈ ${plural(g.etaWeeks, 'неделя', 'недели', 'недель')} при таком темпе` : 'укажи, чтобы увидеть срок'}</span>
          </div>
          <span className="small" style={{ color: 'var(--text-2)', lineHeight: 1.5 }}>
            {g.goal === 'lose' ? `Дефицит ≈ ${n0(kcalPerDay)} ккал в день (не больше 25% от нормы, чтобы не страдали тренировки). Белок поднят до 2,0 г/кг — так уходит жир, а не мышцы.` : `Профицит ≈ ${n0(Math.min(500, kcalPerDay))} ккал в день. Набирать лучше вместе с силовыми — тогда растут мышцы, а не только жир.`}
          </span>
        </>
      )}
      {g.goal === 'maintain' && <span className="small muted" style={{ lineHeight: 1.5 }}>Калории — на поддержание текущего веса. Цель можно поменять в любой момент, все цифры пересчитаются.</span>}
      <div className="between small divider" style={{ paddingTop: 12 }}>
        <span>Белок <b className="num">{n1(g.proteinPerKg)} г/кг</b> <span className="muted">{g.proteinManual == null ? '· по цели: 1,8 — поддержание и набор, 2,0 — снижение' : '· задан вручную'}</span></span>
        {g.proteinManual != null && <button type="button" className="linkbtn small" onClick={() => save({ proteinManual: null })}>вернуть по цели</button>}
      </div>
      {g.monthly.length > 1 && (
        <div className="stack" style={{ gap: 0 }}>
          <span className="small" style={{ fontWeight: 700, paddingBottom: 4 }}>Вес по месяцам · медиана</span>
          <div className="row" style={{ gap: 14 }}>
            {g.monthly.map(([m, w]) => <span key={m} className="small"><span className="muted">{new Date(m + '-15').toLocaleDateString('ru-RU', { month: 'short' })}</span> <b className="num">{n1(w)}</b></span>)}
          </div>
          <button type="button" className="linkbtn small" style={{ alignSelf: 'flex-start', marginTop: 6 }} onClick={onHealth}>Состав тела подробно — в «Здоровье» →</button>
        </div>
      )}
      {msg && <div className="note-err">{msg}</div>}
    </section>
  );
}

function DayMeals({ date, today, onDate, target }: { date: string; today: string; onDate: (d: string) => void; target: { kcal: number; protein: number } }) {
  const [d, setD] = useState<NutritionDay | null>(null);
  useEffect(() => { setD(null); api.nutritionDay(date).then(setD).catch(() => setD(null)); }, [date]);
  const total = d?.meals.reduce((a, m) => a + m.kcal, 0) ?? 0;
  return (
    <section className="card" aria-label="Рацион">
      <div className="between">
        <span className="row" style={{ gap: 8 }}>
          <button type="button" className="xbtn" aria-label="Предыдущий день" onClick={() => onDate(addDay(date, -1))}>←</button>
          <b style={{ fontFamily: 'var(--display)', fontSize: 18 }}>Рацион · {longDay(date)}</b>
          <button type="button" className="xbtn" aria-label="Следующий день" disabled={date >= today} onClick={() => onDate(addDay(date, 1))}>→</button>
        </span>
        <input type="date" className="select" value={date} max={today} aria-label="Выбрать день" onChange={(e) => e.target.value && onDate(e.target.value)} />
      </div>
      {!d && <span className="muted small">Загружаю…</span>}
      {d && (
        <>
          <div className="tiles4" style={{ gridTemplateColumns: 'repeat(5, minmax(0, 1fr))' }}>
            <div className="tile"><span className="num tile-v2" style={{ color: C.kcal }}>{n0(d.kcal)}</span><span className="muted tiny">ккал · цель {n0(target.kcal)}</span></div>
            <div className="tile"><span className="num tile-v2" style={{ color: C.protein }}>{n0(d.protein)} г</span><span className="muted tiny">белки · цель {n0(target.protein)}</span></div>
            <div className="tile"><span className="num tile-v2" style={{ color: C.carbs }}>{n0(d.carbs)} г</span><span className="muted tiny">углеводы</span></div>
            <div className="tile"><span className="num tile-v2" style={{ color: C.fat }}>{n0(d.fat)} г</span><span className="muted tiny">жиры{d.kcal && d.fat ? ` · ${Math.round((d.fat * 9 / d.kcal) * 100)}% калорий` : ''}</span></div>
            <div className="tile"><span className="num tile-v2">{d.latePct != null ? `${d.latePct}%` : '—'}</span><span className="muted tiny">калорий после 20:00</span></div>
          </div>
          {d.meals.length ? (
            <div>
              {d.meals.map((m, i) => (
                <div key={i} className="meal">
                  <span className="num small" style={{ fontWeight: 700 }}>{m.time}</span>
                  <span className="stack" style={{ gap: 2, minWidth: 0 }}>
                    <b className="small">{m.label}</b>
                    <span className="muted small" style={{ lineHeight: 1.4 }}>{m.names.join(' · ')}</span>
                  </span>
                  <span className="stack" style={{ gap: 2, alignItems: 'flex-end' }}>
                    <b className="num small">{n0(m.kcal)} ккал</b>
                    <span className="muted tiny num">Б {n0(m.protein)} · У {n0(m.carbs)} · Ж {n0(m.fat)}</span>
                  </span>
                  <div className="mealbar"><div style={{ width: `${total ? (m.kcal / total) * 100 : 0}%` }} /></div>
                </div>
              ))}
            </div>
          ) : <span className="muted small">За этот день нет записей в YAZIO.</span>}
          {(d.items?.length ?? 0) > 0 && <FoodItems items={d.items!} />}
          <span className="muted tiny">День питания считается с 5 утра, чтобы ночной перекус попал в свой вечер. Итоги сверху — как в YAZIO.</span>
        </>
      )}
    </section>
  );
}

const KIND: Record<string, [string, string]> = {
  yazio: ['из YAZIO', '#3DDC97'], history: ['≈ по твоей прошлой записи', '#9DB8FF'], reference: ['≈ по справочнику', '#C792EA'],
  unknown: ['не узнал', '#F5B642'], supplement: ['добавка — не в еде', 'var(--muted)'],
};

/** Every product of the day and where its fiber and micros come from. */
function FoodItems({ items }: { items: NonNullable<NutritionDay['items']> }) {
  const [open, setOpen] = useState(false);
  const est = items.filter((i) => i.kind === 'history' || i.kind === 'reference').length;
  return (
    <div className="stack" style={{ gap: 6 }}>
      <button type="button" className="linkbtn small" style={{ alignSelf: 'flex-start' }} onClick={() => setOpen(!open)}>
        {open ? 'Скрыть продукты' : `Продукты и откуда витамины (${items.length}, оценено ${est})`}
      </button>
      {open && items.map((it, i) => {
        const [t, c] = KIND[it.kind] ?? KIND.unknown;
        return (
          <div key={i} className="list-row small" style={{ alignItems: 'flex-start' }}>
            <span className="num muted" style={{ width: 44 }}>{it.time}</span>
            <span className="stack" style={{ gap: 2, flex: 1, minWidth: 0 }}>
              <span>{it.name}</span>
              <span className="tiny" style={{ color: c }}>{t}{it.parts.length ? ` · ${it.parts.join(', ')}` : ''}</span>
            </span>
            <span className="stack" style={{ gap: 2, alignItems: 'flex-end' }}>
              <span className="num">{n0(it.kcal)} ккал</span>
              {(it.kind === 'history' || it.kind === 'reference') && <span className="muted tiny num">≈ клетч. {n1(it.fiber)} г · K {n0(it.potassium)} · Mg {n0(it.magnesium)}</span>}
            </span>
          </div>
        );
      })}
    </div>
  );
}

function Quality({ n }: { n: NutritionSummary }) {
  const q = n.quality, t = n.targets;
  const items: [string, string, string, string][] = [
    ['Клетчатка', q.fiberWithEst != null && q.fiberWithEst > (q.fiber ?? 0) + 0.5 ? `≈${n1(q.fiberWithEst)} г в день` : q.fiber != null ? `${n1(q.fiber)} г в день` : '—',
      q.fiberWithEst != null && q.fiberWithEst > (q.fiber ?? 0) + 0.5 ? `с оценкой · по данным YAZIO ${n1(q.fiber)} · нужно ${n0(t.fiber)} г` : `нужно ${n0(t.fiber)} г`,
      (q.fiberWithEst ?? q.fiber) != null && (q.fiberWithEst ?? q.fiber)! < t.fiber * 0.7 ? '#F5B642' : '#3DDC97'],
    ['Белок', q.protein != null ? `${n0(q.protein)} г в день` : '—', q.proteinPerKg != null ? `${n1(q.proteinPerKg)} г/кг · цель ${n1(n.goal.proteinPerKg)}` : '', q.proteinPerKg != null && q.proteinPerKg < n.goal.proteinPerKg * 0.85 ? '#F5B642' : '#3DDC97'],
    ['Жиры', q.fatPct != null ? `${q.fatPct}% калорий` : '—', 'ориентир около 30%', q.fatPct != null && q.fatPct > 36 ? '#F5B642' : '#3DDC97'],
    ['Сахар', q.sugar != null ? `${n0(q.sugar)} г в день` : '—', q.sugarPct != null ? `${q.sugarPct}% калорий · норма до 10%` : '', q.sugarPct != null && q.sugarPct > 10 ? '#F5B642' : '#3DDC97'],
    ['Овощи', `${q.vegDays} из ${q.fullDays} дней`, 'по названиям блюд · цель — каждый день', q.fullDays && q.vegDays < q.fullDays * 0.7 ? '#F5B642' : '#3DDC97'],
  ];
  return (
    <section className="card" aria-label="Качество рациона">
      <div className="between"><span className="label">Качество рациона</span><span className="muted tiny">{plural(q.fullDays, 'полный день', 'полных дня', 'полных дней')} за месяц</span></div>
      <div>
        {items.map(([name, v, note, c]) => (
          <div key={name} className="list-row small">
            <span className="row" style={{ gap: 8, flex: 1 }}><span className="dot" style={{ background: c }} /><b>{name}</b></span>
            <span className="stack" style={{ gap: 0, alignItems: 'flex-end' }}><b className="num">{v}</b><span className="muted tiny">{note}</span></span>
          </div>
        ))}
      </div>
      {q.top.length > 0 && (
        <div className="stack" style={{ gap: 0 }}>
          <span className="small" style={{ fontWeight: 700, paddingBottom: 4 }}>Что ты ешь чаще всего · 30 дней</span>
          {q.top.map(([name, c]) => <div key={name} className="list-row small"><span className="ellipsis" style={{ flex: 1 }}>{name}</span><span className="muted num">× {c}</span></div>)}
        </div>
      )}
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>Считаются только полные дни. У части продуктов в YAZIO клетчатка и микроэлементы не заполнены — реальные цифры могут быть выше.</span>
    </section>
  );
}

function Window({ n }: { n: NutritionSummary }) {
  const w = n.window;
  // Timeline 05:00 → 05:00 next day.
  const pos = (m: number) => `${(m / 1440) * 100}%`;
  return (
    <section className="card" aria-label="Режим питания">
      <div className="between"><span className="label">Режим питания · когда ты ешь</span><span className="muted tiny">{plural(w.days, 'день', 'дня', 'дней')} с записями</span></div>
      <div className="tiles4">
        <div className="tile"><span className="num tile-v2">{w.first ?? '—'}</span><span className="muted tiny">первый приём · медиана</span></div>
        <div className="tile"><span className="num tile-v2">{w.last ?? '—'}</span><span className="muted tiny">последний приём</span></div>
        <div className="tile"><span className="num tile-v2">{w.hours != null ? `${n1(w.hours)} ч` : '—'}</span><span className="muted tiny">окно питания</span></div>
        <div className="tile"><span className="num tile-v2" style={{ color: w.afterMidnight > w.days / 3 ? '#F5B642' : undefined }}>{w.afterMidnight} из {w.days}</span><span className="muted tiny">дней с едой после полуночи</span></div>
      </div>
      <div className="stack" style={{ gap: 4 }}>
        {w.byDay.map(([d, a, b, fa, fb]) => (
          <div key={d} className="winrow">
            <span className="muted tiny">{dayLabel(d)}</span>
            <div className="wintrack">
              <div className="midnight" style={{ left: pos(19 * 60) }} />
              <div className="win" style={{ left: pos(a), width: `calc(${pos(Math.max(b - a, 8))})` }} title={`${fa} — ${fb}`} />
            </div>
          </div>
        ))}
      </div>
      <div className="between tiny muted num" style={{ paddingLeft: 66 }}><span>05:00</span><span>11:00</span><span>17:00</span><span>23:00</span><span>05:00</span></div>
      <div className="stack" style={{ gap: 6 }}>
        <span className="small"><b>Калории по времени суток</b> <span className="muted">· 7 дней</span></span>
        <div style={{ display: 'flex', height: 14, borderRadius: 999, overflow: 'hidden', background: '#22262D' }}>
          <div style={{ width: `${w.parts[0]}%`, background: '#7FD3FF' }} /><div style={{ width: `${w.parts[1]}%`, background: '#3DDC97' }} /><div style={{ width: `${w.parts[2]}%`, background: '#F5B642' }} />
        </div>
        <div className="row tiny" style={{ gap: 14 }}>
          <span><b style={{ color: '#7FD3FF' }}>{w.parts[0]}%</b> до 12:00</span><span><b style={{ color: '#3DDC97' }}>{w.parts[1]}%</b> 12:00–20:00</span><span><b style={{ color: '#F5B642' }}>{w.parts[2]}%</b> после 20:00</span>
        </div>
      </div>
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>Пунктир — полночь. Тяжёлая поздняя еда мешает глубокому сну и HRV — в «Здоровье» дневник уже сравнивает дни с поздним ужином и без.</span>
    </section>
  );
}

function Micro({ n }: { n: NutritionSummary }) {
  const list = n.micro.filter((m) => m.pct != null || m.pctEst != null || m.noData);
  const src = n.norms?.list.find((c) => c.code === n.norms?.country);
  const supp = n.micro.filter((m) => (m.supp7 ?? 0) > 0).map((m) => m.name);
  return (
    <section className="card" aria-label="Микронутриенты">
      <div className="between"><span className="label">Микронутриенты · 7 дней · % от нормы</span><span className="muted tiny">из еды (и отдельно — из добавок) · нормы: {src ? `${src.name} (${src.source})` : 'Россия'}</span></div>
      {list.length ? (
        <div className="micro">
          {list.map((m) => {
            if (m.noData) {
              return (
                <div key={m.key} className="stack" style={{ gap: 4 }}>
                  <div className="between small"><span>{m.name}</span><span className="muted tiny">мало данных</span></div>
                  <Bar v={0} max={150} color="#3A404A" />
                  <span className="muted tiny">YAZIO почти не передаёт · норма {String(m.rda).replace('.', ',')} {m.unit}</span>
                  {m.fromSupp != null && <span className="tiny" style={{ color: '#9DB8FF' }}>из добавок {n1(m.fromSupp)} {m.unit} в день · {m.pctAll}% нормы</span>}
                </div>
              );
            }
            const real = m.pct ?? 0;
            const pct = m.pctEst ?? real;
            const c = m.key.includes('sodium') ? (pct > 150 ? '#F5B642' : '#3DDC97') : pct < 50 ? '#FF6B6B' : pct < 80 ? '#F5B642' : '#3DDC97';
            return (
              <div key={m.key} className="stack" style={{ gap: 4 }}>
                <div className="between small"><span>{m.name}</span><span className="num" style={{ color: c, fontWeight: 700 }}>{m.pctEst != null ? '≈' : ''}{pct}%</span></div>
                <div className="stack" style={{ gap: 0, position: 'relative' }}>
                  <Bar v={Math.min(pct, 150)} max={150} color={m.pctEst != null ? `${c}66` : c} />
                  {m.pctEst != null && <div className="est-real" style={{ width: `${(Math.min(real, 150) / 150) * 100}%`, background: c }} />}
                </div>
                <span className="muted tiny">
                  {m.avgEst != null ? `≈${n1(m.avgEst)} ${m.unit} в день · из YAZIO ${n1(m.avg)}` : `${n1(m.avg)} ${m.unit} в день`} · норма {String(m.rda).replace('.', ',')}{m.days < 4 && m.avgEst == null ? ` · данных ${m.days} дн.` : ''}
                </span>
                {m.fromSupp != null && <span className="tiny" style={{ color: '#9DB8FF' }}>+ из добавок {n1(m.fromSupp)} {m.unit} в день · вместе {m.pctAll}%</span>}
              </div>
            );
          })}
        </div>
      ) : <span className="muted small">YAZIO пока не передал витамины и минералы за эту неделю.</span>}
      {n.estimate && n.estimate.estimated7 > 0 && (
        <span className="muted tiny" style={{ lineHeight: 1.5 }}>
          ≈ — с оценкой: за 7 дней {n.estimate.estimated7} из {n.estimate.entries7} записей пришли только с БЖУ, их клетчатка и микроэлементы посчитаны по твоим прошлым записям того же продукта или по справочнику. Тёмная часть полосы — данные YAZIO, светлая — оценка.
          {n.estimate.unknown7.length > 0 && ` Не узнал: ${n.estimate.unknown7.slice(0, 6).join(', ')}${n.estimate.unknown7.length > 6 ? '…' : ''}.`}
        </span>
      )}
      {supp.length > 0 && <span className="muted tiny" style={{ lineHeight: 1.5 }}>Добавки, записанные в YAZIO (витамины, омега-3 и т. п.), не считаются. Голубая строка «из добавок» — только то, что ты отметил в «Здоровье» → «Витамины и лекарства».</span>}
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>Натрий — без соли при готовке, её никто не записывает. Шкала до 150%; натрий — верхний ориентир, его как раз не стоит превышать сильно. Нормы — для взрослого мужчины; при больших нагрузках потребность в магнии, калии и натрии выше.</span>
    </section>
  );
}

// ---------- history ----------

const PERIODS: [number, string][] = [[7, 'Неделя'], [30, 'Месяц'], [90, '3 месяца']];

function History({ n }: { n: NutritionSummary }) {
  const [days, setDays] = useState(30);
  const [sel, setSel] = useState(addDay(n.today, -1));
  const list = useMemo(() => n.days.slice(-days), [n.days, days]);
  const full = list.filter((d) => d.status === 2);
  const avg = (f: (d: NutDay) => number | null) => { const v = full.map(f).filter((x): x is number => x != null); return v.length ? v.reduce((a, b) => a + b, 0) / v.length : null; };
  const kcal = avg((d) => d.kcal), spent = avg((d) => d.spent), prot = avg((d) => d.protein), carbs = avg((d) => d.carbs), fat = avg((d) => d.fat), water = avg((d) => d.waterMl);
  const max = Math.max(1, ...list.map((d) => Math.max(d.kcal ?? 0, d.spent ?? 0, d.target.kcal)));
  const counts = [0, 1, 2].map((s) => list.filter((d) => d.status === s).length);
  const protPts: Pt[] = list.filter((d) => d.protein != null && d.status === 2).map((d) => [d.date, d.protein!]);
  return (
    <>
      <DayMeals date={sel} today={n.today} onDate={setSel} target={n.days.find((d) => d.date === sel)?.target ?? n.targets} />
      <div className="row" role="group" aria-label="Период">
        {PERIODS.map(([d, t]) => <button key={d} type="button" className="pill" aria-pressed={days === d} onClick={() => setDays(d)}>{t}</button>)}
        <span className="muted small">средние по {plural(full.length, 'полному дню', 'полным дням', 'полным дням')} · нажми на день, чтобы открыть его рацион наверху</span>
      </div>
      <div className="tiles">
        <div className="tile"><span className="num tile-v">{n0(kcal)}</span><span className="muted small">ккал в среднем{spent ? ` · расход по часам ${n0(spent)}` : ''}</span></div>
        <div className="tile"><span className="num tile-v">{n0(prot)} г</span><span className="muted small">белки · {prot ? n1(prot / n.weight) : '—'} г/кг</span></div>
        <div className="tile"><span className="num tile-v">{n0(carbs)} г</span><span className="muted small">углеводы · {carbs ? n1(carbs / n.weight) : '—'} г/кг</span></div>
        <div className="tile"><span className="num tile-v">{n0(fat)} г</span><span className="muted small">жиры · {fat && kcal ? Math.round((fat * 9 / kcal) * 100) : '—'}% калорий</span></div>
        <div className="tile"><span className="num tile-v">{water ? n1(water / 1000) : '—'} л</span><span className="muted small">вода в среднем</span></div>
      </div>
      <section className="card" aria-label="Съедено и потрачено">
        <div className="between"><span className="label">Съедено и потрачено по дням</span>
          <span className="row tiny" style={{ gap: 12 }}><span className="row" style={{ gap: 4 }}><span className="dot" style={{ background: C.kcal }} />съедено (YAZIO)</span><span className="row" style={{ gap: 4 }}><span className="dot" style={{ background: '#4a5262' }} />расход (Apple Watch)</span><span className="row" style={{ gap: 4 }}><span className="dot" style={{ background: '#f1f3f5' }} />цель</span></span></div>
        <div className="eatbars" style={{ height: 150 }}>
          {list.map((d) => (
            <button key={d.date} type="button" className={`ebar plain${d.date === sel ? ' sel' : ''}`} onClick={() => setSel(d.date)} title={`${dayLabel(d.date)}: съедено ${n0(d.kcal)}, расход ${n0(d.spent)}, цель ${n0(d.target.kcal)}`}>
              <div className="epair">
                <div style={{ height: `${((d.spent ?? 0) / max) * 100}%`, background: '#4a5262' }} />
                <div style={{ height: `${((d.kcal ?? 0) / max) * 100}%`, background: d.status === 1 ? '#7a6230' : C.kcal }} />
              </div>
              <div className="etarget" style={{ bottom: `${(d.target.kcal / max) * 100}%` }} />
            </button>
          ))}
        </div>
        <div className="between tiny muted"><span>{dayLabel(list[0].date)}</span><span>{dayLabel(list[list.length - 1].date)}</span></div>
        <span className="muted tiny" style={{ lineHeight: 1.5 }}>Тусклые столбцы — дни, где, похоже, записано не всё. Расход по часам часто завышен; через 3–4 недели Kouros сверит его с изменением веса.</span>
      </section>
      <div className="grid2">
        <section className="card" aria-label="Доля БЖУ">
          <span className="label">Доля белков, жиров и углеводов в калориях</span>
          <div className="macrobars" style={{ height: 110 }}>
            {list.map((d) => {
              const k = (d.protein ?? 0) * 4 + (d.carbs ?? 0) * 4 + (d.fat ?? 0) * 9;
              if (!k || d.status === 0) return <div key={d.date} className="mbar" />;
              const fp = Math.round(((d.fat ?? 0) * 9 / k) * 100);
              return (
                <button key={d.date} type="button" className={`mbar plain${d.date === sel ? ' sel' : ''}`} onClick={() => setSel(d.date)} title={`${dayLabel(d.date)}: жиры ${fp}%`}>
                  <div className="mstack">
                    <div style={{ flex: (d.fat ?? 0) * 9, background: C.fat }} />
                    <div style={{ flex: (d.carbs ?? 0) * 4, background: C.carbs }} />
                    <div style={{ flex: (d.protein ?? 0) * 4, background: C.protein }} />
                  </div>
                  {days <= 30 && <span className="tiny muted num">{fp}</span>}
                </button>
              );
            })}
          </div>
          <span className="row tiny" style={{ gap: 12 }}><span className="row" style={{ gap: 4 }}><span className="dot" style={{ background: C.protein }} />белки</span><span className="row" style={{ gap: 4 }}><span className="dot" style={{ background: C.carbs }} />углеводы</span><span className="row" style={{ gap: 4 }}><span className="dot" style={{ background: C.fat }} />жиры</span><span className="muted">под столбцом — % жиров, ориентир около 30</span></span>
        </section>
        <section className="card" aria-label="Белок по дням">
          <span className="label">Белок по дням · г</span>
          <LineBand pts={protPts} band={[n.targets.protein, n.targets.protein * 0.9, n.targets.protein * 1.1]} color={C.protein} fmt={(v) => String(Math.round(v))} sel={sel} onPick={setSel} />
          <span className="muted tiny">Полоса — цель ±10%. Только полные дни.</span>
        </section>
      </div>
      <section className="card" aria-label="Полнота записей">
        <div className="between"><span className="label">Полнота записей</span><span className="row tiny" style={{ gap: 12 }}><span>{counts[2]} полных</span><span className="muted">{counts[1]} под вопросом</span><span className="muted">{counts[0]} без записей</span></span></div>
        <div className="fullness">
          {list.map((d) => <button key={d.date} type="button" className={`fcell${d.date === sel ? ' sel' : ''}`} title={`${dayLabel(d.date)}: ${['нет записей', 'похоже, не весь день', 'полный день'][d.status]}`} onClick={() => setSel(d.date)} style={{ background: ['#1c2026', '#7a6230', '#3DDC97'][d.status] }} />)}
        </div>
        <span className="muted tiny">Полный день — записано хотя бы 60% цели по калориям. Выводы и средние строятся только по полным дням.</span>
      </section>
    </>
  );
}
