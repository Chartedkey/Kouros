// «План» in the coach tab (0.15): proposals to accept, the coming days with ‹ ›, the workout of
// the day with warm-up and drills, manual edits, the weekly template and the history of changes.
import { useCallback, useEffect, useState } from 'react';
import { api, errText, type PlanChange, type PlanDay, type PlanItem, type PlanState } from '../lib/api';
import { useDataChanged } from '../lib/events';

const STATUS: Record<string, [string, string]> = {
  done: ['сделано', '#3DDC97'], partial: ['частично', '#F5B642'], missed: ['пропущено', '#FF6B6B'], planned: ['', '#9AA3AE'], rest: ['отдых', '#4A6A8A'],
};
const ORIGIN: Record<string, string> = { kouros: 'Kouros', claude: 'Claude', manual: 'вручную' };
const WD = ['Пн', 'Вт', 'Ср', 'Чт', 'Пт', 'Сб', 'Вс'];
const reasonText = (r: string) => r.replace(/\s*\[[a-z]+:[^\]]*\]/g, '');

function ItemsEditor({ items, kinds, onSave, onCancel, saveLabel }: { items: PlanItem[]; kinds: PlanState['kinds']; onSave: (v: PlanItem[]) => void; onCancel: () => void; saveLabel?: string }) {
  const [v, setV] = useState<PlanItem[]>(items.filter((i) => i.kind !== 'rest'));
  const upd = (k: number, x: Partial<PlanItem>) => setV(v.map((i, j) => (j === k ? { ...i, ...x } : i)));
  return (
    <div className="stack" style={{ gap: 8 }}>
      {v.length === 0 && <span className="muted small">Отдых. Добавь занятие, если хочешь тренироваться.</span>}
      {v.map((i, k) => (
        <div key={k} className="row" style={{ gap: 8, flexWrap: 'wrap' }}>
          <select className="select" value={i.kind} onChange={(e) => upd(k, { kind: e.target.value })}>
            {kinds.filter((x) => x.key !== 'rest').map((x) => <option key={x.key} value={x.key}>{x.title}</option>)}
          </select>
          <input className="input" style={{ width: 90 }} type="number" min={5} max={300} step={5} value={i.min} onChange={(e) => upd(k, { min: Number(e.target.value) || 0 })} />
          <span className="muted small">мин</span>
          <button type="button" className="xbtn" aria-label="Убрать" onClick={() => setV(v.filter((_, j) => j !== k))}>×</button>
        </div>
      ))}
      <div className="row" style={{ gap: 10, flexWrap: 'wrap' }}>
        <button type="button" className="linkbtn small" onClick={() => setV([...v, { kind: 'easy', min: 45 }])}>+ занятие</button>
        <span style={{ flex: 1 }} />
        <button type="button" className="btn" onClick={() => onSave(v.length ? v : [{ kind: 'rest', min: 0 }])}>{saveLabel ?? 'Сохранить'}</button>
        <button type="button" className="btn2" onClick={onCancel}>Отмена</button>
      </div>
    </div>
  );
}

function Proposals({ list, onDecide }: { list: PlanChange[]; onDecide: () => void }) {
  const [copied, setCopied] = useState<number | null>(null);
  const [err, setErr] = useState<string | null>(null);
  if (!list.length) return null;
  const groups = new Map<number, PlanChange[]>();
  for (const c of list) { const g = c.group ?? c.id; groups.set(g, [...(groups.get(g) ?? []), c]); }
  const decide = (id: number, accept: boolean) => api.planDecide(id, accept).then(onDecide).catch((e) => setErr(errText(e)));
  const discuss = async (cs: PlanChange[]) => {
    const q = `Kouros предлагает изменить план: ${cs.map((c) => `${c.dateRu}: ${c.beforeText} → ${c.afterText} (${reasonText(c.reason)})`).join('; ')}. Стоит ли принять? Если лучше по-другому — предложи строками [ПЛАН].`;
    try { const t = await api.coachPackage(null, q); await navigator.clipboard.writeText(t); setCopied(cs[0].id); } catch (e) { setErr(errText(e)); }
  };
  return (
    <section className="card" aria-label="Предложения по плану" style={{ borderColor: '#4a3a1a' }}>
      <span className="label" style={{ color: '#F5B642' }}>Тренер предлагает изменить план</span>
      {[...groups.values()].map((cs) => (
        <div key={cs[0].id} className="inset" style={{ gap: 8 }}>
          {cs.map((c) => (
            <div key={c.id} className="stack" style={{ gap: 2 }}>
              <span className="small"><b>{c.dateRu}:</b> <span className="muted" style={{ textDecoration: 'line-through' }}>{c.beforeText}</span> → <b>{c.afterText}</b></span>
              <span className="muted tiny">{ORIGIN[c.origin]}: {reasonText(c.reason)}</span>
            </div>
          ))}
          <div className="row" style={{ gap: 10, flexWrap: 'wrap' }}>
            <button type="button" className="btn btn-green" onClick={() => decide(cs[0].id, true)}>Принять</button>
            <button type="button" className="btn2" onClick={() => decide(cs[0].id, false)}>Оставить как было</button>
            <button type="button" className="linkbtn small" onClick={() => discuss(cs)}>Обсудить с Claude</button>
            {copied === cs[0].id && <span className="small" style={{ color: '#3DDC97' }}>Скопировано — вставь в чат «Тренер Kouros», ответ сохрани во вкладке «Сегодня у тренера».</span>}
          </div>
        </div>
      ))}
      {err && <div className="note-err">{err}</div>}
    </section>
  );
}

function DayCell({ d, sel, today, onPick }: { d: PlanDay; sel: boolean; today: string; onPick: () => void }) {
  const [word, color] = STATUS[d.status];
  const min = d.items.reduce((a, i) => a + (i.kind === 'rest' ? 0 : i.min), 0);
  return (
    <button type="button" className={`wday${d.date === today ? ' today' : ''}`} onClick={onPick}
      style={{ cursor: 'pointer', textAlign: 'left', border: sel ? '1px solid #f1f3f5' : undefined, background: d.deload ? '#151b22' : undefined, alignItems: 'flex-start' }}>
      <span className="tiny" style={{ fontWeight: 700 }}>{d.wd} {Number(d.date.slice(8))}{d.date === today ? ' · сегодня' : ''}</span>
      <span className="row" style={{ gap: 6 }}><span className="dot" style={{ background: d.hard ? '#FF8A65' : d.items.some((i) => i.kind !== 'rest') ? '#3DDC97' : '#3A404A' }} /><span className="tiny" style={{ lineHeight: 1.3, color: 'var(--text-2)' }}>{d.items.filter((i) => i.kind !== 'rest').map((i) => i.kind === 'easy' ? 'бег' : i.kind === 'long' ? 'длительный' : i.kind === 'tempo' ? 'темп' : i.kind === 'intervals' ? 'интервалы' : i.kind === 'gym' ? 'зал' : i.kind === 'ofp' ? 'ОФП' : i.kind === 'bike' ? 'вело' : i.kind === 'walk' ? 'прогулка' : i.kind === 'upper' ? 'верх тела' : i.kind === 'walkrun' ? 'бег+ходьба' : i.kind === 'test' ? 'тест 5 км' : 'мобилити').join(' + ') || 'отдых'}</span></span>
      {min > 0 && <span className="muted tiny">{min} мин</span>}
      {word && <span className="tiny" style={{ color }}>{word}</span>}
      {d.source && d.source !== 'template' && <span className="tiny" style={{ color: '#9DB8FF' }}>изменено</span>}
    </button>
  );
}

function DayView({ d, st, onChanged }: { d: PlanDay; st: PlanState; onChanged: () => void }) {
  const [edit, setEdit] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => setEdit(false), [d.date]);
  const past = d.date < st.today;
  return (
    <section className="card" aria-label={`План на ${d.dateRu}`}>
      <div className="between" style={{ flexWrap: 'wrap' }}>
        <span className="label">{d.date === st.today ? 'Тренировка дня' : d.dateRu} · {d.deload ? 'разгрузочная неделя' : `неделя ${d.week} из 4`}</span>
        {!past && !edit && (
          <span className="row" style={{ gap: 12 }}>
            <button type="button" className="linkbtn small" onClick={() => setEdit(true)}>Изменить день</button>
            {d.source && d.source !== 'template' && <button type="button" className="linkbtn small" onClick={() => api.planResetDay(d.date).then(onChanged).catch((e) => setErr(errText(e)))}>Вернуть по шаблону</button>}
          </span>
        )}
      </div>
      <h3 style={{ margin: 0, fontFamily: 'var(--display)', fontWeight: 500, fontSize: 22 }}>{d.text}</h3>
      {d.doneActs.length > 0 && <span className="small" style={{ color: '#3DDC97' }}>Сделано: {d.doneActs.map((a) => `${a.sport} ${a.min} мин (нагрузка ${a.load})${a.unplanned ? ' — вне плана' : ''}`).join(', ')}</span>}
      {d.status === 'missed' && <span className="small" style={{ color: '#FF6B6B' }}>Тренировки в этот день в Strava нет.</span>}
      {d.note && <span className="muted small" style={{ lineHeight: 1.5 }}>{d.note}</span>}
      {edit ? (
        <ItemsEditor items={d.items} kinds={st.kinds} onCancel={() => setEdit(false)} onSave={(v) => api.planSetDay(d.date, v).then(() => { setEdit(false); onChanged(); }).catch((e) => setErr(errText(e)))} />
      ) : (
        d.detail.map((x) => (
          <div key={x.kind} className="inset" style={{ gap: 6 }}>
            <b className="small">{x.title} · {x.min} мин</b>
            {x.warmup && <span className="small" style={{ lineHeight: 1.5 }}><span className="muted">Разминка: </span>{x.warmup}</span>}
            {x.drills && <span className="small" style={{ lineHeight: 1.5 }}><span className="muted">Упражнения: </span>{x.drills}</span>}
            <span className="small" style={{ lineHeight: 1.5 }}><span className="muted">Основная часть: </span>{x.main}</span>
            {x.cooldown && <span className="small" style={{ lineHeight: 1.5 }}><span className="muted">Заминка: </span>{x.cooldown}</span>}
            {x.notes.map((n) => <span key={n} className="muted tiny" style={{ lineHeight: 1.5 }}>{n}</span>)}
          </div>
        ))
      )}
      {!edit && d.detail.length === 0 && <span className="muted small">День отдыха: прогулка 30–40 минут и немного мобилити — по желанию.</span>}
      {!edit && d.hard && d.date >= st.today && <span className="muted tiny">Интенсивная тренировка состоится, только если в этот день позволят готовность и проверки безопасности — иначе Kouros предложит замену.</span>}
      <span className="muted tiny">Прогулки каждый день идут сверх плана и всегда считаются в нагрузку.</span>
      {err && <div className="note-err">{err}</div>}
    </section>
  );
}

function Template({ st, onChanged }: { st: PlanState; onChanged: () => void }) {
  const [edit, setEdit] = useState<number | null>(null);
  const [days, setDays] = useState<PlanItem[][]>(st.template);
  const [dirty, setDirty] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => { setDays(st.template); setDirty(false); }, [st.template]);
  const text = (v: PlanItem[]) => v.filter((i) => i.kind !== 'rest').map((i) => `${st.kinds.find((k) => k.key === i.kind)?.title ?? i.kind} ${i.min}`).join(' + ') || 'отдых';
  return (
    <section className="card" aria-label="Шаблон недели">
      <div className="between" style={{ flexWrap: 'wrap' }}>
        <span className="label">Шаблон обычной недели</span>
        <span className="muted tiny">{st.templateAuto ? 'собран из знакомства' : 'изменён вручную'}</span>
      </div>
      <div className="stack" style={{ gap: 0 }}>
        {days.map((d, k) => (
          <div key={k} className="list-row small" style={{ flexWrap: 'wrap' }}>
            <b style={{ width: 28 }}>{WD[k]}</b>
            {edit === k
              ? <div style={{ flex: '1 1 100%' }}><ItemsEditor items={d} kinds={st.kinds} saveLabel="Готово" onCancel={() => setEdit(null)} onSave={(v) => { setDays(days.map((x, j) => (j === k ? v : x))); setEdit(null); setDirty(true); }} /></div>
              : <><span style={{ flex: 1 }}>{text(d)}</span><button type="button" className="linkbtn tiny" onClick={() => setEdit(k)}>изменить</button></>}
          </div>
        ))}
      </div>
      <div className="row" style={{ gap: 10, flexWrap: 'wrap' }}>
        {dirty && <button type="button" className="btn" onClick={() => api.planTemplateSave(days, false).then(onChanged).catch((e) => setErr(errText(e)))}>Сохранить шаблон</button>}
        <button type="button" className="btn2" onClick={() => api.planTemplateSave([], true).then(onChanged).catch((e) => setErr(errText(e)))}>Собрать заново из знакомства</button>
      </div>
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>Новый шаблон применяется с завтрашнего дня; дни, которые ты менял или принял, остаются. Каждая 4-я неделя — разгрузочная, а объём недели не превышает безопасный запас нагрузки.</span>
      {err && <div className="note-err">{err}</div>}
    </section>
  );
}

function History({ list }: { list: PlanChange[] }) {
  const done = list.filter((c) => c.state !== 'proposed');
  if (!done.length) return null;
  return (
    <section className="card" aria-label="История изменений плана">
      <span className="label">История изменений плана</span>
      {done.slice(0, 30).map((c) => (
        <div key={c.id} className="list-row small" style={{ alignItems: 'flex-start' }}>
          <span style={{ flex: 1, lineHeight: 1.5 }}><b>{c.dateRu}:</b> {c.beforeText} → {c.afterText}<br /><span className="muted tiny">{ORIGIN[c.origin]}: {reasonText(c.reason)}</span></span>
          <span className={`chip ${c.state === 'accepted' ? 'chip-ok' : 'chip-off'}`}>{c.state === 'accepted' ? 'принято' : 'оставлено как было'}</span>
          <span className="muted tiny num">{new Date(c.ts * 1000).toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' })}</span>
        </div>
      ))}
    </section>
  );
}

export function CoachPlan() {
  const [st, setSt] = useState<PlanState | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [off, setOff] = useState(6); // index of the first shown day; 6 = today
  const [sel, setSel] = useState<string | null>(null);
  const load = useCallback(() => { api.planState().then((v) => { setSt(v); setErr(null); }).catch((e) => setErr(errText(e))); }, []);
  useEffect(load, [load]);
  useDataChanged(load);
  if (err) return <div className="note-err">{err}</div>;
  if (!st) return <p className="muted">Составляю план…</p>;
  const shown = st.days.slice(off, off + 7);
  const cur = st.days.find((d) => d.date === (sel ?? st.today)) ?? shown[0];
  const pct = st.week.cap ? Math.min(100, (st.week.plan / st.week.cap) * 100) : null;
  return (
    <div className="stack" style={{ gap: 16 }}>
      <Proposals list={st.proposals} onDecide={load} />
      <section className="card" aria-label="План на дни">
        <div className="between" style={{ flexWrap: 'wrap' }}>
          <span className="label">{st.phase.name} · блок {st.phase.block}, {st.phase.deload ? 'разгрузочная неделя' : `неделя ${st.phase.week} из 4`}</span>
          <span className="row" style={{ gap: 6 }}>
            <button type="button" className="btn2" style={{ width: 38, padding: 0 }} aria-label="Раньше" disabled={off <= 0} onClick={() => setOff(off - 1)}>‹</button>
            <button type="button" className="linkbtn small" onClick={() => { setOff(6); setSel(null); }}>сегодня</button>
            <button type="button" className="btn2" style={{ width: 38, padding: 0 }} aria-label="Позже" disabled={off >= st.days.length - 7} onClick={() => setOff(off + 1)}>›</button>
          </span>
        </div>
        <div className="week">{shown.map((d) => <DayCell key={d.date} d={d} today={st.today} sel={d.date === cur?.date} onPick={() => setSel(d.date)} />)}</div>
        {st.week.cap != null && (
          <div className="stack" style={{ gap: 4 }}>
            <div className="between tiny"><span className="muted">Нагрузка недели по плану</span><span className="num">{st.week.plan} из безопасных {st.week.cap}{st.week.usual ? ` · обычно ${st.week.usual}` : ''}</span></div>
            <div style={{ height: 6, borderRadius: 999, background: '#1c2026' }}><div style={{ height: 6, borderRadius: 999, width: `${pct}%`, background: (pct ?? 0) > 95 ? '#F5B642' : '#3DDC97' }} /></div>
          </div>
        )}
      </section>
      {cur && <DayView d={cur} st={st} onChanged={load} />}
      <div className="grid2">
        <Template st={st} onChanged={load} />
        <History list={st.changes} />
      </div>
    </div>
  );
}
