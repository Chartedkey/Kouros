import { useCallback, useEffect, useState } from 'react';
import {
  api, errText, fmtBytes, fmtDateTime, fmtIso, plural,
  type AthleteProfile, type BackupInfo, type SecretName, type SourceStatus, type Status,
} from '../lib/api';
import { useDataChanged } from '../lib/events';
import { DataSourcesCard } from '../components/DataSources';
import { WorkoutSourcesCard } from '../components/Workouts';
import { setZoom, useZoom, ZOOM_MAX, ZOOM_MIN } from '../lib/zoom';
import { paceFromThreshold, zonesFromMax } from './TrainingParts';
import { UpdatesSection } from '../components/Updates';

type Sub = 'sync' | 'prof' | 'view' | 'data' | 'about';

export function Settings({ status, refresh, goImport, initialSub, onSubUsed }: { status: Status; refresh: () => void; goImport: () => void; initialSub?: 'prof' | null; onSubUsed?: () => void }) {
  const [sub, setSub] = useState<Sub>(initialSub ?? 'sync');
  useEffect(() => { if (initialSub) { setSub(initialSub); onSubUsed?.(); } }, [initialSub, onSubUsed]);
  return (
    <>
      <div className="page-head">
        <div className="stack">
          <h1>Настройки</h1>
          <p className="subtitle">Источники данных, ключи, папка с данными и безопасность</p>
        </div>
      </div>
      <nav className="subtabs" aria-label="Разделы настроек">
        {([['sync', 'Синхронизация'], ['prof', 'Профиль и зоны'], ['view', 'Вид и карта'], ['data', 'Данные и безопасность'], ['about', 'О приложении']] as [Sub, string][]).map(
          ([id, name]) => (
            <button key={id} type="button" aria-pressed={sub === id} onClick={() => setSub(id)}>{name}</button>
          ),
        )}
      </nav>
      {sub === 'sync' && <SyncTab goImport={goImport} />}
      {sub === 'prof' && <ProfileTab />}
      {sub === 'view' && <ViewTab />}
      {sub === 'data' && <DataTab status={status} refresh={refresh} />}
      {sub === 'about' && <AboutTab status={status} />}
    </>
  );
}

// ---------- Синхронизация ----------

type Src = {
  id: string;
  letter: string;
  bg: string;
  fg: string;
  name: string;
  via: string;
  gives: string;
  secrets: { name: SecretName; label: string; hint?: string }[];
  stage: string;
  help?: string;
};

const SOURCES: Src[] = [
  {
    id: 'strava', letter: 'S', bg: '#2A1C14', fg: '#FF9A6B', name: 'Strava', via: 'напрямую, по API',
    gives: 'Все тренировки: бег, ходьба, вело, силовые; пульс, темп, мощность, обувь',
    secrets: [
      { name: 'strava_client_id', label: 'Client ID', hint: 'число со страницы strava.com/settings/api' },
      { name: 'strava_client_secret', label: 'Client Secret' },
    ],
    stage: '',
    help: 'Создай приложение на strava.com/settings/api: Website — http://localhost, Authorization Callback Domain — localhost.',
  },
  {
    id: 'freddy', letter: 'H', bg: '#2A1717', fg: '#FF8B8B', name: 'Здоровье', via: 'через freddy',
    gives: 'Apple Health и всё, что подключено во freddy (Garmin, Suunto, Fitbit…): сон, HRV, пульс покоя, VO₂max, вес, питание YAZIO, вода',
    secrets: [{ name: 'freddy_api_key', label: 'API-ключ freddy', hint: 'freddy.coach → AI apps → API keys' }],
    stage: '',
  },
  {
    id: 'files', letter: 'F', bg: '#22262D', fg: '#C3CAD2', name: 'Файлы тренировок', via: 'импорт вручную',
    gives: '.fit, .gpx, .tcx и архив данных Strava — запасной путь',
    secrets: [],
    stage: '',
  },
];

function SyncTab({ goImport }: { goImport: () => void }) {
  const [saved, setSaved] = useState<Partial<Record<SecretName, boolean>>>({});
  const [err, setErr] = useState<string | null>(null);

  const load = () => api.secretStatus().then(setSaved).catch((e) => setErr(errText(e)));
  useEffect(() => { load(); }, []);

  return (
    <>
      <section className="card" aria-label="Источники данных">
        <div className="between" style={{ flexWrap: 'wrap' }}>
          <span className="label">Источники данных</span>
          <span className="muted tiny">Тренировки — из Strava (и freddy, если включено), здоровье и питание — через freddy</span>
        </div>
        <div className="note-ok">
          Ключи хранятся в Диспетчере учётных данных Windows, а не в файлах. После сохранения Kouros их больше не показывает
          — только заменить или удалить.
        </div>
        {err && <div className="note-err">{err}</div>}
        {SOURCES.map((s) => {
          const ready = s.secrets.length > 0 && s.secrets.every((k) => saved[k.name]);
          return (
            <div key={s.id} className="inset">
              <div className="between" style={{ alignItems: 'flex-start', flexWrap: 'wrap' }}>
                <div className="row" style={{ gap: 12, flexWrap: 'nowrap', minWidth: 0 }}>
                  <span style={{ flex: 'none', width: 36, height: 36, borderRadius: 10, display: 'flex', alignItems: 'center', justifyContent: 'center', fontFamily: 'var(--display)', fontWeight: 700, fontSize: 14, background: s.bg, color: s.fg }}>
                    {s.letter}
                  </span>
                  <div className="stack" style={{ gap: 2, minWidth: 0 }}>
                    <span style={{ fontWeight: 700 }}>{s.name} <span className="muted tiny" style={{ fontWeight: 500 }}>· {s.via}</span></span>
                    <span className="muted small">{s.gives}</span>
                  </div>
                </div>
                <div className="row">
                  {s.secrets.length > 0 && (
                    <span className={`chip ${ready ? 'chip-ok' : 'chip-off'}`}>{ready ? 'ключи сохранены' : 'не настроено'}</span>
                  )}
                  {s.stage && <span className="chip chip-off">{s.stage}</span>}
                </div>
              </div>
              {s.help && <span className="muted tiny" style={{ lineHeight: 1.5 }}>{s.help}</span>}
              {s.secrets.map((k) => (
                <SecretField key={k.name} spec={k} saved={!!saved[k.name]} onChange={load} />
              ))}
              {s.id === 'strava' && <StravaPanel ready={ready} goImport={goImport} />}
              {s.id === 'freddy' && <FreddyPanel ready={ready} />}
              {s.id === 'files' && (
                <button type="button" className="btn2" style={{ alignSelf: 'flex-start' }} onClick={goImport}>Открыть импорт →</button>
              )}
            </div>
          );
        })}
      </section>
      <DataSourcesCard />
      <WorkoutSourcesCard />
    </>
  );
}

function SecretField({
  spec, saved, onChange,
}: { spec: { name: SecretName; label: string; hint?: string }; saved: boolean; onChange: () => void }) {
  const [value, setValue] = useState('');
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  async function save() {
    setBusy(true); setMsg(null);
    try {
      await api.secretSet(spec.name, value);
      setValue(''); setEditing(false); onChange();
    } catch (e) { setMsg(errText(e)); } finally { setBusy(false); }
  }
  async function remove() {
    setBusy(true); setMsg(null);
    try { await api.secretDelete(spec.name); onChange(); } catch (e) { setMsg(errText(e)); } finally { setBusy(false); }
  }

  const id = `sec-${spec.name}`;
  return (
    <div className="stack" style={{ gap: 6 }}>
      <label htmlFor={id} className="small" style={{ fontWeight: 600 }}>
        {spec.label}{spec.hint && <span className="muted" style={{ fontWeight: 500 }}> — {spec.hint}</span>}
      </label>
      {saved && !editing ? (
        <div className="row">
          <span className="chip chip-ok">✓ сохранён в хранилище Windows</span>
          <button type="button" className="btn2" onClick={() => setEditing(true)} disabled={busy}>Заменить</button>
          <button type="button" className="btn2 btn-danger" onClick={remove} disabled={busy}>Удалить</button>
        </div>
      ) : (
        <div className="row" style={{ flexWrap: 'nowrap' }}>
          <input
            id={id} className="input" type="password" autoComplete="off" spellCheck={false}
            value={value} onChange={(e) => setValue(e.target.value)} placeholder="вставь значение"
            onKeyDown={(e) => { if (e.key === 'Enter' && value.trim()) save(); }}
          />
          <button type="button" className="btn" onClick={save} disabled={busy || !value.trim()}>Сохранить</button>
          {editing && <button type="button" className="btn2" onClick={() => { setEditing(false); setValue(''); }}>Отмена</button>}
        </div>
      )}
      {msg && <div className="note-err">{msg}</div>}
    </div>
  );
}

function StravaPanel({ ready, goImport }: { ready: boolean; goImport: () => void }) {
  const [st, setSt] = useState<SourceStatus | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [choice, setChoice] = useState(false);
  const load = useCallback(() => { api.stravaStatus().then(setSt).catch(() => {}); }, []);
  useEffect(load, [load, ready]);
  useDataChanged(load);

  async function sync(full: boolean) {
    setBusy('Загружаю тренировки из Strava…'); setMsg(null); setChoice(false);
    try {
      const r = await api.stravaSync(full);
      if (r.needsChoice) { setChoice(true); return; }
      const parts = [
        r.added ? `новых тренировок: ${r.added}` : 'новых тренировок нет',
        r.merged ? `дополнено: ${r.merged}` : '',
        r.streams ? `графики пульса и темпа: ${r.streams}` : '',
        r.streamsLeft ? `ещё ${r.streamsLeft} догружу в фоне` : '',
      ].filter(Boolean);
      setMsg({ ok: true, text: (r.rateLimited ? 'Strava просит паузу — продолжу позже. ' : '') + parts.join(', ') + '.' });
    } catch (e) { setMsg({ ok: false, text: errText(e) }); } finally { setBusy(null); load(); }
  }
  async function connect() {
    setBusy('Открыл браузер — разреши доступ в окне Strava…'); setMsg(null);
    try {
      const name = await api.stravaConnect();
      setMsg({ ok: true, text: `Strava подключена${name ? `: ${name}` : ''}.` });
      load();
      await sync(false);
    } catch (e) { setMsg({ ok: false, text: errText(e) }); setBusy(null); }
  }
  async function disconnect() {
    try { await api.stravaDisconnect(); setMsg(null); load(); } catch (e) { setMsg({ ok: false, text: errText(e) }); }
  }

  if (!st) return null;
  const left = (() => { try { return st.extra ? (JSON.parse(st.extra).streamsLeft as number) : 0; } catch { return 0; } })();
  return (
    <div className="stack" style={{ gap: 10, paddingTop: 6 }}>
      {st.connected ? (
        <div className="between" style={{ flexWrap: 'wrap' }}>
          <span className="small">
            <span className="chip chip-ok">подключена{st.athlete ? ` · ${st.athlete}` : ''}</span>{' '}
            <span className="muted">последняя синхронизация: {fmtIso(st.lastOk)}{left ? ` · графики догружаются: осталось ${left}` : ''}</span>
          </span>
          <div className="row">
            <button type="button" className="btn2" disabled={!!busy || st.busy} onClick={() => sync(false)}>Обновить сейчас</button>
            <button type="button" className="btn2 btn-danger" disabled={!!busy} onClick={disconnect}>Отключить</button>
          </div>
        </div>
      ) : (
        <div className="row">
          <button type="button" className="btn btn-green" disabled={!ready || !!busy} onClick={connect}>Подключить Strava</button>
          {!ready && <span className="muted tiny">сначала сохрани Client ID и Client Secret</span>}
        </div>
      )}
      {st.connected && <span className="muted tiny">Kouros проверяет новые тренировки каждые 20 минут, пока приложение открыто.</span>}
      {busy && <div className="note-warn">{busy}</div>}
      {st.lastError && !busy && !msg && <div className="note-err">Последняя ошибка: {st.lastError}</div>}
      {msg && <div className={msg.ok ? 'note-ok' : 'note-err'}>{msg.text}</div>}
      {choice && (
        <div className="inset">
          <b>В Kouros ещё нет тренировок. Откуда взять историю?</b>
          <span className="muted small" style={{ lineHeight: 1.5 }}>
            Архив Strava загрузится за минуты и сразу с пульсом и темпом. Через Strava история придёт сразу, а графики будут догружаться по 40 тренировок за раз — пару дней.
          </span>
          <div className="row">
            <button type="button" className="btn btn-green" onClick={goImport}>Импортировать архив (рекомендую)</button>
            <button type="button" className="btn2" onClick={() => sync(true)}>Загрузить всё через Strava</button>
          </div>
        </div>
      )}
    </div>
  );
}

function FreddyPanel({ ready }: { ready: boolean }) {
  const [st, setSt] = useState<SourceStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const load = useCallback(() => { api.freddyStatus().then(setSt).catch(() => {}); }, []);
  useEffect(load, [load, ready]);
  useDataChanged(load);
  async function sync(full: boolean) {
    setBusy(true); setMsg(null);
    try {
      const r = await api.freddySync(full);
      setMsg({ ok: true, text: `Загружено ${plural(r.rows, 'запись', 'записи', 'записей')}${r.lastDate ? `, данные по ${r.lastDate}` : ''}.` });
    } catch (e) { setMsg({ ok: false, text: errText(e) }); } finally { setBusy(false); load(); }
  }
  if (!st) return null;
  return (
    <div className="stack" style={{ gap: 10, paddingTop: 6 }}>
      <div className="between" style={{ flexWrap: 'wrap' }}>
        <span className="small">
          {st.connected
            ? <><span className="chip chip-ok">на связи</span> <span className="muted">последняя загрузка: {fmtIso(st.lastOk)}{st.extra ? ` · данные по ${st.extra}` : ''}</span></>
            : <span className="muted">Первая загрузка заберёт всю историю, доступную в freddy.</span>}
        </span>
        <div className="row">
          <button type="button" className={st.connected ? 'btn2' : 'btn btn-green'} disabled={!ready || busy || st.busy} onClick={() => sync(!st.connected)}>
            {busy ? 'Загружаю…' : st.connected ? 'Обновить сейчас' : 'Загрузить из freddy'}
          </button>
        </div>
      </div>
      {st.lastError && !busy && !msg && <div className="note-err">Последняя ошибка: {st.lastError}</div>}
      {msg && <div className={msg.ok ? 'note-ok' : 'note-err'}>{msg.text}</div>}
    </div>
  );
}

// ---------- Профиль и зоны ----------

const parsePace = (v: string): number | null => { const m = v.trim().match(/^(\d{1,2})[:.,](\d{2})$/); return m ? Number(m[1]) * 60 + Number(m[2]) : null; };

const ZONE_NAMES = ['Z1 восстановление', 'Z2 база', 'Z3 темп', 'Z4 порог', 'Z5 МПК'];

function ProfileTab() {
  const [p, setP] = useState<(AthleteProfile & { hrRestAuto: number }) | null>(null);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [pace, setPace] = useState('');
  const [lab, setLab] = useState({ value: '', date: '', where: '' });
  useEffect(() => {
    api.profileGet().then((v) => { setP(v); setPace(v.thresholdPaceS ? `${Math.floor(v.thresholdPaceS / 60)}:${String(Math.round(v.thresholdPaceS % 60)).padStart(2, '0')}` : ''); }).catch((e) => setMsg({ ok: false, text: errText(e) }));
    api.healthSummary().then((h) => { const l = h.vo2.lab; if (l) setLab({ value: String(l.value).replace('.', ','), date: l.date ?? '', where: l.where ?? '' }); }).catch(() => {});
  }, []);
  if (!p) return msg ? <div className="note-err">{msg.text}</div> : null;
  const num = (v: string) => (v.trim() === '' ? NaN : Number(v.replace(',', '.')));
  const upd = (k: keyof AthleteProfile, v: unknown) => setP({ ...p, [k]: v } as typeof p);
  const setZone = (i: number, v: string) => { const z = p.zones.slice(); z[i] = num(v); upd('zones', z); };
  async function save() {
    if (!p) return;
    try {
      const { hrRestAuto: _, ...prof } = p;
      const t = parsePace(pace);
      await api.profileSet({ ...prof, filled: true, thresholdPaceS: t ?? prof.thresholdPaceS, paceZones: t && t !== prof.thresholdPaceS ? paceFromThreshold(t) : prof.paceZones });
      const lv = Number(lab.value.replace(',', '.'));
      if (lab.value.trim() && Number.isFinite(lv)) await api.healthSettingSet('lab_vo2', { value: lv, date: lab.date || null, where: lab.where || null });
      setP({ ...p, filled: true });
      setMsg({ ok: true, text: 'Сохранено. Нагрузка и зоны всех тренировок пересчитаются при следующем открытии «Сегодня».' });
    } catch (e) { setMsg({ ok: false, text: errText(e) }); }
  }
  function fromMax() {
    if (!p || !(p.hrMax > 100)) return;
    setP({ ...p, zones: zonesFromMax(p.hrMax), zonesDate: null });
  }
  return (
    <div className="grid">
      {!p.filled && (
        <section className="card" aria-label="Первый запуск" style={{ borderColor: '#3DDC9755' }}>
          <span className="label" style={{ color: '#3DDC97' }}>Заполни профиль</span>
          <span className="small" style={{ lineHeight: 1.5 }}>Сейчас стоят примерные значения. По ним Kouros считает нагрузку, зоны, калории и план — чем точнее данные, тем точнее советы. Если был лабораторный тест (лактат, газоанализ) — возьми пульс и темп оттуда.</span>
        </section>
      )}
      <section className="card" aria-label="О тебе">
        <span className="label">О тебе</span>
        <div className="row" style={{ gap: 16, alignItems: 'flex-end', flexWrap: 'wrap' }}>
          <div className="field"><label htmlFor="pname">Имя</label><input id="pname" value={p.name ?? ''} placeholder="как к тебе обращаться" onChange={(e) => upd('name', e.target.value)} /></div>
          <div className="field"><label htmlFor="by">Год рождения</label><input id="by" inputMode="numeric" value={Number.isNaN(p.birthYear) ? '' : p.birthYear} onChange={(e) => upd('birthYear', num(e.target.value))} /></div>
          <div className="field"><label htmlFor="height">Рост, см</label><input id="height" inputMode="numeric" value={Number.isNaN(p.heightCm) ? '' : p.heightCm} onChange={(e) => upd('heightCm', num(e.target.value))} /></div>
        </div>
        <div className="row" role="group" aria-label="Пол">
          <button type="button" className="pill" aria-pressed={p.male} onClick={() => upd('male', true)}>Мужчина</button>
          <button type="button" className="pill" aria-pressed={!p.male} onClick={() => upd('male', false)}>Женщина</button>
        </div>
        <span className="muted tiny">Пол и возраст нужны для расчёта калорий и норм витаминов. Вес берётся из Apple Health.</span>
      </section>
      <section className="card" aria-label="Пульс">
        <span className="label">Пульс</span>
        <div className="row" style={{ gap: 16, alignItems: 'flex-end' }}>
          <div className="field"><label htmlFor="hrmax">Максимальный пульс</label><input id="hrmax" inputMode="numeric" value={Number.isNaN(p.hrMax) ? '' : p.hrMax} onChange={(e) => upd('hrMax', num(e.target.value))} /></div>
          <div className="field"><label htmlFor="hrrest">Пульс покоя</label><input id="hrrest" inputMode="numeric" placeholder={`авто: ${p.hrRestAuto}`} value={p.hrRest ?? ''} onChange={(e) => upd('hrRest', e.target.value.trim() === '' ? null : num(e.target.value))} /></div>
        </div>
        <span className="muted tiny" style={{ lineHeight: 1.5 }}>
          Пульс покоя по умолчанию — медиана за 30 дней из Apple Health (сейчас {p.hrRestAuto}). Оставь поле пустым, чтобы считать автоматически.
        </span>
        <span className="label" style={{ paddingTop: 8 }}>Зоны пульса, уд/мин</span>
        <div className="stack" style={{ gap: 8 }}>
          {ZONE_NAMES.map((n, i) => (
            <div key={n} className="between small">
              <span>{n}</span>
              <span className="row" style={{ gap: 6 }}>
                <input aria-label={`${n}: от`} className="input" style={{ width: 80, height: 38, flex: 'none' }} value={Number.isNaN(p.zones[i]) ? '' : p.zones[i]} onChange={(e) => setZone(i, e.target.value)} />
                <span className="muted">—</span>
                <input aria-label={`${n}: до`} className="input" style={{ width: 80, height: 38, flex: 'none' }} value={Number.isNaN(p.zones[i + 1]) ? '' : p.zones[i + 1]} onChange={(e) => setZone(i + 1, e.target.value)} />
              </span>
            </div>
          ))}
        </div>
        <div className="row">
          <div className="field"><label htmlFor="zdate">Дата теста</label><input id="zdate" style={{ width: 150 }} value={p.zonesDate ?? ''} placeholder="ГГГГ-ММ-ДД" onChange={(e) => upd('zonesDate', e.target.value || null)} /></div>
        </div>
        <div className="row" style={{ gap: 16, alignItems: 'flex-end', flexWrap: 'wrap' }}>
          <div className="field"><label htmlFor="tpace">Пороговый темп, мин:с на км</label><input id="tpace" style={{ width: 120 }} value={pace} placeholder="5:30" onChange={(e) => setPace(e.target.value)} /></div>
          <button type="button" className="btn2" onClick={fromMax}>Рассчитать зоны от макс. пульса</button>
        </div>
        <span className="muted tiny" style={{ lineHeight: 1.5 }}>Не знаешь максимальный пульс — возьми самый высокий за последние тяжёлые тренировки или финиш забега; формула 208 − 0,7 × возраст даёт только примерное значение. Пороговый темп — темп, который держишь около часа на пределе (примерно темп забега на 10 км).</span>
        <span className="label" style={{ paddingTop: 8 }}>VO₂max по лабораторному тесту</span>
        <div className="row" style={{ gap: 16, alignItems: 'flex-end', flexWrap: 'wrap' }}>
          <div className="field"><label htmlFor="lv">мл/кг/мин</label><input id="lv" style={{ width: 100 }} inputMode="decimal" value={lab.value} onChange={(e) => setLab({ ...lab, value: e.target.value })} /></div>
          <div className="field"><label htmlFor="ld">Дата</label><input id="ld" type="date" value={lab.date} onChange={(e) => setLab({ ...lab, date: e.target.value })} /></div>
          <div className="field"><label htmlFor="lw">Где</label><input id="lw" value={lab.where} placeholder="лаборатория" onChange={(e) => setLab({ ...lab, where: e.target.value })} /></div>
        </div>
        <span className="muted tiny">Необязательно. Нужен как точка отсчёта для VO₂max с часов.</span>
      </section>
      <section className="card" aria-label="Тело и питание">
        <span className="label">Для расчёта питания</span>
        <div className="row" style={{ gap: 16, alignItems: 'flex-end' }}>
          <div className="field"><label htmlFor="prot">Белок, г на кг</label><input id="prot" inputMode="decimal" placeholder={`по цели: ${String(p.proteinGPerKg).replace('.', ',')}`} value={p.proteinManual == null || Number.isNaN(p.proteinManual) ? '' : p.proteinManual} onChange={(e) => upd('proteinManual', e.target.value.trim() === '' ? null : num(e.target.value))} /></div>
        </div>
        <span className="muted tiny" style={{ lineHeight: 1.5 }}>
          Белок: пусто — по цели из вкладки «Питание» (1,8 г/кг, при снижении веса 2,0). Вес берётся из Apple Health. Калории — по формуле Миффлина — Сан Жеора с поправкой на обычную активность и тренировки дня.
        </span>
        <span className="label" style={{ paddingTop: 8 }}>Рост нагрузки за неделю</span>
        <div className="row" role="group" aria-label="Порог недели">
          {([[1.2, 'Осторожно'], [1.3, 'Обычно'], [1.4, 'Смело']] as [number, string][]).map(([v, n]) => (
            <button key={v} type="button" className="pill" aria-pressed={(p.weekCap ?? 1.3) === v} onClick={() => upd('weekCap', v)}>
              {n} · ×{String(v).replace('.', ',')}
            </button>
          ))}
        </div>
        <span className="muted tiny" style={{ lineHeight: 1.5 }}>
          Насколько неделя может быть тяжелее твоей обычной (средняя за 4 недели). Верх дневной цели на «Сегодня» не даст перейти этот порог.
          «Обычно» — общепринятая граница безопасного роста; «смело» — если хорошо восстанавливаешься.
        </span>
        <div className="row divider" style={{ paddingTop: 14 }}>
          <button type="button" className="btn" onClick={save}>Сохранить</button>
        </div>
        {msg && <div className={msg.ok ? 'note-ok' : 'note-err'}>{msg.text}</div>}
      </section>
      <NormsCard />
    </div>
  );
}

/** Which country's daily norms of vitamins and minerals to compare with. */
function NormsCard() {
  const [norms, setNorms] = useState<{ country: string; list: { code: string; name: string; source: string }[] } | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  useEffect(() => { api.nutritionSummary().then((n) => setNorms(n.norms ?? null)).catch(() => {}); }, []);
  const pick = async (code: string) => {
    try {
      await api.healthSettingSet('nutrient_norms', code);
      setNorms((n) => (n ? { ...n, country: code } : n));
      setMsg(null);
    } catch (e) { setMsg(errText(e)); }
  };
  return (
    <section className="card" aria-label="Нормы витаминов и минералов">
      <span className="label">Нормы витаминов и минералов</span>
      <span className="small" style={{ color: 'var(--text-2)', lineHeight: 1.5 }}>С какими нормами сравнивать питание. У разных стран они немного отличаются — например, витамин D: 15 мкг в России и США, 20 мкг в Германии.</span>
      {norms ? (
        <div className="row" role="group" aria-label="Страна норм" style={{ flexWrap: 'wrap' }}>
          {norms.list.map((c) => <button key={c.code} type="button" className="pill" aria-pressed={norms.country === c.code} title={c.source} onClick={() => pick(c.code)}>{c.name}</button>)}
        </div>
      ) : <span className="muted small">Загружаю…</span>}
      {norms && <span className="muted tiny">Источник: {norms.list.find((c) => c.code === norms.country)?.source}. Нормы для взрослого мужчины.</span>}
      {msg && <div className="note-err">{msg}</div>}
    </section>
  );
}

// ---------- Данные и безопасность ----------

function DataTab({ status, refresh }: { status: Status; refresh: () => void }) {
  const [backups, setBackups] = useState<BackupInfo[]>([]);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [key, setKey] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const loadBackups = () => api.backups().then(setBackups).catch((e) => setMsg({ ok: false, text: errText(e) }));
  useEffect(() => { loadBackups(); }, []);

  async function backup() {
    setBusy(true); setMsg(null);
    try {
      const name = await api.backupNow();
      setMsg({ ok: true, text: `Копия ${name} сохранена в папке backups.` });
      loadBackups(); refresh();
    } catch (e) { setMsg({ ok: false, text: errText(e) }); } finally { setBusy(false); }
  }
  async function showKey() {
    try {
      const k = await api.recoveryKey();
      if (k) setKey(k);
    } catch (e) { setMsg({ ok: false, text: errText(e) }); }
  }
  async function toggleLock() {
    try {
      if (await api.setLockEnabled(!status.lockEnabled)) refresh();
    } catch (e) { setMsg({ ok: false, text: errText(e) }); }
  }

  const helloText = {
    available: 'Windows Hello настроен на этом компьютере.',
    not_configured: 'Windows Hello не настроен. Включи PIN-код: Параметры → Учётные записи → Варианты входа. До этого Kouros открывается без проверки.',
    unavailable: 'Windows Hello недоступен на этом устройстве — Kouros открывается без проверки.',
  }[status.hello];

  const grouped = key ? key.match(/.{1,8}/g)!.join(' ') : '';

  return (
    <div className="grid">
      <div className="stack" style={{ gap: 20 }}>
        <section className="card" aria-label="Папка с данными">
          <span className="label">Папка с данными</span>
          <div className="inset">
            <span className="mono" style={{ fontSize: 14, wordBreak: 'break-all' }}>{status.dataDir ?? '—'}</span>
            <ul className="muted small" style={{ margin: 0, paddingLeft: 18, lineHeight: 1.7 }}>
              <li><span className="mono">kouros.db</span> — вся история, зашифрована</li>
              <li><span className="mono">files\</span> — импортированные файлы и документы</li>
              <li><span className="mono">backups\</span> — резервные копии</li>
              <li><span className="mono">exports\</span> — выгрузки</li>
            </ul>
          </div>
          <div className="row">
            <button type="button" className="btn2" onClick={() => api.openDataDir().catch((e) => setMsg({ ok: false, text: errText(e) }))}>
              Открыть папку
            </button>
          </div>
        </section>

        <section className="card" aria-label="База данных">
          <span className="label">База данных</span>
          {status.dbError && <div className="note-err">{status.dbError}</div>}
          <div>
            <div className="list-row"><span className="muted">Шифрование</span><span style={{ fontWeight: 700 }}>{status.cipherVersion ? `SQLCipher ${status.cipherVersion.split(' ')[0]} · AES-256` : '—'}</span></div>
            <div className="list-row"><span className="muted">Размер</span><span style={{ fontWeight: 700 }}>{fmtBytes(status.dbSize)}</span></div>
            <div className="list-row"><span className="muted">Версия схемы</span><span style={{ fontWeight: 700 }}>{status.schemaVersion}</span></div>
          </div>
        </section>

        <section className="card" aria-label="Вход">
          <div className="between">
            <div className="stack" style={{ gap: 4 }}>
              <span style={{ fontWeight: 700 }}>Вход через Windows Hello</span>
              <span className="muted tiny" style={{ lineHeight: 1.5 }}>{helloText}</span>
            </div>
            <button
              type="button" role="switch" className="switch" aria-checked={status.lockEnabled}
              aria-label="Вход через Windows Hello" onClick={toggleLock}
            ><span /></button>
          </div>
        </section>
      </div>

      <div className="stack" style={{ gap: 20 }}>
        <section className="card" aria-label="Резервные копии">
          <div className="between">
            <span className="label">Резервные копии</span>
            <button type="button" className="btn" onClick={backup} disabled={busy}>{busy ? 'Сохраняю…' : 'Сделать копию сейчас'}</button>
          </div>
          <span className="muted small" style={{ lineHeight: 1.5 }}>
            Kouros сам делает копию раз в день при запуске и хранит последние 14. Копии зашифрованы тем же ключом, что и база.
            Чтобы сохранить данные на другом диске или в облаке, копируй папку <span className="mono">backups</span>.
          </span>
          {msg && <div className={msg.ok ? 'note-ok' : 'note-err'}>{msg.text}</div>}
          <div>
            {backups.length === 0 && <div className="list-row muted small">Копий пока нет</div>}
            {backups.slice(0, 7).map((b) => (
              <div key={b.name} className="list-row small">
                <span className="mono" style={{ wordBreak: 'break-all' }}>{b.name}</span>
                <span className="muted" style={{ whiteSpace: 'nowrap' }}>{fmtDateTime(b.modified)} · {fmtBytes(b.size)}</span>
              </div>
            ))}
            {backups.length > 7 && <div className="list-row muted tiny">и ещё {backups.length - 7}</div>}
          </div>
        </section>

        <section className="card" aria-label="Ключ восстановления">
          <span className="label">Ключ восстановления</span>
          <span className="small" style={{ lineHeight: 1.55, color: 'var(--text-2)' }}>
            База и копии зашифрованы ключом, который лежит в хранилище Windows этого компьютера. Чтобы открыть копию
            на другом компьютере или после переустановки Windows, понадобится этот ключ. Сохрани его в менеджере паролей.
          </span>
          {key ? (
            <div className="inset">
              <span className="mono" style={{ fontSize: 15, letterSpacing: '.04em', lineHeight: 1.7, wordBreak: 'break-all' }}>{grouped}</span>
              <div className="row">
                <button
                  type="button" className="btn2"
                  onClick={() => navigator.clipboard.writeText(key).then(() => setCopied(true)).catch(() => setCopied(false))}
                >{copied ? '✓ Скопировано' : 'Скопировать'}</button>
                <button type="button" className="btn2" onClick={() => { setKey(null); setCopied(false); }}>Скрыть</button>
              </div>
            </div>
          ) : (
            <button type="button" className="btn2" style={{ alignSelf: 'flex-start' }} onClick={showKey}>
              Показать — подтвердить через Windows Hello
            </button>
          )}
          <div className="note-warn">Никому не отправляй этот ключ: с ним и копией базы можно прочитать все твои данные.</div>
        </section>
      </div>
    </div>
  );
}

// ---------- О приложении ----------

function AboutTab({ status }: { status: Status }) {
  return (
    <div className="grid">
      <section className="card" aria-label="Версия">
        <span className="label">Kouros</span>
        <div>
          <div className="list-row"><span className="muted">Версия</span><span style={{ fontWeight: 700 }}>{status.version}</span></div>
          <div className="list-row"><span className="muted">Система</span><span style={{ fontWeight: 700 }}>{status.platform}</span></div>
          <div className="list-row"><span className="muted">Данные</span><span style={{ fontWeight: 700 }}>только на этом компьютере</span></div>
        </div>
        <span className="muted small" style={{ lineHeight: 1.55 }}>
          Твои данные уходят только в Strava и freddy — и только когда ты их подключишь. Ещё Kouros обращается к GitHub (есть ли новая версия),
          к карте OpenStreetMap, если она включена, и к открытым справочникам лекарств — без каких-либо твоих данных. Никакой аналитики и трекеров.
        </span>
      </section>
      <UpdatesSection />
    </div>
  );
}

// ---------- Вид и карта ----------

function ViewTab() {
  const zoom = useZoom();
  const [map, setMap] = useState<{ enabled: boolean; tiles: number; bytes: number } | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const load = () => api.mapCacheInfo().then(setMap).catch((e) => setMsg(errText(e)));
  useEffect(() => { load(); }, []);
  const [medsOn, setMedsOn] = useState<boolean | null>(null);
  useEffect(() => { api.medsEnabled().then(setMedsOn).catch(() => {}); }, []);
  async function toggleMeds() {
    if (medsOn == null) return;
    try { await api.medsEnabledSet(!medsOn); setMedsOn(!medsOn); } catch (e) { setMsg(errText(e)); }
  }
  async function toggle() {
    if (!map) return;
    try { await api.mapSetEnabled(!map.enabled); load(); } catch (e) { setMsg(errText(e)); }
  }
  async function clear() {
    try { await api.mapCacheClear(); load(); } catch (e) { setMsg(errText(e)); }
  }
  return (
    <div className="grid">
      <section className="card" aria-label="Масштаб">
        <div className="between"><span className="label">Масштаб интерфейса</span><span className="num" style={{ fontSize: 22, fontWeight: 700 }}>{Math.round(zoom * 100)}%</span></div>
        <input type="range" className="range" min={ZOOM_MIN} max={ZOOM_MAX} step={0.05} value={zoom} aria-label="Масштаб" onChange={(e) => setZoom(Number(e.target.value))} />
        <div className="row">
          {[0.8, 0.9, 1, 1.1].map((z) => <button key={z} type="button" className="pill" aria-pressed={Math.abs(zoom - z) < 0.01} onClick={() => setZoom(z)}>{Math.round(z * 100)}%</button>)}
        </div>
        <span className="muted tiny" style={{ lineHeight: 1.5 }}>Быстро: Ctrl + колесо мыши, Ctrl и «+» / «−», Ctrl + 0 — вернуть 100%. Масштаб запоминается на этом компьютере.</span>
      </section>
      <section className="card" aria-label="Разделы">
        <div className="between">
          <span className="label">Витамины и лекарства</span>
          {medsOn != null && <button type="button" role="switch" aria-checked={medsOn} aria-label="Показывать витамины и лекарства" className="switch" onClick={toggleMeds}><span /></button>}
        </div>
        <span className="muted tiny" style={{ lineHeight: 1.5 }}>
          {medsOn === false
            ? 'Раздел скрыт: добавки не считаются в питании, тренер и план их не учитывают. Всё, что ты вносил, сохранено — включи, и оно вернётся.'
            : 'Раздел в «Здоровье», карточка на «Сегодня», витамины из добавок в «Питании», подсказки тренера по лекарствам. Можно скрыть, если не пользуешься.'}
        </span>
      </section>
      <section className="card" aria-label="Карта маршрутов">
        <div className="between">
          <span className="label">Карта под маршрутами</span>
          {map && <button type="button" role="switch" aria-checked={map.enabled} aria-label="Карта OpenStreetMap" className="switch" onClick={toggle}><span /></button>}
        </div>
        <span className="small" style={{ lineHeight: 1.55, color: 'var(--text-2)' }}>
          Подложка OpenStreetMap под треком бега и ходьбы. Kouros запрашивает у OpenStreetMap только квадраты карты (масштаб и номер квадрата) —
          сам трек, пульс и другие данные не уходят. Но по этим запросам видно, какие районы ты смотришь. Без карты маршрут показывается контуром.
        </span>
        {map && (
          <div className="between divider" style={{ paddingTop: 14 }}>
            <span className="muted small">Сохранено квадратов: {map.tiles.toLocaleString('ru-RU')} · {fmtBytes(map.bytes)} — уже открытые маршруты не требуют интернета</span>
            <button type="button" className="btn2" disabled={!map.tiles} onClick={clear}>Очистить</button>
          </div>
        )}
        {msg && <div className="note-err">{msg}</div>}
        <span className="muted tiny">Карта © участники OpenStreetMap. Квадраты хранятся в папке Kouros\cache\tiles.</span>
      </section>
    </div>
  );
}
