// Auto-update (0.19): Kouros looks for a newer release on GitHub, shows «что нового» from the
// release description and installs it with one button (signed — the app checks the signature).
import { useEffect, useState } from 'react';
import { errText } from '../lib/api';
import { isPhone } from '../lib/platform';

const inApp = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in (window as object);
const LAST = isPhone() ? 'kouros_android_update_checked_v2' : 'kouros_update_checked';
const AUTO = 'kouros_update_auto';
const PENDING = 'kouros_android_update_pending';

type Found = { version: string; notes: string; install: (onProgress: (pct: number | null) => void) => Promise<void> };

export function autoCheckOn(): boolean {
  try { return localStorage.getItem(AUTO) !== '0'; } catch { return true; }
}
export function setAutoCheck(on: boolean) {
  try { localStorage.setItem(AUTO, on ? '1' : '0'); } catch { /* no storage */ }
  window.dispatchEvent(new Event('kouros-update-preference'));
}

/** null = no newer version. */
export async function checkUpdate(): Promise<Found | null> {
  if (!inApp()) return null;
  if (isPhone()) {
    // Android downloads inside the app, verifies the artifact and asks the system installer.
    // No external browser is needed after the one-time alpha.1 -> alpha.2 transition.
    const { invoke } = await import('@tauri-apps/api/core');
    const r = await invoke<{ version: string; notes: string; url: string } | null>('mobile_update_check');
    if (!r) { try { localStorage.removeItem(PENDING); } catch { /* no storage */ } return null; }
    return {
      version: r.version,
      notes: r.notes,
      install: async (onProgress) => {
        onProgress(null);
        try { localStorage.setItem(PENDING, '1'); } catch { /* no storage */ }
        await invoke('mobile_update_install');
      },
    };
  }
  const { check } = await import('@tauri-apps/plugin-updater');
  const u = await check();
  if (!u) return null;
  return {
    version: u.version,
    notes: u.body ?? '',
    install: async (onProgress) => {
      let total = 0;
      let got = 0;
      await u.downloadAndInstall((e) => {
        if (e.event === 'Started') { total = e.data.contentLength ?? 0; onProgress(total ? 0 : null); }
        if (e.event === 'Progress') { got += e.data.chunkLength; onProgress(total ? Math.round((got / total) * 100) : null); }
      });
      const { relaunch } = await import('@tauri-apps/plugin-process');
      await relaunch();
    },
  };
}

/** Release description (markdown from CHANGELOG.md) as simple lines. */
function Notes({ text }: { text: string }) {
  const lines = text.split('\n').map((l) => l.trim()).filter((l) => l && !l.startsWith('**Установка'));
  return (
    <div className="stack" style={{ gap: 3, maxHeight: 260, overflowY: 'auto' }}>
      {lines.map((l, i) => {
        const head = /^\*\*(.+)\*\*$/.exec(l);
        if (head) return <b key={i} className="small" style={{ paddingTop: 6 }}>{head[1]}</b>;
        return <span key={i} className="small" style={{ lineHeight: 1.5 }}>{l.replace(/^- /, '• ').replace(/\*\*/g, '').replace(/`/g, '')}</span>;
      })}
    </div>
  );
}

function UpdateCard({ u, onLater }: { u: Found; onLater?: () => void }) {
  const [open, setOpen] = useState(false);
  const [pct, setPct] = useState<number | null | undefined>(undefined);
  const [err, setErr] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const go = async () => {
    setErr(null); setNotice(null); setPct(null);
    try { await u.install(setPct); if (isPhone()) { setPct(undefined); setNotice("Подтвердите установку в системном окне Android."); } } catch (e) { setErr(errText(e)); setPct(undefined); }
  };
  return (
    <section className="card" aria-label="Обновление" style={{ borderColor: '#3DDC9755' }}>
      <div className="between" style={{ flexWrap: 'wrap', gap: 10 }}>
        <span className="label" style={{ color: '#3DDC97' }}>Доступна версия {u.version}</span>
        <span className="row" style={{ gap: 8 }}>
          {u.notes && <button type="button" className="linkbtn small" aria-expanded={open} onClick={() => setOpen(!open)}>{open ? 'Скрыть' : 'Что нового'}</button>}
          {onLater && pct === undefined && <button type="button" className="btn2" onClick={onLater}>Позже</button>}
          <button type="button" className="btn" disabled={pct !== undefined} onClick={go}>
            {pct === undefined ? 'Обновить' : pct === null ? 'Скачиваю…' : `Скачиваю ${pct}%`}
          </button>
        </span>
      </div>
      {open && <Notes text={u.notes} />}
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>{isPhone()
        ? 'Kouros скачает и проверит APK. Android попросит подтвердить установку поверх приложения; данные сохранятся.'
        : 'Данные не трогаются: обновится только программа, после установки Kouros перезапустится сам.'}</span>
      {notice && <div role="status" className="muted small">{notice}</div>}
      {err && <div className="note-err">Не получилось обновить: {err}. Можно скачать установщик вручную со страницы Releases.</div>}
    </section>
  );
}

/** Automatic checks while the app is open; failures remain retryable. */
export function UpdateBanner() {
  const [u, setU] = useState<Found | null>(null);
  const [auto, setAuto] = useState(autoCheckOn());
  useEffect(() => {
    const changed = () => setAuto(autoCheckOn());
    window.addEventListener('kouros-update-preference', changed);
    return () => window.removeEventListener('kouros-update-preference', changed);
  }, []);
  useEffect(() => {
    if (!inApp()) return;
    let pending = false;
    try { pending = isPhone() && localStorage.getItem(PENDING) === '1'; } catch { /* no storage */ }
    if (!auto && !pending) return;
    let last = 0;
    try { last = Number(localStorage.getItem(LAST) ?? 0); } catch { /* no storage */ }
    const interval = isPhone() ? 30 * 60 * 1000 : 12 * 3600 * 1000;
    if (!isPhone() && !pending && Date.now() - last < interval) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout>;
    const run = async () => {
      try {
        const v = await checkUpdate();
        if (cancelled) return;
        try { localStorage.setItem(LAST, String(Date.now())); } catch { /* no storage */ }
        setU(v);
      } catch { /* A temporary network failure is retried on the next interval. */ }
      finally { if (!cancelled && isPhone() && auto) timer = setTimeout(run, interval); }
    };
    const delay = pending ? 4000 : Math.max(4000, Math.min(interval, interval - (Date.now() - last)));
    timer = setTimeout(run, delay);
    return () => { cancelled = true; clearTimeout(timer); };
  }, [auto]);
  if (!u) return null;
  return <UpdateCard u={u} onLater={() => { try { localStorage.removeItem(PENDING); } catch { /* no storage */ } setU(null); }} />;
}

/** Settings → «О приложении». */
export function UpdatesSection() {
  const [state, setState] = useState<'idle' | 'checking' | 'none' | 'found' | 'error'>('idle');
  const [u, setU] = useState<Found | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [auto, setAuto] = useState(autoCheckOn());
  const run = async () => {
    setState('checking'); setMsg(null);
    try {
      const v = await checkUpdate();
      setU(v); setState(v ? 'found' : 'none');
    } catch (e) { setMsg(errText(e)); setState('error'); }
  };
  return (
    <section className="card" aria-label="Обновления">
      <div className="between">
        <span className="label">Обновления</span>
        <button type="button" className="btn2" disabled={state === 'checking' || !inApp()} onClick={run}>{state === 'checking' ? 'Проверяю…' : 'Проверить обновления'}</button>
      </div>
      <label className="row small" style={{ gap: 8, alignItems: 'center' }}>
        <input type="checkbox" checked={auto} onChange={(e) => { setAuto(e.target.checked); setAutoCheck(e.target.checked); }} />
        {isPhone() ? 'Проверять автоматически при запуске и каждые 30 минут работы' : 'Проверять при запуске (раз в 12 часов)'}
      </label>
      {state === 'none' && <span className="small">У тебя последняя версия.</span>}
      {state === 'error' && <span className="note-err">Не удалось проверить: {msg}</span>}
      {!inApp() && <span className="muted tiny">В демо-режиме в браузере обновления не проверяются.</span>}
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>Kouros спрашивает у GitHub только номер последней версии. Обновление подписано — программа проверяет подпись перед установкой.</span>
      {state === 'found' && u && <UpdateCard u={u} />}
    </section>
  );
}
