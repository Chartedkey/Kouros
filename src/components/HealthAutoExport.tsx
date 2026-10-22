// «Health Auto Export» (1.3): Apple Health straight from the iPhone — a receiver on the home
// network (address + key, QR check), a folder as the spare road, history from a file, and the
// switch of all Apple data from freddy to Health Auto Export (from today or replacing the past).
import { useCallback, useEffect, useMemo, useState } from 'react';
import { api, errText, inTauri, type HaeImport, type HaeState } from '../lib/api';
import { useBackendEvent, useDataChanged } from '../lib/events';
import { openHelp } from '../lib/helptips';
import { qrMatrix } from '../lib/qr';

const ruDate = (d: string) => new Date(`${d}T12:00:00`).toLocaleDateString('ru-RU', { day: 'numeric', month: 'short', year: 'numeric' });
const ago = (ts: number) => {
  const m = Math.round((Date.now() / 1000 - ts) / 60);
  if (m < 1) return 'только что';
  if (m < 60) return `${m} мин назад`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} ч назад`;
  return new Date(ts * 1000).toLocaleString('ru-RU', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
};
const n0 = (n: number) => n.toLocaleString('ru-RU');

function Qr({ text, size = 168 }: { text: string; size?: number }) {
  const m = useMemo(() => qrMatrix(text), [text]);
  if (!m) return null;
  const q = 4;
  const n = m.length + q * 2;
  let d = '';
  m.forEach((row, y) => row.forEach((c, x) => { if (c) d += `M${x + q} ${y + q}h1v1h-1z`; }));
  return (
    <svg width={size} height={size} viewBox={`0 0 ${n} ${n}`} shapeRendering="crispEdges" role="img" aria-label="QR-код для проверки связи" style={{ borderRadius: 10, flex: 'none' }}>
      <rect width={n} height={n} fill="#fff" />
      <path d={d} fill="#000" />
    </svg>
  );
}

function Copy({ text }: { text: string }) {
  const [ok, setOk] = useState(false);
  return (
    <button type="button" className="btn2" onClick={() => navigator.clipboard?.writeText(text).then(() => { setOk(true); setTimeout(() => setOk(false), 1500); })}>
      {ok ? 'Скопировано ✓' : 'Скопировать'}
    </button>
  );
}

async function pickFile(): Promise<string | null> {
  if (!inTauri) return 'HealthAutoExport.zip';
  const { open } = await import('@tauri-apps/plugin-dialog');
  const r = await open({ multiple: false, title: 'Выгрузка Health Auto Export', filters: [{ name: 'Health Auto Export', extensions: ['zip', 'json'] }] });
  return typeof r === 'string' ? r : null;
}
async function pickFolder(): Promise<string | null> {
  if (!inTauri) return 'G:\\Мой диск\\Health Auto Export';
  const { open } = await import('@tauri-apps/plugin-dialog');
  const r = await open({ directory: true, multiple: false, title: 'Папка, куда Health Auto Export сохраняет файлы' });
  return typeof r === 'string' ? r : null;
}

function Switch({ st, onDone }: { st: HaeState; onDone: () => void }) {
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const onHae = st.groups.filter((g) => g.current === 'HAE').length;
  const apple = st.groups.filter((g) => g.current === 'APPLE_HEALTH').length;
  const run = async (provider: string, mode: 'keep' | 'replace') => {
    setBusy(true); setErr(null);
    try { await api.sourcesSetAll(provider, mode); onDone(); } catch (e) { setErr(errText(e)); } finally { setBusy(false); }
  };
  if (!st.data.rows) return null;
  if (apple === 0 && onHae > 0) {
    return (
      <div className="note-ok" style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
        <span>Apple Health сейчас приходит через Health Auto Export. freddy остаётся запасным путём: если телефон перестанет присылать данные, верни его одной кнопкой.</span>
        <button type="button" className="linkbtn small" style={{ alignSelf: 'flex-start' }} disabled={busy} onClick={() => run('APPLE_HEALTH', 'keep')}>Вернуть Apple Health через freddy</button>
        {err && <div className="note-err">{err}</div>}
      </div>
    );
  }
  return (
    <div className="note-warn" style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
      <span>
        <b>Перейти на Health Auto Export?</b> Есть данные{st.data.first ? ` с ${ruDate(st.data.first)}` : ''}{st.data.last ? ` по ${ruDate(st.data.last)}` : ''}. Это те же цифры Apple Health, только без посредника — личные нормы не сбрасываются.
      </span>
      <div className="row" style={{ gap: 10, flexWrap: 'wrap' }}>
        <button type="button" className="btn" disabled={busy} onClick={() => run('HAE', 'keep')}>С сегодняшнего дня</button>
        <button type="button" className="btn2" disabled={busy} onClick={() => run('HAE', 'replace')}>Заменить прошлое, где есть выгрузка</button>
      </div>
      <span className="tiny" style={{ color: '#c9b78f', lineHeight: 1.5 }}>
        «С сегодняшнего дня» — прошлое остаётся из freddy. «Заменить прошлое» — дни, за которые загружена выгрузка, Kouros возьмёт из Health Auto Export и заново посчитает готовность за них; более ранние дни останутся из freddy.
        Группы, которые ты брал из Garmin или Suunto, не меняются. Вернуться можно в любой момент — ниже, в «Откуда брать данные».
      </span>
      {err && <div className="note-err">{err}</div>}
    </div>
  );
}

/** What has arrived from the phone; important data missing for 3+ days gets a hint. */
function Coverage({ st }: { st: HaeState }) {
  if (!st.coverage.length) return null;
  const today = new Date();
  const days = (d: string) => Math.round((today.getTime() - new Date(`${d}T12:00:00`).getTime()) / 86400000);
  const missing = st.coverage.filter((c) => c.important && (!c.last || days(c.last) > 3));
  return (
    <div className="stack" style={{ gap: 6, marginTop: 4 }}>
      <div className="row" style={{ gap: '6px 8px', flexWrap: 'wrap' }}>
        {st.coverage.map((c) => {
          const ok = !!c.last && days(c.last) <= 3;
          const col = ok ? 'var(--green)' : c.important ? '#F5B642' : 'var(--muted)';
          return (
            <span key={c.key} className="tiny" title={c.last ? `последние данные — ${ruDate(c.last)}` : 'не приходило'}
              style={{ padding: '4px 9px', borderRadius: 999, border: `1px solid ${ok ? 'var(--green-line)' : 'var(--line-2)'}`, color: col, whiteSpace: 'nowrap' }}>
              {ok ? '✓' : c.last ? '…' : '—'} {c.name}
            </span>
          );
        })}
      </div>
      {missing.length > 0 && (
        <span className="tiny" style={{ color: '#F5B642', lineHeight: 1.5 }}>
          Не приходит: {missing.map((c) => c.name.toLowerCase()).join(', ')}. Проверь, что в автоматизации выбраны все показатели, а на iPhone у Health Auto Export есть доступ к ним: Настройки → Здоровье → Доступ к данным и устройства → Health Auto Export → «Включить все».
        </span>
      )}
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>«—» — не приходило. Если эти данные у тебя есть в Здоровье, проверь доступ Health Auto Export: Настройки → Здоровье → Доступ к данным и устройства → Health Auto Export.</span>
    </div>
  );
}

export function HaeCard() {
  const [st, setSt] = useState<HaeState | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [port, setPort] = useState('');
  const [folder, setFolder] = useState('');
  const [prog, setProg] = useState<HaeState['progress']>(null);
  const [done, setDone] = useState<HaeImport | null>(null);
  const [scanMsg, setScanMsg] = useState<string | null>(null);

  const load = useCallback(() => {
    api.haeState().then((s) => { setSt(s); setPort(String(s.port)); setFolder(s.folder); }).catch((e) => setErr(errText(e)));
  }, []);
  useEffect(load, [load]);
  useDataChanged(load);
  useBackendEvent<string>('hae-status', useCallback(() => load(), [load]));
  useBackendEvent<HaeState['progress']>('hae-progress', useCallback((p) => setProg(p), []));
  // While the phone check is pending, look for it every few seconds.
  useEffect(() => {
    if (!st?.enabled || st.lastCheck) return;
    const t = setInterval(load, 5000);
    return () => clearInterval(t);
  }, [st?.enabled, st?.lastCheck, load]);

  const patch = async (p: Parameters<typeof api.haeSet>[0]) => {
    setBusy(true); setErr(null);
    try { setSt(await api.haeSet(p)); } catch (e) { setErr(errText(e)); } finally { setBusy(false); }
  };
  const importFile = async () => {
    const path = await pickFile();
    if (!path) return;
    setBusy(true); setErr(null); setDone(null); setProg({ label: 'file', samples: 0, first: null, last: null });
    try { setDone(await api.haeImportFile(path)); load(); } catch (e) { setErr(errText(e)); } finally { setBusy(false); setProg(null); }
  };
  const scan = async () => {
    setScanMsg(null);
    try { const n = await api.haeScan(); setScanMsg(n ? `Прочитано файлов: ${n}` : 'Новых файлов нет'); load(); } catch (e) { setErr(errText(e)); }
  };

  if (!st) return err ? <div className="note-err">{err}</div> : null;
  const portNum = Number(port);
  return (
    <section className="card" aria-label="Health Auto Export">
      <div className="between" style={{ flexWrap: 'wrap' }}>
        <span className="label">Health Auto Export</span>
        <span className={`chip ${st.enabled && st.running ? 'chip-ok' : 'chip-off'}`}>{!st.enabled ? 'выключено' : st.running ? 'принимает данные' : 'запускается…'}</span>
      </div>
      <span className="muted small" style={{ lineHeight: 1.5 }}>
        Приложение для iPhone, которое отправляет данные Apple Health прямо на этот компьютер по домашнему Wi-Fi — без freddy и без облака.
        Приходит то же, что и через freddy (сон по фазам, пульс, HRV, еда YAZIO, вода, кофеин), и больше: беговая динамика и походка.
      </span>
      <div className="row" style={{ gap: 10, flexWrap: 'wrap' }}>
        <button type="button" className={st.enabled ? 'btn2' : 'btn btn-green'} disabled={busy} onClick={() => patch({ enabled: !st.enabled })}>
          {st.enabled ? 'Выключить приём' : 'Включить приём с iPhone'}
        </button>
        <button type="button" className="linkbtn small" onClick={() => openHelp('settings.hae')}>Как настроить на iPhone</button>
      </div>
      {st.enabled && (
        <>
          {st.error && <div className="note-err">{st.error}</div>}
          {!st.ip && <div className="note-warn">Не нашёл адрес компьютера в домашней сети. Проверь, что компьютер подключён к роутеру (кабелем или по Wi-Fi), и обнови страницу.</div>}
          {st.url && (
            <div className="inset" style={{ gap: 12 }}>
              <div className="row" style={{ gap: 16, alignItems: 'flex-start', flexWrap: 'wrap' }}>
                {st.checkUrl && <Qr text={st.checkUrl} />}
                <div className="stack" style={{ gap: 8, flex: '1 1 260px', minWidth: 0 }}>
                  <b className="small">1. Проверь связь</b>
                  <span className="small" style={{ lineHeight: 1.5 }}>
                    Наведи камеру iPhone на код и открой ссылку. Если появится «Всё работает» — телефон видит компьютер.
                    Если страница не открывается — Windows могла спросить разрешение для Kouros: разреши доступ в <b>частных сетях</b>.
                  </span>
                  {st.lastCheck
                    ? <span className="small" style={{ color: 'var(--green)' }}>✓ Телефон открыл проверку {ago(st.lastCheck)}</span>
                    : <span className="muted tiny">Ждём, когда телефон откроет страницу…</span>}
                </div>
              </div>
              <div className="stack" style={{ gap: 6 }}>
                <b className="small">2. Адрес для Health Auto Export</b>
                <span className="muted tiny">Автоматизации → новая → REST API → вставь в поле URL (ключ уже внутри). Показатели — выбери все: Kouros сам возьмёт нужное.</span>
                <div className="row" style={{ gap: 8, flexWrap: 'wrap' }}>
                  <code className="mono small" style={{ flex: '1 1 260px', minWidth: 0, padding: '10px 12px', borderRadius: 10, background: 'var(--bg)', border: '1px solid var(--line-2)', overflowWrap: 'anywhere' }}>{st.url}</code>
                  <Copy text={st.url} />
                </div>
              </div>
              <div className="stack" style={{ gap: 4 }}>
                <b className="small">3. Что пришло</b>
                <span className="small">
                  {st.lastPush ? `Последние данные — ${ago(st.lastPush)}.` : 'С телефона пока ничего не приходило.'}
                  {st.data.rows > 0 && st.data.first && st.data.last ? ` Всего записано с ${ruDate(st.data.first)} по ${ruDate(st.data.last)}.` : ''}
                  {st.waiting > 0 ? ` Ждут разбора: ${st.waiting}.` : ''}
                </span>
                <Coverage st={st} />
              </div>
              <div className="row" style={{ gap: 8, flexWrap: 'wrap', alignItems: 'center' }}>
                <span className="muted tiny">Порт</span>
                <input className="input" style={{ maxWidth: 110, height: 36 }} inputMode="numeric" value={port} onChange={(e) => setPort(e.target.value.replace(/\D/g, ''))} />
                {portNum !== st.port && <button type="button" className="btn2" disabled={busy || portNum < 1024 || portNum > 65535} onClick={() => patch({ port: portNum })}>Сохранить порт</button>}
                <button type="button" className="linkbtn tiny" disabled={busy} onClick={() => { if (window.confirm('Сделать новый ключ? Старый адрес перестанет работать — его нужно будет заменить в Health Auto Export.')) patch({ newToken: true }); }}>Новый ключ</button>
              </div>
            </div>
          )}
        </>
      )}

      <div className="inset" style={{ gap: 8 }}>
        <b className="small">Запасной путь — папка</b>
        <span className="muted tiny" style={{ lineHeight: 1.5 }}>
          Если телефон и компьютер не видят друг друга (гостевая сеть, работа, поездка): в Health Auto Export выбери экспорт в Google Drive, Dropbox или iCloud Drive, а здесь — эту папку на компьютере. Kouros проверяет её раз в минуту; файлы он только читает.
        </span>
        <div className="row" style={{ gap: 8, flexWrap: 'wrap' }}>
          <input className="input" style={{ flex: '1 1 260px', height: 38 }} placeholder="Папка не выбрана" value={folder} onChange={(e) => setFolder(e.target.value)} onBlur={() => folder !== st.folder && patch({ folder })} />
          <button type="button" className="btn2" disabled={busy} onClick={async () => { const f = await pickFolder(); if (f) { setFolder(f); patch({ folder: f }); } }}>Выбрать папку</button>
          {st.folder && <button type="button" className="btn2" disabled={busy} onClick={scan}>Проверить сейчас</button>}
          {st.folder && <button type="button" className="linkbtn tiny" disabled={busy} onClick={() => { setFolder(''); patch({ folder: '' }); }}>Не использовать</button>}
        </div>
        {scanMsg && <span className="muted tiny">{scanMsg}</span>}
      </div>

      <div className="inset" style={{ gap: 8 }}>
        <b className="small">История из выгрузки</b>
        <span className="muted tiny" style={{ lineHeight: 1.5 }}>
          ZIP или JSON из «Ручной загрузки» Health Auto Export (все показатели). Выгружай по неделе — так телефон справится быстрее; можно загружать повторно, одинаковые записи не задвоятся.
        </span>
        <div className="row" style={{ gap: 10, flexWrap: 'wrap', alignItems: 'center' }}>
          <button type="button" className="btn2" disabled={busy} onClick={importFile}>Загрузить выгрузку…</button>
          {prog && <span className="small">Читаю… {n0(prog.samples)} замеров{prog.first ? ` · ${ruDate(prog.first)} — ${ruDate(prog.last ?? prog.first)}` : ''}</span>}
          {done && !prog && (
            <span className="small" style={{ color: 'var(--green)' }}>
              ✓ Загружено {n0(done.samples)} замеров{done.first ? ` с ${ruDate(done.first)} по ${ruDate(done.last ?? done.first)}` : ''}
            </span>
          )}
        </div>
      </div>

      <Switch st={st} onDone={load} />
      {err && <div className="note-err">{err}</div>}
    </section>
  );
}
