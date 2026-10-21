// «Откуда брать данные» (0.14): a source per data group, a warning before switching with two
// choices (from today / replace the history), and which metrics of the other source are used.
import { useCallback, useEffect, useState } from 'react';
import { api, errText, type SourceGroup, type SourcePreview, type SourcesState } from '../lib/api';
import { useDataChanged } from '../lib/events';

const ruDate = (d: string) => new Date(`${d}T12:00:00`).toLocaleDateString('ru-RU', { day: 'numeric', month: 'short', year: 'numeric' });

function History({ g }: { g: SourceGroup }) {
  if (g.segs.length < 2) return null;
  return (
    <span className="muted tiny" style={{ lineHeight: 1.5 }}>
      {g.segs.map((s, i) => `${i === 0 ? 'до' : `с ${ruDate(s.from)}`}${i === 0 && g.segs[1] ? ` ${ruDate(g.segs[1].from)}` : ''} — ${s.name}`).join(' · ')}
    </span>
  );
}

function Fields({ g, onChanged }: { g: SourceGroup; onChanged: () => void }) {
  const [err, setErr] = useState<string | null>(null);
  if (!g.fields.length) return null;
  const set = (canon: string, v: string) => {
    const metric = v === '__auto' ? null : v === '__off' ? '' : v;
    api.sourcesMapSet(g.current, canon, metric).then(onChanged).catch((e) => setErr(errText(e)));
  };
  return (
    <div className="stack" style={{ gap: 6 }}>
      <span className="muted tiny">Что берётся из {g.currentName}. Kouros подобрал показатели по названию — если что-то не то, выбери вручную.</span>
      {g.fields.map((f) => (
        <div key={f.canon} className="row" style={{ gap: 10, flexWrap: 'wrap' }}>
          <span className="small" style={{ minWidth: 150 }}>{f.label}</span>
          <select className="select" style={{ flex: '1 1 220px', maxWidth: 380 }} value={f.manual ? (f.metric ?? '__off') : f.metric == null ? '__off' : '__auto'} onChange={(e) => set(f.canon, e.target.value)}>
            <option value="__auto">{f.metric && !f.manual ? `авто: ${f.metric}` : 'авто — не найдено'}</option>
            <option value="__off">не брать</option>
            {f.candidates.map((c) => <option key={c.metric} value={c.metric}>{c.metric}{c.unit ? ` (${c.unit})` : ''} · {c.count}</option>)}
          </select>
          {f.metric && <span className="muted tiny">{f.count} зап.{f.last ? ` · по ${ruDate(f.last)}` : ''}</span>}
        </div>
      ))}
      {err && <div className="note-err">{err}</div>}
    </div>
  );
}

function Group({ g, onChanged }: { g: SourceGroup; onChanged: () => void }) {
  const [pick, setPick] = useState<string | null>(null);
  const [pv, setPv] = useState<SourcePreview | null>(null);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const choose = (p: string) => {
    if (p === g.current) { setPick(null); setPv(null); return; }
    setPick(p); setPv(null); setErr(null);
    api.sourcesPreview(g.key, p).then(setPv).catch((e) => setErr(errText(e)));
  };
  const apply = async (mode: 'keep' | 'replace') => {
    if (!pick) return;
    setBusy(true);
    try { await api.sourcesSet(g.key, pick, mode); setPick(null); setPv(null); onChanged(); } catch (e) { setErr(errText(e)); } finally { setBusy(false); }
  };
  const pickName = g.options.find((o) => o.provider === pick)?.name ?? pick;
  const multi = g.writers.length > 1;
  return (
    <div className="inset" style={{ gap: 8 }}>
      <div className="between" style={{ flexWrap: 'wrap', gap: 10 }}>
        <div className="stack" style={{ gap: 2 }}>
          <b className="small">{g.name}</b>
          <History g={g} />
        </div>
        <div className="row" style={{ gap: 6, flexWrap: 'wrap' }}>
          {g.options.map((o) => (
            <button key={o.provider} type="button" className="pill" aria-pressed={(pick ?? g.current) === o.provider} disabled={o.matched === 0} title={o.matched === 0 ? 'У этого источника таких данных нет' : undefined} onClick={() => choose(o.provider)}>
              {o.name}{o.of > 1 && o.provider !== 'APPLE_HEALTH' && o.matched < o.of ? ` · ${o.matched} из ${o.of}` : ''}
            </button>
          ))}
        </div>
      </div>
      {!g.open && <span className="muted tiny">Пока только Apple Health: пульс по минутам и стресс Kouros считает из сырых замеров, а их другие источники во freddy не отдают.</span>}
      {g.cut && <span className="tiny" style={{ color: '#F5B642' }}>Норма считается с {ruDate(g.cut)} — по данным {g.currentName}.</span>}
      {g.current === 'APPLE_HEALTH' && g.writers.length > 0 && (
        <span className="muted tiny" style={{ lineHeight: 1.5 }}>
          В Apple Health это пишут: {g.writers.map((w) => w.name).join(', ')}.{multi ? ' Несколько устройств — если одно из них часы другой марки (например, через Garmin Connect), лучше выбрать его источник напрямую.' : ''}
        </span>
      )}
      {pick && (
        <div className="note-warn" style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          <span><b>{g.name}: {g.currentName} → {pickName}.</b> {pv?.text ?? '…'}</span>
          <span className="small">Старые данные никуда не денутся — к прежнему источнику можно вернуться в любой момент.</span>
          <div className="row" style={{ gap: 10, flexWrap: 'wrap' }}>
            <button type="button" className="btn" disabled={busy} onClick={() => apply('keep')}>С сегодняшнего дня</button>
            <button type="button" className="btn2" disabled={busy} onClick={() => apply('replace')}>
              Заменить и историю{pv?.historyFrom ? ` (есть с ${ruDate(pv.historyFrom)})` : ''}
            </button>
            <button type="button" className="linkbtn small" onClick={() => { setPick(null); setPv(null); }}>Отмена</button>
          </div>
          <span className="tiny" style={{ color: '#c9b78f', lineHeight: 1.5 }}>
            «С сегодняшнего дня» — прошлые дни остаются как были, на графиках будет отметка смены, а норма первые 2 недели набирается заново. «Заменить и историю» — Kouros пересчитает прошлое по новому источнику там, где у него есть данные, и норма будет сразу.
          </span>
        </div>
      )}
      {g.fields.length > 0 && !pick && (
        <>
          <button type="button" className="linkbtn tiny" style={{ alignSelf: 'flex-start' }} onClick={() => setOpen(!open)}>{open ? 'скрыть показатели' : 'какие показатели берутся'}</button>
          {open && <Fields g={g} onChanged={onChanged} />}
        </>
      )}
      {err && <div className="note-err">{err}</div>}
    </div>
  );
}

export function DataSourcesCard() {
  const [st, setSt] = useState<SourcesState | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(() => { api.sourcesState().then(setSt).catch((e) => setErr(errText(e))); }, []);
  useEffect(load, [load]);
  useDataChanged(load);
  if (err) return <div className="note-err">{err}</div>;
  if (!st) return null;
  const missing = st.known.filter((k) => !k.connected);
  return (
    <section className="card" aria-label="Откуда брать данные">
      <div className="between" style={{ flexWrap: 'wrap' }}>
        <span className="label">Откуда брать данные</span>
        <span className="muted tiny">Тренировки — в карточке «Откуда брать тренировки» ниже</span>
      </div>
      <span className="muted small" style={{ lineHeight: 1.5 }}>
        Kouros получает через freddy всё, что там подключено, и хранит данные всех источников. Здесь выбираешь, какой из них показывать и считать — для каждого вида данных отдельно. Еда, вода, кофеин и алкоголь — из Apple Health (YAZIO, Waterllama).
      </span>
      {st.providers.length > 0 && (
        <span className="small">Подключено во freddy, кроме Apple Health: {st.providers.map((p) => `${p.name} (${p.metrics} показ.${p.first ? `, с ${ruDate(p.first)}` : ''})`).join(', ')}.</span>
      )}
      {missing.length > 0 && (
        <span className="muted tiny" style={{ lineHeight: 1.5 }}>
          Ещё не подключено: {missing.map((m) => m.name).join(', ')}. Подключи их на freddy.coach (раздел источников) — после следующей загрузки они появятся здесь, Kouros заберёт и их историю.
        </span>
      )}
      {st.groups.map((g) => <Group key={g.key} g={g} onChanged={load} />)}
    </section>
  );
}
