// «Как прошла тренировка?» after a workout, and the morning «как сейчас?» question for a painful
// place (it decides the next return-to-run step).
import { useEffect, useState } from 'react';
import { api, errText, sportOf, type BodyState, type PendingCheckin, type Shoe } from '../lib/api';
import { PainFigure, painColor, sideText } from './PainFigure';

const RPE_HINT: Record<number, string> = { 1: 'очень легко', 3: 'легко', 5: 'средне', 7: 'тяжело', 9: 'очень тяжело', 10: 'на пределе' };
const FEEL = ['плохо', 'так себе', 'нормально', 'хорошо', 'отлично'];

export function LevelPills({ value, onChange }: { value: number | null; onChange: (v: number) => void }) {
  return (
    <div className="row" style={{ gap: 4, flexWrap: 'wrap' }}>
      {Array.from({ length: 11 }, (_, i) => (
        <button key={i} type="button" className="pill" aria-pressed={value === i} onClick={() => onChange(i)}
          style={{ width: 38, padding: 0, justifyContent: 'center', borderColor: value === i ? painColor(i === 0 ? 0 : i) : undefined }}>{i}</button>
      ))}
    </div>
  );
}

export function CheckinCard({ a, areas, suggest, onDone }: { a: PendingCheckin; areas: BodyState['areas']; suggest?: string | null; onDone: () => void }) {
  const [shoes, setShoes] = useState<Shoe[]>([]);
  const [shoe, setShoe] = useState<string | null>(a.shoeId ?? null);
  useEffect(() => {
    if (!a.askShoe) return;
    api.shoesGet().then((l) => {
      const act = l.filter((x) => !x.retired && (a.sport === 'run' || x.walks));
      setShoes(act);
      // Preselect: already chosen → today's advice → the main pair → the only pair.
      setShoe((cur) => cur ?? (act.find((x) => x.id === suggest)?.id ?? act.find((x) => x.main)?.id ?? (act.length === 1 ? act[0].id : null)));
    }).catch(() => {});
  }, [a.askShoe, a.sport, suggest]);
  const [rpe, setRpe] = useState<number | null>(a.rpe);
  const [feel, setFeel] = useState<number | null>(null);
  const [hurt, setHurt] = useState(false);
  const [pains, setPains] = useState<Record<string, number>>({});
  const [sel, setSel] = useState<string | null>(null);
  const [note, setNote] = useState('');
  const [err, setErr] = useState<string | null>(null);
  const sp = sportOf(a.sport);
  const when = new Date(a.start * 1000).toLocaleString('ru-RU', { weekday: 'short', hour: '2-digit', minute: '2-digit' });
  const name = (k: string) => { const [ar, sd] = k.split(':'); return `${areas.find((x) => x.key === ar)?.name ?? ar}${sd ? ` ${sideText(sd)}` : ''}`; };
  const save = async (skip: boolean) => {
    setErr(null);
    try {
      await api.checkinSave(a.id, skip ? { skip: true } : {
        shoe: shoe ?? undefined, rpe, feel, note, pain: (Object.entries(pains) as [string, number][]).filter(([, l]) => l > 0).map(([k, level]) => { const [area, side] = k.split(':'); return { area, side, level }; }),
      });
      onDone();
    } catch (e) { setErr(errText(e)); }
  };
  return (
    <section className="card" aria-label="Как прошла тренировка">
      <div className="between"><span className="label">Как прошла тренировка?</span><span className="muted tiny">{when}</span></div>
      <b><span style={{ color: sp.color }}>{sp.name}</span> · {a.name || sp.name} · {a.min} мин{a.km ? ` · ${String(a.km).replace('.', ',')} км` : ''}</b>
      {shoes.length > 0 && (
        <div className="stack" style={{ gap: 6 }}>
          <span className="small">В каких кроссовках {a.sport === 'run' ? 'бегал' : 'ходил'}?</span>
          <div className="row" style={{ gap: 6, flexWrap: 'wrap' }}>
            {shoes.map((x) => <button key={x.id} type="button" className="pill" aria-pressed={shoe === x.id} onClick={() => setShoe(x.id)}>{x.name}</button>)}
            <button type="button" className="pill" aria-pressed={shoe === 'none'} onClick={() => setShoe('none')}>в других</button>
          </div>
          {shoe && shoe !== 'none' && suggest === shoe && !a.shoeId && <span className="muted tiny">Выбрана пара, которую Kouros советовал на сегодня — поменяй, если бегал в другой.</span>}
        </div>
      )}
      <div className="stack" style={{ gap: 6 }}>
        <span className="small">Насколько было тяжело (RPE){rpe ? ` — ${RPE_HINT[rpe] ?? RPE_HINT[rpe - 1] ?? ''}` : ''}</span>
        <div className="row" style={{ gap: 4, flexWrap: 'wrap' }}>
          {Array.from({ length: 10 }, (_, i) => i + 1).map((v) => <button key={v} type="button" className="pill" aria-pressed={rpe === v} onClick={() => setRpe(v)} style={{ width: 38, padding: 0, justifyContent: 'center' }}>{v}</button>)}
        </div>
      </div>
      <div className="stack" style={{ gap: 6 }}>
        <span className="small">Как ощущения</span>
        <div className="row" style={{ gap: 6, flexWrap: 'wrap' }}>{FEEL.map((f, i) => <button key={f} type="button" className="pill" aria-pressed={feel === i + 1} onClick={() => setFeel(i + 1)}>{f}</button>)}</div>
      </div>
      <div className="row" style={{ gap: 8 }}>
        <span className="small">Что-то болело?</span>
        <button type="button" className="pill" aria-pressed={!hurt} onClick={() => { setHurt(false); setPains({}); setSel(null); }}>Нет</button>
        <button type="button" className="pill" aria-pressed={hurt} onClick={() => setHurt(true)}>Да</button>
      </div>
      {hurt && (
        <div className="stack" style={{ gap: 8 }}>
          <span className="muted tiny">Нажми на место на фигуре и выбери, насколько болело (0 — нет, 10 — очень сильно).</span>
          <PainFigure levels={pains} sel={sel} onPick={(ar, sd) => { const k = `${ar}:${sd}`; setSel(k); if (pains[k] == null) setPains({ ...pains, [k]: 3 }); }} />
          {sel && (
            <div className="stack" style={{ gap: 6 }}>
              <span className="small"><b>{name(sel)}</b></span>
              <LevelPills value={pains[sel] ?? null} onChange={(v) => setPains({ ...pains, [sel]: v })} />
            </div>
          )}
          {Object.keys(pains).filter((k) => pains[k] > 0).length > 0 && <span className="tiny muted">Отмечено: {(Object.entries(pains) as [string, number][]).filter(([, l]) => l > 0).map(([k, l]) => `${name(k)} ${l}/10`).join(', ')}</span>}
        </div>
      )}
      <input className="input" placeholder="Заметка (необязательно)" value={note} onChange={(e) => setNote(e.target.value)} />
      <div className="row" style={{ gap: 10 }}>
        <button type="button" className="btn btn-green" onClick={() => save(false)}>Сохранить</button>
        <button type="button" className="linkbtn small" onClick={() => save(true)}>Не сейчас</button>
      </div>
      <span className="muted tiny">RPE уточняет нагрузку силовых и йоги, отметки боли идут в дневник — тренер учтёт их в плане{shoes.length ? ', а пробег засчитается выбранной паре' : ''}.</span>
      {err && <div className="note-err">{err}</div>}
    </section>
  );
}

/** Morning question about each painful place (once a day). */
export function MorningPain({ b, today, onDone }: { b: BodyState; today: string; onDone: () => void }) {
  const ask = b.pains.filter((p) => (p.active || (b.return.active && p.area === b.return.area)) && !p.entries.some((e) => e.date === today && e.context === 'morning'));
  const [vals, setVals] = useState<Record<string, number>>({});
  if (!ask.length) return null;
  const save = (p: (typeof ask)[number], v: number) => {
    setVals({ ...vals, [`${p.area}:${p.side}`]: v });
    api.painMark({ area: p.area, side: p.side, level: v, context: 'morning' }).then(onDone).catch(() => {});
  };
  return (
    <section className="card" aria-label="Как сегодня болит">
      <span className="label">Как сегодня?</span>
      {ask.map((p) => (
        <div key={`${p.area}:${p.side}`} className="stack" style={{ gap: 6 }}>
          <span className="small"><b>{p.name}</b> <span className="muted">— вчера/последний раз {p.level}/10</span></span>
          <LevelPills value={vals[`${p.area}:${p.side}`] ?? null} onChange={(v) => save(p, v)} />
        </div>
      ))}
      {b.return.active && <span className="muted tiny">От утреннего ответа зависит следующий шаг возврата к бегу: 0–3 — дальше, 4–5 — повторяем, 6+ — шаг назад.</span>}
    </section>
  );
}
