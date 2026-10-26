// «Боль и восстановление» and «Цель» in the coach tab (0.16).
import { useCallback, useEffect, useState } from 'react';
import { api, errText, type BodyState, type CoachProfile, type GoalRace, type PainAdvice } from '../lib/api';
import { useDataChanged } from '../lib/events';
import { LevelPills } from '../components/Checkin';
import { PainFigure, sideText } from '../components/PainFigure';
import { LineBand } from './Health';

const TREND: Record<string, [string, string]> = { better: ['лучше', '#3DDC97'], worse: ['хуже', '#FF6B6B'], same: ['без изменений', '#9AA3AE'], new: ['новая', '#9DB8FF'] };
const RUN_AREAS = ['foot', 'ankles', 'shin', 'calves', 'knees', 'quads', 'hams', 'hips', 'glutes'];
const ruDate = (d: string) => new Date(`${d}T12:00:00`).toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' });
const fmtT = (s: number) => { const t = Math.round(s); return t >= 3600 ? `${Math.floor(t / 3600)}:${String(Math.floor((t % 3600) / 60)).padStart(2, '0')}:${String(t % 60).padStart(2, '0')}` : `${Math.floor(t / 60)}:${String(t % 60).padStart(2, '0')}`; };

function useBody() {
  const [b, setB] = useState<BodyState | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(() => { api.bodyState().then((v) => { setB(v); setErr(null); }).catch((e) => setErr(errText(e))); }, []);
  useEffect(load, [load]);
  useDataChanged(load);
  return { b, err, load };
}

/** 1.4: what to do with an active pain — general advice, not a diagnosis. */
function PainAdviceBlock({ a }: { a: PainAdvice }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="stack" style={{ gap: 6, borderTop: '1px solid var(--line)', paddingTop: 8 }}>
      {a.doctor.length > 0 && <span className="small" style={{ color: '#FF6B6B', lineHeight: 1.5 }}>Покажись врачу: {a.doctor.join(', ')}.</span>}
      <button type="button" className="linkbtn small" style={{ alignSelf: 'flex-start' }} aria-expanded={open} onClick={() => setOpen(!open)}>
        Что делать · {a.phaseText}, {a.days} {a.days === 1 ? 'день' : a.days < 5 ? 'дня' : 'дней'} {open ? '▴' : '▾'}
      </button>
      {open && (
        <div className="stack small" style={{ gap: 6, lineHeight: 1.5 }}>
          {a.remove.map((t) => <span key={t}>• {t}</span>)}
          {a.complexes.length > 0 && <span>• {a.complexText} <b>{a.complexes.map((c) => c.name).join(', ')}</b>.</span>}
          <span>• {a.temp}</span>
          <span className="muted">• {a.ointment}</span>
          <span style={{ color: '#F5B642' }}>{a.redFlags}</span>
          <span className="muted tiny">Это общие советы, а не диагноз. План тренировок Kouros поправит сам — предложения появятся в «Плане».</span>
        </div>
      )}
    </div>
  );
}

export function CoachBody() {
  const { b, err, load } = useBody();
  const [sel, setSel] = useState<string | null>(null);
  const [lv, setLv] = useState<number | null>(null);
  const [note, setNote] = useState('');
  const [rArea, setRArea] = useState('');
  const [rStage, setRStage] = useState(0);
  const [msg, setMsg] = useState<string | null>(null);
  if (err) return <div className="note-err">{err}</div>;
  if (!b) return <p className="muted">Загружаю…</p>;
  const levels: Record<string, number> = {};
  for (const p of b.pains) if (p.active) levels[`${p.area}:${p.side}`] = p.level;
  const name = (k: string) => { const [a, s] = k.split(':'); return `${b.areas.find((x) => x.key === a)?.name ?? a}${s ? ` ${sideText(s)}` : ''}`; };
  const mark = async () => {
    if (!sel || lv == null) return;
    const [area, side] = sel.split(':');
    try { await api.painMark({ area, side, level: lv, note: note || undefined }); setSel(null); setLv(null); setNote(''); setMsg('Записал в дневник.'); load(); } catch (e) { setMsg(errText(e)); }
  };
  const runPains = b.pains.filter((p) => p.active && RUN_AREAS.includes(p.area));
  const r = b.return;
  return (
    <div className="stack" style={{ gap: 16 }}>
      <div className="grid2">
        <section className="card" aria-label="Дневник боли">
          <span className="label">Дневник боли</span>
          <span className="muted small" style={{ lineHeight: 1.5 }}>Нажми на место, где болит, и выбери силу боли. Отметки после тренировок попадают сюда сами. Через боль тренер не ставит нагрузку на это место — предложит замену в плане.</span>
          <PainFigure levels={levels} sel={sel} onPick={(a, s) => { setSel(`${a}:${s}`); setLv(null); setMsg(null); }} />
          {sel && (
            <div className="inset" style={{ gap: 8 }}>
              <b className="small">{name(sel)} — сейчас</b>
              <LevelPills value={lv} onChange={setLv} />
              <input className="input" placeholder="Что за боль, когда появилась (необязательно)" value={note} onChange={(e) => setNote(e.target.value)} />
              <div className="row" style={{ gap: 10 }}>
                <button type="button" className="btn" disabled={lv == null} onClick={mark}>Записать</button>
                <button type="button" className="linkbtn small" onClick={() => setSel(null)}>Отмена</button>
              </div>
            </div>
          )}
          {msg && <span className="small" style={{ color: '#3DDC97' }}>{msg}</span>}
        </section>
        <div className="stack" style={{ gap: 16 }}>
          <section className="card" aria-label="Что болит">
            <span className="label">Что болит · последние 60 дней</span>
            {b.pains.length === 0 && <span className="muted small">Ничего не отмечено — отлично.</span>}
            {b.pains.map((p) => {
              const [tw, tc] = TREND[p.trend];
              return (
                <div key={`${p.area}:${p.side}`} className="inset" style={{ gap: 6 }}>
                  <div className="between">
                    <b className="small">{p.name}</b>
                    <span className="row tiny" style={{ gap: 8 }}><span style={{ color: tc }}>{tw}</span><span className={`chip ${p.active ? 'chip-warn' : 'chip-off'}`}>{p.active ? `${p.level}/10` : `было ${p.level}/10`}</span></span>
                  </div>
                  {p.series.length >= 2 && <LineBand pts={p.series} band={[3, 0, 3]} color="#F5B642" height={70} invertGood fmt={(v) => `${Math.round(v)}`} />}
                  <span className="muted tiny">Последняя отметка {ruDate(p.date)} · дней с болью ≥3 за 2 недели: {p.days14}</span>
                  {p.entries.slice(0, 4).map((e) => (
                    <div key={e.id} className="between tiny"><span className="muted">{ruDate(e.date)} · {e.context === 'checkin' ? 'после тренировки' : e.context === 'morning' ? 'утром' : 'вручную'}{e.note ? ` · ${e.note}` : ''}</span><span className="row" style={{ gap: 8 }}><b>{e.level}/10</b><button type="button" className="xbtn" aria-label="Удалить" onClick={() => api.painDelete(e.id).then(load)}>×</button></span></div>
                  ))}
                  {p.active && p.days14 >= 7 && !p.advice?.doctor.length && <span className="tiny" style={{ color: '#F5B642', lineHeight: 1.5 }}>Болит уже больше недели — стоит показаться врачу или физиотерапевту.</span>}
                  {p.advice && <PainAdviceBlock a={p.advice} />}
                </div>
              );
            })}
          </section>
        </div>
      </div>
      <div className="grid2">
        <section className="card" aria-label="Возврат к бегу">
          <span className="label">Возврат к бегу</span>
          {r.active ? (
            <>
              <b>{r.areaName} · шаг {(r.stage ?? 0) + 1} из {r.steps.length}</b>
              <div className="stack" style={{ gap: 4 }}>
                {r.steps.map((t, i) => (
                  <span key={t + i} className="small" style={{ color: i < (r.stage ?? 0) ? '#3DDC97' : i === r.stage ? 'var(--text)' : 'var(--muted)', fontWeight: i === r.stage ? 700 : 400 }}>{i < (r.stage ?? 0) ? '✓' : i === r.stage ? '→' : '·'} {i + 1}. {t}</span>
                ))}
              </div>
              <span className="muted tiny" style={{ lineHeight: 1.5 }}>Шаг стоит в плане через день. После пробежки отметь боль, утром — как себя чувствует это место. 0–3 из 10 — дальше, 4–5 — повторяем шаг, 6 и больше — шаг назад.</span>
              {(r.history ?? []).slice(-4).reverse().map((h) => <span key={h.date + h.why} className="tiny muted">{ruDate(h.date)}: {h.why}</span>)}
              <button type="button" className="btn2" style={{ alignSelf: 'flex-start' }} onClick={() => api.returnStop().then(load)}>Закончить возврат</button>
            </>
          ) : (
            <>
              <span className="muted small" style={{ lineHeight: 1.5 }}>После травмы или перерыва — постепенно: ходьба, бег с ходьбой, потом лёгкий бег. На следующий шаг — только если боль во время и наутро не выше 3 из 10. Шаги сами встают в план.</span>
              {r.finished && <span className="small" style={{ color: '#3DDC97' }}>Прошлый возврат закончен {ruDate(r.finished)} — дальше обычный план.</span>}
              <div className="row" style={{ gap: 8, flexWrap: 'wrap' }}>
                <select className="select" value={rArea || runPains[0]?.area || ''} onChange={(e) => setRArea(e.target.value)}>
                  <option value="" disabled>Что восстанавливаем</option>
                  {b.areas.filter((a) => RUN_AREAS.includes(a.key) || a.key === 'other').map((a) => <option key={a.key} value={a.key}>{a.key === 'other' ? 'После перерыва или болезни' : a.name}</option>)}
                </select>
                <select className="select" value={rStage} onChange={(e) => setRStage(Number(e.target.value))}>
                  {r.steps.map((t, i) => <option key={i} value={i}>с шага {i + 1}: {t}</option>)}
                </select>
                <button type="button" className="btn" disabled={!(rArea || runPains[0]?.area)} onClick={() => api.returnStart(rArea || runPains[0].area, rStage).then(load)}>Начать</button>
              </div>
              {runPains.some((p) => p.days14 >= 5) && <span className="tiny" style={{ color: '#F5B642' }}>Боль держится несколько дней — возврат по шагам безопаснее, чем сразу обычный план.</span>}
            </>
          )}
        </section>
        <section className="card" aria-label="Что лучше сегодня">
          <span className="label">Восстановление · что лучше сегодня</span>
          {b.tools.have === 0 ? (
            <span className="muted small">Отметь в знакомстве («Профиль» → «Пройти знакомство заново»), чем можешь восстанавливаться — сауна, роллер, массажёр… — и тренер будет подсказывать, что лучше сегодня.</span>
          ) : (
            <>
              {b.tools.best.length === 0 && <span className="muted small">Сегодня особых средств не нужно — достаточно сна, еды и прогулки.</span>}
              {b.tools.best.map((t) => <div key={t.name} className="stack" style={{ gap: 2 }}><b className="small" style={{ color: '#3DDC97' }}>{t.name}</b><span className="small" style={{ lineHeight: 1.5 }}>{t.text}</span></div>)}
              {b.tools.more.map((t) => <span key={t.name} className="small muted" style={{ lineHeight: 1.5 }}>Ещё: <b>{t.name}</b> — {t.text}</span>)}
              {b.tools.not.map((t) => <span key={t.name} className="small" style={{ color: '#F5B642', lineHeight: 1.5 }}>Не сегодня: <b>{t.name}</b> — {t.text}</span>)}
            </>
          )}
        </section>
      </div>
    </div>
  );
}

const DIST: [number, string][] = [[5, '5 км'], [10, '10 км'], [21.1, 'Полумарафон'], [42.2, 'Марафон']];

export function CoachGoal({ profile, onSave }: { profile: CoachProfile; onSave: (p: CoachProfile) => Promise<void> }) {
  const { b, err, load } = useBody();
  const g0 = profile.goalRace ?? {};
  const [edit, setEdit] = useState(!profile.goalRace);
  const [g, setG] = useState<GoalRace>({ title: g0.title ?? '', distanceKm: g0.distanceKm ?? 21.1, target: g0.target ?? '', date: g0.date ?? '' });
  const [e2, setE2] = useState<string | null>(null);
  if (err) return <div className="note-err">{err}</div>;
  if (!b) return <p className="muted">Считаю прогноз…</p>;
  const st = b.goal;
  const save = async () => {
    if (g.target && !/^\d{1,2}:\d{2}(:\d{2})?$/.test(g.target.trim())) { setE2('Время — в виде 1:45:00 или 45:00'); return; }
    await onSave({ ...profile, goalRace: { ...g, target: g.target?.trim() } });
    setEdit(false); setE2(null); load();
  };
  return (
    <div className="stack" style={{ gap: 16 }}>
      <section className="card" aria-label="Цель">
        <div className="between"><span className="label">Цель</span>{!edit && <button type="button" className="linkbtn small" onClick={() => setEdit(true)}>Изменить</button>}</div>
        {edit ? (
          <div className="stack" style={{ gap: 10 }}>
            <input className="input" placeholder="Название старта, например «Весенний полумарафон»" value={g.title} onChange={(e) => setG({ ...g, title: e.target.value })} />
            <div className="row" style={{ gap: 6, flexWrap: 'wrap' }}>
              {DIST.map(([d, n]) => <button key={d} type="button" className="pill" aria-pressed={g.distanceKm === d} onClick={() => setG({ ...g, distanceKm: d })}>{n}</button>)}
              <input className="input" style={{ width: 90 }} type="number" step={0.1} value={g.distanceKm ?? ''} onChange={(e) => setG({ ...g, distanceKm: Number(e.target.value) })} /><span className="muted small">км</span>
            </div>
            <div className="row" style={{ gap: 10, flexWrap: 'wrap' }}>
              <label className="stack small" style={{ gap: 4 }}>Желаемое время<input className="input" placeholder="1:45:00" value={g.target} onChange={(e) => setG({ ...g, target: e.target.value })} /></label>
              <label className="stack small" style={{ gap: 4 }}>Дата старта<input className="input" type="date" value={g.date} onChange={(e) => setG({ ...g, date: e.target.value })} /></label>
            </div>
            <div className="row" style={{ gap: 10 }}><button type="button" className="btn" onClick={save}>Сохранить цель</button>{profile.goalRace && <button type="button" className="btn2" onClick={() => setEdit(false)}>Отмена</button>}</div>
            {e2 && <div className="note-err">{e2}</div>}
          </div>
        ) : (
          <>
            <h2 className="coach-title">{g0.title || `${g0.distanceKm} км`} · {g0.target || 'без целевого времени'}</h2>
            <span className="muted small">{g0.distanceKm} км{g0.date ? ` · ${new Date(`${g0.date}T12:00:00`).toLocaleDateString('ru-RU', { day: 'numeric', month: 'long', year: 'numeric' })}` : ''}{st.daysLeft != null ? ` · осталось ${st.daysLeft} дн.` : ''}</span>
          </>
        )}
      </section>
      {!edit && (
        <div className="grid2">
          <section className="card" aria-label="Прогноз">
            <span className="label">Прогноз по твоим тренировкам</span>
            {st.predText ? (
              <>
                <div className="row" style={{ gap: 18, alignItems: 'baseline', flexWrap: 'wrap' }}>
                  <span style={{ fontFamily: 'var(--display)', fontSize: 34 }}>{st.predText}</span>
                  {st.gapS != null && <span className="small" style={{ color: st.gapS <= 0 ? '#3DDC97' : '#F5B642' }}>{st.gapS <= 0 ? `быстрее цели на ${fmtT(-st.gapS)}` : `до цели ${fmtT(st.gapS)}`}</span>}
                </div>
                <span className="muted tiny">По лучшему отрезку за 6 недель: {((st.fromDistanceM ?? 0) / 1000).toFixed(1).replace('.', ',')} км за {fmtT(st.fromTimeS ?? 0)} ({st.fromDate ? ruDate(st.fromDate) : ''}), формула Ригеля.</span>
                {st.trend.length >= 2 && <LineBand pts={st.trend} band={st.targetS ? [st.targetS, st.targetS, st.targetS] : null} color="#9DB8FF" invertGood fmt={fmtT} />}
                {st.verdict && <span className="small" style={{ lineHeight: 1.5 }}>{st.verdict}</span>}
              </>
            ) : <span className="muted small">Нужна хотя бы одна пробежка от 5 км за последние 6 недель.</span>}
          </section>
          <section className="card" aria-label="Контрольные тесты">
            <span className="label">Контрольные тесты 5 км</span>
            <span className="muted small" style={{ lineHeight: 1.5 }}>Раз в 4 недели (3-я неделя блока) план ставит тест вместо одной темповой — и здесь видно, как меняется результат.</span>
            {st.nextTest && <span className="small">Следующий тест: <b>{new Date(`${st.nextTest}T12:00:00`).toLocaleDateString('ru-RU', { weekday: 'long', day: 'numeric', month: 'long' })}</b></span>}
            {st.tests.length === 0 && <span className="muted small">Тестов ещё не было.</span>}
            {st.tests.map((t, i) => {
              const prev = st.tests.slice(0, i).reverse().find((x) => x.time != null);
              const d = t.time != null && prev?.time != null ? t.time - prev.time : null;
              return (
                <div key={t.date} className="list-row small">
                  <span>{ruDate(t.date)}</span>
                  <b className="num">{t.timeText ?? 'нет записи'}</b>
                  {d != null && <span style={{ color: d <= 0 ? '#3DDC97' : '#FF6B6B' }}>{d <= 0 ? '−' : '+'}{fmtT(Math.abs(d))}</span>}
                </div>
              );
            })}
          </section>
        </div>
      )}
    </div>
  );
}
