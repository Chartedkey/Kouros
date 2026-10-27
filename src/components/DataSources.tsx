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
      {!g.open && g.key !== 'other' && <span className="muted tiny">Пульс по минутам: из него Kouros считает стресс днём, ночной пульс покоя и ночной HRV. Такие замеры отдают Apple Health и Garmin — у Garmin пульс каждые 15 секунд, шаги по 15 минут. Ночной пульс покоя по Garmin считается, если и «Пульс покоя» стоит на Garmin.</span>}
      {g.key === 'other' && <span className="muted tiny">Еда YAZIO, вода и алкоголь Waterllama, кофеин, беговая динамика и походка — всё, что не вошло в группы выше.</span>}
      {g.current === 'MANUAL' && <span className="muted tiny" style={{ lineHeight: 1.5 }}>Значения вводишь сам: каждое утро на «Сегодня» появится карточка{g.key === 'hrv' ? ' для HRV (RMSSD или SDNN — как показывает твоё приложение; у Fitbit / Google Health, Garmin, Oura и Whoop это RMSSD, у Apple Watch — SDNN)' : ' для дыхания и SpO₂'}. Своя норма набирается по ним около 2 недель.</span>}
      {g.cut && <span className="tiny" style={{ color: '#F5B642' }}>Норма считается с {ruDate(g.cut)} — по данным {g.currentName}.</span>}
      {(g.current === 'APPLE_HEALTH' || g.current === 'HAE') && g.writers.length > 0 && (
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
            {pick !== 'MANUAL' && <button type="button" className="btn2" disabled={busy} onClick={() => apply('replace')}>
              {pv?.sameData && pick === 'HAE' ? 'Заменить прошлое, где есть выгрузка' : 'Заменить и историю'}{pv?.historyFrom ? ` (есть с ${ruDate(pv.historyFrom)})` : ''}
            </button>}
            <button type="button" className="linkbtn small" onClick={() => { setPick(null); setPv(null); }}>Отмена</button>
          </div>
          <span className="tiny" style={{ color: '#c9b78f', lineHeight: 1.5 }}>
            {pv?.sameData
              ? '«С сегодняшнего дня» — прошлые дни остаются как были. «Заменить прошлое» — дни, за которые есть данные нового пути, Kouros возьмёт оттуда и заново посчитает готовность за них. Норма в обоих случаях не сбрасывается.'
              : '«С сегодняшнего дня» — прошлые дни остаются как были, на графиках будет отметка смены, а норма первые 2 недели набирается заново. «Заменить и историю» — Kouros пересчитает прошлое по новому источнику там, где у него есть данные, и норма будет сразу.'}
          </span>
        </div>
      )}
      {g.fields.length > 0 && !pick && g.current !== 'MANUAL' && (
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
        Kouros получает через freddy всё, что там подключено, и хранит данные всех источников. Здесь выбираешь, какой из них показывать и считать — для каждого вида данных отдельно. Еда, вода, кофеин и алкоголь — из Apple Health (YAZIO, Waterllama): через freddy или, если подключён, через Health Auto Export.
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

/** 1.4: «Похоже, ты сменил часы на Garmin. Начать норму заново?» — one card per change of watch. */
export function DeviceChangeCard({ changes, onDone }: { changes: import('../lib/api').DeviceChange[]; onDone: () => void }) {
  const [busy, setBusy] = useState(false);
  const pairs = Array.from(new Set(changes.map((c) => `${c.from}→${c.to}`)));
  return (
    <>
      {pairs.map((p) => {
        const cs = changes.filter((c) => `${c.from}→${c.to}` === p);
        const since = cs.map((c) => c.since).sort()[0];
        const decide = async (reset: boolean) => {
          setBusy(true);
          try { await api.deviceChangeDecide(cs, reset); onDone(); } finally { setBusy(false); }
        };
        return (
          <section key={p} className="card" aria-label="Новые часы" style={{ borderColor: '#F5B64255' }}>
            <span className="label" style={{ color: '#F5B642' }}>Похоже, ты сменил часы на {cs[0].to}</span>
            <span className="small" style={{ lineHeight: 1.5 }}>
              С {ruDate(since)} {cs.map((c) => c.name.toLowerCase()).join(', ')} пишет {cs[0].to}, а раньше — {cs[0].from}. Разные часы меряют по-разному, поэтому старая норма может не подходить: готовность будет то завышенной, то заниженной.
            </span>
            <span className="muted small" style={{ lineHeight: 1.5 }}>Начать норму заново — Kouros 2 недели набирает её по новым часам («норма набирается»). Если часы те же или ты носишь двое, оставь как есть.</span>
            <div className="row" style={{ gap: 10 }}>
              <button type="button" className="btn" disabled={busy} onClick={() => decide(true)}>Начать норму заново</button>
              <button type="button" className="btn2" disabled={busy} onClick={() => decide(false)}>Оставить как есть</button>
            </div>
          </section>
        );
      })}
    </>
  );
}

/** 1.4: morning card «Введи HRV» while the HRV group (or breathing / SpO₂) is typed in by hand. */
export function ManualEntryCard({ m, onDone }: { m: import('../lib/api').ManualState; onDone: () => void }) {
  const has = (m.hrvOn ? m.values.hrv != null : true) && (m.vitalsOn ? m.values.resp != null || m.values.spo2 != null : true);
  const [edit, setEdit] = useState(!has);
  const [kind, setKind] = useState(m.kind);
  const [f, setF] = useState({ hrv: m.values.hrv?.toString() ?? '', resp: m.values.resp?.toString() ?? '', spo2: m.values.spo2?.toString() ?? '' });
  const [ask, setAsk] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const n = (v: string) => (v.trim() === '' ? null : Number(v.replace(',', '.')));
  const odd = () => {
    const out: string[] = [];
    const chk = (v: number | null, u: number | null, what: string, k: number) => {
      if (v == null || u == null) return;
      if (v > u * (1 + k)) out.push(`${what} намного выше обычного (${Math.round(u)})`);
      if (v < u * (1 - k)) out.push(`${what} намного ниже обычного (${Math.round(u)})`);
    };
    if (m.hrvOn) chk(n(f.hrv), m.usual.hrv, 'HRV', 0.5);
    if (m.vitalsOn) { chk(n(f.resp), m.usual.resp, 'Дыхание', 0.25); chk(n(f.spo2), m.usual.spo2, 'SpO₂', 0.04); }
    return out;
  };
  async function save(force: boolean) {
    setErr(null);
    const o = odd();
    if (!force && o.length) { setAsk(`${o.join('; ')} — точно?`); return; }
    try {
      await api.manualSave(m.today, { kind, ...(m.hrvOn ? { hrv: n(f.hrv) } : {}), ...(m.vitalsOn ? { resp: n(f.resp), spo2: n(f.spo2) } : {}) });
      setAsk(null); setEdit(false); onDone();
    } catch (e) { setErr(errText(e)); }
  }
  return (
    <section className="card" aria-label="Утренние показатели вручную">
      <div className="between">
        <span className="label">{has && !edit ? 'Утренние показатели' : m.hrvOn ? 'Введи утренний HRV' : 'Введи утренние показатели'}</span>
        {has && !edit && <button type="button" className="linkbtn small" onClick={() => setEdit(true)}>Изменить</button>}
      </div>
      {!edit ? (
        <span className="small">
          {m.hrvOn && <>HRV <b className="num">{m.values.hrv}</b> мс ({m.kind === 'sdnn' ? 'SDNN' : 'RMSSD'})</>}
          {m.vitalsOn && m.values.resp != null && <> · дыхание <b className="num">{m.values.resp}</b>/мин</>}
          {m.vitalsOn && m.values.spo2 != null && <> · SpO₂ <b className="num">{m.values.spo2}</b>%</>}
        </span>
      ) : (
        <>
          <span className="muted small" style={{ lineHeight: 1.5 }}>Утром, до кофе и тренировки — из приложения, которым ты меряешь (нагрудный датчик, Elite HRV, Welltory и т. п.). Готовность посчитается с этими цифрами.</span>
          <div className="row" style={{ gap: 10, alignItems: 'flex-end' }}>
            {m.hrvOn && (
              <div className="field"><label htmlFor="mn-hrv">HRV, мс</label><input id="mn-hrv" className="input" inputMode="decimal" style={{ width: 90 }} value={f.hrv} onChange={(e) => setF({ ...f, hrv: e.target.value })} /></div>
            )}
            {m.hrvOn && (
              <div className="row" role="group" aria-label="Какой HRV" style={{ gap: 4 }}>
                <button type="button" className="pill sm" aria-pressed={kind === 'rmssd'} onClick={() => setKind('rmssd')}>RMSSD</button>
                <button type="button" className="pill sm" aria-pressed={kind === 'sdnn'} onClick={() => setKind('sdnn')}>SDNN</button>
              </div>
            )}
            {m.vitalsOn && <div className="field"><label htmlFor="mn-resp">Дыхание, /мин</label><input id="mn-resp" className="input" inputMode="decimal" style={{ width: 90 }} value={f.resp} onChange={(e) => setF({ ...f, resp: e.target.value })} /></div>}
            {m.vitalsOn && <div className="field"><label htmlFor="mn-spo2">SpO₂, %</label><input id="mn-spo2" className="input" inputMode="decimal" style={{ width: 90 }} value={f.spo2} onChange={(e) => setF({ ...f, spo2: e.target.value })} /></div>}
          </div>
          {kind !== m.kind && m.usual.hrv != null && <span className="tiny" style={{ color: '#F5B642', lineHeight: 1.5 }}>RMSSD и SDNN — разные числа. После смены норма HRV начнёт набираться заново (около 2 недель).</span>}
          {ask ? (
            <div className="row" style={{ gap: 10 }}>
              <span className="small" style={{ color: '#F5B642' }}>{ask}</span>
              <button type="button" className="btn" onClick={() => save(true)}>Да, верно</button>
              <button type="button" className="linkbtn small" onClick={() => setAsk(null)}>Исправлю</button>
            </div>
          ) : (
            <div className="row" style={{ gap: 10 }}>
              <button type="button" className="btn" onClick={() => save(false)}>Сохранить</button>
              {has && <button type="button" className="linkbtn small" onClick={() => setEdit(false)}>Отмена</button>}
            </div>
          )}
          {err && <span className="small note-err">{err}</span>}
        </>
      )}
    </section>
  );
}
