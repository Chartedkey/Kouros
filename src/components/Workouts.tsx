// Workout sources (0.17): the settings card, the «Две записи одной тренировки» cards, and the list of
// deleted workouts.
import { useCallback, useEffect, useState } from 'react';
import { api, errText, sportOf, type DeletedWorkout, type DupGroup, type StravaGone, type WorkoutSources } from '../lib/api';
import { useDataChanged } from '../lib/events';

const GROUP_RU: Record<string, string> = { run: 'бега', walk: 'ходьбы', strength: 'силовых', ride: 'велосипеда', other: 'остального' };
const when = (t: number) => new Date(t * 1000).toLocaleString('ru-RU', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });

export function WorkoutSourcesCard() {
  const [st, setSt] = useState<WorkoutSources | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const load = useCallback(() => { api.workoutsState().then(setSt).catch((e) => setErr(errText(e))); }, []);
  useEffect(load, [load]);
  useDataChanged(load);
  if (!st) return err ? <div className="note-err">{err}</div> : null;
  const save = async (d: Parameters<typeof api.workoutsSave>[0]) => {
    setBusy(true); setErr(null);
    try { await api.workoutsSave(d); load(); } catch (e) { setErr(errText(e)); } finally { setBusy(false); }
  };
  const provs = st.providers ?? [];
  const rules = st.rules ?? {};
  const options: [string, string][] = [['ask', 'Спрашивать каждый раз'], ['best', 'Самая подробная запись'], ['strava', 'Всегда Strava'], ...st.available.map((p) => [p.provider, `Всегда ${p.name}`] as [string, string])];
  return (
    <section className="card" aria-label="Откуда брать тренировки">
      <span className="label">Откуда брать тренировки</span>
      <div className="row" style={{ gap: 8, flexWrap: 'wrap' }}>
        {([['strava', 'Только Strava'], ['freddy', 'Только freddy'], ['both', 'Strava и freddy']] as const).map(([k, n]) => (
          <button key={k} type="button" className="pill" disabled={busy} aria-pressed={st.mode === k} onClick={() => save({ mode: k })}>{n}</button>
        ))}
      </div>
      <div className="stack" style={{ gap: 4 }}>
        <span className="small"><b>Strava</b> <span className="muted">— пульс, темп и высота по секундам, трек GPS, подходы силовых. Самые подробные данные.</span></span>
        <span className="small"><b>freddy</b> <span className="muted">— тренировки часов, подключённых во freddy (Apple Watch через Apple Health, Garmin, Suunto…). Обычно итоги и пульс по ходу каждые ~15 с, без трека: нагрузка на мышцы считается без подъёмов и спусков.</span></span>
      </div>
      {st.mode !== 'strava' && st.available.length > 1 && (
        <div className="row" style={{ gap: 8, flexWrap: 'wrap' }}>
          <span className="small">Из freddy брать:</span>
          {st.available.map((p) => {
            const on = provs.length === 0 || provs.includes(p.provider);
            return <button key={p.provider} type="button" className="pill" aria-pressed={on} onClick={() => {
              const all = st.available.map((x) => x.provider);
              const cur = provs.length ? provs : all;
              const next = on ? cur.filter((x) => x !== p.provider) : [...cur, p.provider];
              save({ providers: next.length === all.length ? [] : next });
            }}>{p.name}</button>;
          })}
        </div>
      )}
      {st.mode === 'both' && (
        <div className="stack" style={{ gap: 6 }}>
          <span className="small"><b>Если пришли две записи одной тренировки</b></span>
          {st.groups.map((g) => (
            <div key={g.key} className="row" style={{ gap: 10, flexWrap: 'wrap' }}>
              <span className="small" style={{ minWidth: 170 }}>{g.name}</span>
              <select className="select" value={rules[g.key] ?? 'ask'} onChange={(e) => save({ rules: { ...rules, [g.key]: e.target.value } })}>
                {options.map(([k, n]) => <option key={k} value={k}>{n}</option>)}
              </select>
            </div>
          ))}
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>Для тренировок до включения freddy Kouros оставил записи Strava — прошлые цифры не сдвинулись. Выбор всегда можно поменять на странице тренировки.</span>
        </div>
      )}
      <span className="muted tiny">Записей: из Strava {st.counts.strava}, из freddy {st.counts.freddy}{st.counts.pending ? ` · ждут выбора: ${st.counts.pending}` : ''}{st.counts.deleted ? ` · удалённых: ${st.counts.deleted}` : ''}</span>
      {err && <div className="note-err">{err}</div>}
    </section>
  );
}

export function DupCards({ onOpen, max = 3 }: { onOpen?: (id: string) => void; max?: number }) {
  const [list, setList] = useState<DupGroup[]>([]);
  const [remember, setRemember] = useState<Record<string, boolean>>({});
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(() => { api.dupsPending().then(setList).catch(() => {}); }, []);
  useEffect(load, [load]);
  useDataChanged(load);
  if (!list.length) return null;
  const decide = (g: DupGroup, keep: string | null) => api.dupDecide(g.group, keep, !!remember[g.group] && keep != null).then(load).catch((e) => setErr(errText(e)));
  return (
    <>
      {list.slice(0, max).map((g) => (
        <section key={g.group} className="card" aria-label="Две записи одной тренировки" style={{ borderColor: '#2f3a52' }}>
          <div className="between"><span className="label">Две записи одной тренировки</span>{list.length > max && <span className="muted tiny">ещё {list.length - max}</span>}</div>
          <span className="muted small">Пока ты не выбрал, считается одна — самая подробная. Нагрузка не удвоится.</span>
          <div className="opts">
            {g.records.map((r) => {
              const sp = sportOf(r.sport);
              return (
                <div key={r.id} className="inset" style={{ gap: 6 }}>
                  <b className="small">{r.sourceName}</b>
                  <span className="small"><span style={{ color: sp.color }}>{sp.name}</span> · {when(r.start)} · {r.min} мин{r.km ? ` · ${String(r.km).replace('.', ',')} км` : ''}{r.avgHr ? ` · пульс ${r.avgHr}` : ''}</span>
                  <span className="muted tiny">{r.detail}</span>
                  <div className="row" style={{ gap: 8 }}>
                    <button type="button" className="btn2" onClick={() => decide(g, r.id)}>Оставить эту</button>
                    {onOpen && <button type="button" className="linkbtn small" onClick={() => onOpen(r.id)}>открыть</button>}
                  </div>
                </div>
              );
            })}
          </div>
          <div className="row" style={{ gap: 12, flexWrap: 'wrap' }}>
            <label className="row small" style={{ gap: 6, cursor: 'pointer' }}>
              <input type="checkbox" checked={!!remember[g.group]} onChange={(e) => setRemember({ ...remember, [g.group]: e.target.checked })} />
              Всегда так для {GROUP_RU[g.sportGroup] ?? 'этого вида спорта'}
            </label>
            <button type="button" className="linkbtn small" onClick={() => decide(g, null)}>Это разные тренировки — оставить обе</button>
          </div>
          {err && <div className="note-err">{err}</div>}
        </section>
      ))}
    </>
  );
}

/** 1.3.2: a workout of the last week that Strava no longer has — delete it here too or keep it. */
export function StravaGoneCards({ onOpen }: { onOpen?: (id: string) => void }) {
  const [list, setList] = useState<StravaGone[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(() => { api.stravaGone().then(setList).catch(() => {}); }, []);
  useEffect(load, [load]);
  useDataChanged(load);
  if (!list.length) return null;
  const decide = (id: string, del: boolean) => api.stravaGoneDecide(id, del).then(load).catch((e) => setErr(errText(e)));
  return (
    <>
      {list.slice(0, 2).map((g) => {
        const sp = sportOf(g.sport);
        return (
          <section key={g.id} className="card" aria-label="Тренировка удалена в Strava" style={{ borderColor: '#2f3a52' }}>
            <span className="label">Тренировка удалена в Strava</span>
            <span className="small"><span style={{ color: sp.color }}>{sp.name}</span>{g.name ? ` · ${g.name}` : ''} · {when(g.start)}</span>
            <span className="muted small">В Strava её больше нет. Удалить и в Kouros? Она попадёт в «Удалённые», вернуть можно в любой момент.</span>
            <div className="row" style={{ gap: 10, flexWrap: 'wrap' }}>
              <button type="button" className="btn2" onClick={() => decide(g.id, true)}>Удалить и в Kouros</button>
              <button type="button" className="linkbtn small" onClick={() => decide(g.id, false)}>Оставить</button>
              {onOpen && <button type="button" className="linkbtn small" onClick={() => onOpen(g.id)}>открыть</button>}
            </div>
            {err && <div className="note-err">{err}</div>}
          </section>
        );
      })}
    </>
  );
}

export function DeletedList() {
  const [list, setList] = useState<DeletedWorkout[]>([]);
  const [open, setOpen] = useState(false);
  const [confirm, setConfirm] = useState<string | null>(null);
  const load = useCallback(() => { api.workoutsDeleted().then(setList).catch(() => {}); }, []);
  useEffect(load, [load]);
  useDataChanged(load);
  if (!list.length) return null;
  return (
    <section className="card" aria-label="Удалённые тренировки">
      <button type="button" className="between plain" onClick={() => setOpen(!open)}>
        <span className="label">Удалённые · {list.length}</span><span className="muted small">{open ? 'свернуть' : 'показать'}</span>
      </button>
      {open && (
        <>
          <span className="muted tiny">Kouros их не считает и не загружает заново из Strava или freddy. В самой Strava ничего не меняется.</span>
          {list.map((w) => {
            const sp = sportOf(w.sport);
            return (
              <div key={w.id} className="list-row small" style={{ flexWrap: 'wrap' }}>
                <span style={{ flex: 1, minWidth: 0 }}><span style={{ color: sp.color }}>{sp.letter}</span> {w.name || sp.name} <span className="muted">· {when(w.start)} · {w.min} мин{w.km ? ` · ${String(w.km).replace('.', ',')} км` : ''} · {w.sourceName}</span></span>
                {confirm === w.id ? (
                  <span className="row" style={{ gap: 8 }}>
                    <span className="tiny" style={{ color: '#FF6B6B' }}>Удалить навсегда?</span>
                    <button type="button" className="btn2 btn-danger" onClick={() => api.workoutPurge(w.id).then(() => { setConfirm(null); load(); })}>Да</button>
                    <button type="button" className="linkbtn tiny" onClick={() => setConfirm(null)}>Нет</button>
                  </span>
                ) : (
                  <span className="row" style={{ gap: 12 }}>
                    <button type="button" className="linkbtn small" onClick={() => api.workoutRestore(w.id).then(load)}>Вернуть</button>
                    <button type="button" className="linkbtn tiny" style={{ color: 'var(--muted)' }} onClick={() => setConfirm(w.id)}>удалить навсегда</button>
                  </span>
                )}
              </div>
            );
          })}
        </>
      )}
    </section>
  );
}
