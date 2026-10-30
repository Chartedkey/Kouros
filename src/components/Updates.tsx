// Auto-update (0.19): Kouros looks for a newer release on GitHub, shows «что нового» from the
// release description and installs it with one button (signed — the app checks the signature).
import { useEffect, useState } from 'react';
import { errText } from '../lib/api';
import { isPhone } from '../lib/platform';

const inApp = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in (window as object);
const LAST = 'kouros_update_checked';
const AUTO = 'kouros_update_auto';

type Found = { version: string; notes: string; install: (onProgress: (pct: number | null) => void) => Promise<void> };

export function autoCheckOn(): boolean {
  try { return localStorage.getItem(AUTO) !== '0'; } catch { return true; }
}
export function setAutoCheck(on: boolean) {
  try { localStorage.setItem(AUTO, on ? '1' : '0'); } catch { /* no storage */ }
}

/** null = no newer version. */
export async function checkUpdate(): Promise<Found | null> {
  if (!inApp()) return null;
  if (isPhone()) {
    // 2.0: the phone has no signed auto-updater — the new APK opens in the browser to download,
    // then Android installs it over the old one (same signing key, data stays).
    const { invoke } = await import('@tauri-apps/api/core');
    const r = await invoke<{ version: string; notes: string; url: string } | null>('mobile_update_check');
    if (!r) return null;
    return {
      version: r.version,
      notes: r.notes,
      install: async (onProgress) => {
        onProgress(null);
        await invoke('open_link', { url: r.url });
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
  const go = async () => {
    setErr(null); setPct(null);
    try { await u.install(setPct); } catch (e) { setErr(errText(e)); setPct(undefined); }
  };
  return (
    <section className="card" aria-label="Обновление" style={{ borderColor: '#3DDC9755' }}>
      <div className="between" style={{ flexWrap: 'wrap', gap: 10 }}>
        <span className="label" style={{ color: '#3DDC97' }}>Доступна версия {u.version}</span>
        <span className="row" style={{ gap: 8 }}>
          {u.notes && <button type="button" className="linkbtn small" aria-expanded={open} onClick={() => setOpen(!open)}>{open ? 'Скрыть' : 'Что нового'}</button>}
          {onLater && pct === undefined && <button type="button" className="btn2" onClick={onLater}>Позже</button>}
          <button type="button" className="btn" disabled={pct !== undefined} onClick={go}>
            {pct === undefined ? 'Обновить' : pct === null ? (isPhone() ? 'Открываю…' : 'Скачиваю…') : `Скачиваю ${pct}%`}
          </button>
        </span>
      </div>
      {open && <Notes text={u.notes} />}
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>{isPhone()
        ? 'Откроется скачивание нового APK. Открой файл — Android поставит его поверх, данные сохранятся.'
        : 'Данные не трогаются: обновится только программа, после установки Kouros перезапустится сам.'}</span>
      {err && <div className="note-err">Не получилось обновить: {err}. Можно скачать установщик вручную со страницы Releases.</div>}
    </section>
  );
}

/** Banner at the top of the app: checks once in 12 hours when auto-check is on. */
export function UpdateBanner() {
  const [u, setU] = useState<Found | null>(null);
  useEffect(() => {
    if (!inApp() || !autoCheckOn()) return;
    let last = 0;
    try { last = Number(localStorage.getItem(LAST) ?? 0); } catch { /* no storage */ }
    if (Date.now() - last < 12 * 3600 * 1000) return;
    const t = setTimeout(() => {
      checkUpdate()
        .then((v) => { try { localStorage.setItem(LAST, String(Date.now())); } catch { /* no storage */ } setU(v); })
        .catch(() => {});
    }, 4000);
    return () => clearTimeout(t);
  }, []);
  if (!u) return null;
  return <UpdateCard u={u} onLater={() => setU(null)} />;
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
        Проверять при запуске (раз в 12 часов)
      </label>
      {state === 'none' && <span className="small">У тебя последняя версия.</span>}
      {state === 'error' && <span className="note-err">Не удалось проверить: {msg}</span>}
      {!inApp() && <span className="muted tiny">В демо-режиме в браузере обновления не проверяются.</span>}
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>Kouros спрашивает у GitHub только номер последней версии. Обновление подписано — программа проверяет подпись перед установкой.</span>
      {state === 'found' && u && <UpdateCard u={u} />}
    </section>
  );
}
