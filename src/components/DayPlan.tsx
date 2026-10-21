// «Тренировка на сегодня» (0.23): confirm the planned workout or pick another one (several kinds,
// with minutes). A risky choice gets one warning with the reason and a lighter option; the last
// word is the athlete's. After a choice the rest of the week is rebalanced by Kouros.
import { useState } from 'react';
import { api, errText, type ChoiceCheck, type ChoiceResult, type PlanItem, type PlanState } from '../lib/api';

const ST: Record<string, [string, string]> = { done: ['выполнено', '#3DDC97'], partial: ['частично', '#F5B642'], missed: ['пропущено', '#FF6B6B'] };

export function DayPlanCard({ plan, reload, onCoach }: { plan: PlanState; reload: () => void; onCoach?: () => void }) {
  const day = plan.days.find((d) => d.date === plan.today);
  const [mode, setMode] = useState<'view' | 'pick' | 'check'>('view');
  const [picked, setPicked] = useState<Record<string, string>>({});
  const [check, setCheck] = useState<ChoiceCheck | null>(null);
  const [result, setResult] = useState<ChoiceResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  if (!day) return null;
  const props = plan.proposals.filter((p) => p.date === plan.today);
  const title = (k: string) => plan.kinds.find((x) => x.key === k)?.title ?? k;
  const items = (): PlanItem[] => Object.entries(picked).map(([kind, m]) => ({ kind, min: kind === 'rest' ? 0 : Math.max(5, Number(m.replace(',', '.')) || 0) }));

  const startPick = () => {
    const p: Record<string, string> = {};
    for (const i of day.items) p[i.kind] = String(Math.round(i.min));
    setPicked(p); setCheck(null); setResult(null); setErr(null); setMode('pick');
  };
  const toggle = (k: string) => {
    const p = { ...picked };
    if (p[k] != null) delete p[k];
    else if (k === 'rest') { for (const x of Object.keys(p)) delete p[x]; p.rest = '0'; }
    else { delete p.rest; p[k] = String(day.items.find((i) => i.kind === k)?.min ?? (k === 'long' ? 90 : k === 'walk' ? 60 : k === 'mobility' ? 20 : 45)); }
    setPicked(p);
  };
  const doCheck = async () => {
    const it = items();
    if (!it.length) { setErr('Выбери хотя бы одно занятие или отдых'); return; }
    setBusy(true); setErr(null);
    try {
      const c = await api.planCheckChoice(day.date, it);
      if (c.ok) await save(it);
      else { setCheck(c); setMode('check'); }
    } catch (e) { setErr(errText(e)); } finally { setBusy(false); }
  };
  const save = async (it: PlanItem[]) => {
    setBusy(true);
    try { setResult(await api.planChoose(day.date, it)); setMode('view'); setCheck(null); reload(); } catch (e) { setErr(errText(e)); } finally { setBusy(false); }
  };
  const confirm = async () => {
    try { await api.planConfirm(day.date); reload(); } catch (e) { setErr(errText(e)); }
  };
  const st = ST[day.status];
  return (
    <section className="card" aria-label="Тренировка на сегодня">
      <div className="between">
        <span className="label">Тренировка на сегодня · по плану</span>
        {onCoach && <button type="button" className="linkbtn small" onClick={onCoach}>План недели →</button>}
      </div>
      <div className="row" style={{ gap: 10, alignItems: 'baseline' }}>
        <span style={{ fontFamily: 'var(--display)', fontSize: 20, fontWeight: 700 }}>{day.text}</span>
        {st && <span className="chip" style={{ color: st[1] }}>{st[0]}</span>}
        {day.source === 'chosen' && <span className="muted tiny">твой выбор</span>}
        {day.source === 'accepted' && <span className="muted tiny">изменено Kouros</span>}
      </div>
      {day.detail[0] && mode === 'view' && <span className="muted small" style={{ lineHeight: 1.5 }}>{day.detail[0].main}</span>}

      {props.map((p) => (
        <div key={p.id} className="inset" style={{ gap: 6 }}>
          <span className="small"><b>Kouros предлагает:</b> {p.beforeText} → <b>{p.afterText}</b></span>
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>{p.reason.replace(/ \[[^\]]*\]/g, '')}</span>
          <div className="row" style={{ gap: 8 }}>
            <button type="button" className="btn2" onClick={() => api.planDecide(p.id, true).then(reload).catch((e) => setErr(errText(e)))}>Принять</button>
            <button type="button" className="btn2" onClick={() => api.planDecide(p.id, false).then(reload).catch((e) => setErr(errText(e)))}>Оставить как было</button>
          </div>
        </div>
      ))}

      {mode === 'view' && day.status !== 'done' && (
        <div className="row" style={{ gap: 8 }}>
          {day.confirmed
            ? <span className="small" style={{ color: '#3DDC97' }}>✓ Подтверждено</span>
            : <button type="button" className="btn btn-green" onClick={confirm}>Подтверждаю</button>}
          <button type="button" className="btn2" onClick={startPick}>Выбрать другое</button>
        </div>
      )}

      {mode === 'pick' && (
        <div className="stack" style={{ gap: 10 }}>
          <span className="small">Что сделаешь сегодня? Можно несколько — например, ходьба и силовая.</span>
          <div className="row" style={{ gap: 6 }}>
            {plan.kinds.map((k) => <button key={k.key} type="button" className="pill" aria-pressed={picked[k.key] != null} onClick={() => toggle(k.key)}>{k.title}</button>)}
          </div>
          {Object.keys(picked).filter((k) => k !== 'rest').map((k) => (
            <label key={k} className="row small" style={{ gap: 8 }}>
              <span style={{ minWidth: 200 }}>{title(k)}</span>
              <input className="input sm" style={{ width: 70 }} inputMode="numeric" value={picked[k]} onChange={(e) => setPicked({ ...picked, [k]: e.target.value })} aria-label={`${title(k)}, минут`} />
              <span className="muted">мин</span>
            </label>
          ))}
          <div className="row" style={{ gap: 8 }}>
            <button type="button" className="btn" disabled={busy || !Object.keys(picked).length} onClick={doCheck}>{busy ? 'Проверяю…' : 'Сохранить'}</button>
            <button type="button" className="btn2" onClick={() => setMode('view')}>Отмена</button>
          </div>
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>Kouros проверит выбор по готовности, нагрузке недели, вчерашнему дню и боли. Если рискованно — предупредит, но решение за тобой. После выбора он сам пересчитает остальные дни недели.</span>
        </div>
      )}

      {mode === 'check' && check && (
        <div className="stack" style={{ gap: 8 }}>
          <div className="inset" style={{ gap: 6, background: '#F5B64214', border: '1px solid #F5B64244' }}>
            <b className="small" style={{ color: '#F5B642' }}>Обрати внимание</b>
            {check.warnings.map((w) => <span key={w} className="small" style={{ lineHeight: 1.5 }}>{w}</span>)}
            {check.suggestionText && <span className="small" style={{ lineHeight: 1.5 }}>Вариант полегче: <b>{check.suggestionText}</b>.</span>}
          </div>
          <div className="row" style={{ gap: 8 }}>
            {check.suggestion && <button type="button" className="btn btn-green" disabled={busy} onClick={() => save(check.suggestion!)}>Взять вариант полегче</button>}
            <button type="button" className="btn2" disabled={busy} onClick={() => save(items())}>Оставить мой выбор</button>
            <button type="button" className="linkbtn small" onClick={() => setMode('pick')}>Назад</button>
          </div>
        </div>
      )}

      {result && result.changes.length > 0 && (
        <div className="stack" style={{ gap: 4 }}>
          <span className="small"><b>Kouros пересчитал неделю:</b></span>
          {result.changes.map((c) => <span key={c.date} className="small" style={{ lineHeight: 1.5 }}>{c.dateRu}: {c.before} → <b>{c.after}</b> <span className="muted">— {c.why}</span></span>)}
          <span className="muted tiny">Любой день можно поменять обратно в «Тренер → План».</span>
        </div>
      )}
      {result && result.changes.length === 0 && <span className="small" style={{ color: '#3DDC97' }}>Сохранено. Остальная неделя не требует изменений.</span>}
      {err && <div className="note-err">{err}</div>}
    </section>
  );
}
