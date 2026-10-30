import { useState } from 'react';
import { Logo } from '../components/Logo';
import { api, errText } from '../lib/api';
import { isPhone } from '../lib/platform';

/** 2.0: on the phone the system fingerprint / face prompt, then the app opens. */
async function phoneCheck(): Promise<boolean> {
  const b = await import('@tauri-apps/plugin-biometric');
  try {
    const st = await b.checkStatus();
    if (!st.isAvailable) return true; // no fingerprint set up on the phone — the screen lock protects it
  } catch { return true; }
  try {
    await b.authenticate('Вход в Kouros', { allowDeviceCredential: true, cancelTitle: 'Отмена', title: 'Kouros' });
    return true;
  } catch { return false; }
}

export function Lock({ onUnlocked }: { onUnlocked: () => void }) {
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  async function go() {
    setBusy(true);
    setMsg(null);
    try {
      if (isPhone() && !(await phoneCheck())) { setMsg('Вход не подтверждён. Попробуй ещё раз.'); return; }
      if (await api.unlock()) onUnlocked();
      else setMsg('Вход не подтверждён. Попробуй ещё раз.');
    } catch (e) {
      setMsg(errText(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="lock">
      <div className="lock-card">
        <Logo size={72} />
        <span className="wordmark" style={{ fontSize: 26 }}>KOUROS</span>
        <span style={{ fontWeight: 600, marginTop: -6 }}>Твой тренер знает, как ты спал.</span>
        <span className="muted" style={{ lineHeight: 1.5 }}>
          {isPhone() ? 'Данные хранятся только на этом телефоне, в зашифрованной базе.' : 'Твои данные зашифрованы. Подтверди, что это ты, — PIN-кодом, отпечатком или лицом.'}
        </span>
        <button type="button" className="btn btn-green" onClick={go} disabled={busy} autoFocus>
          {isPhone() ? (busy ? 'Приложи палец…' : 'Войти по отпечатку') : busy ? 'Ждём Windows Hello…' : 'Войти через Windows Hello'}
        </button>
        {msg && <div className="note-warn" role="alert">{msg}</div>}
      </div>
    </div>
  );
}
