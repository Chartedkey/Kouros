// 2.0: the phone. `isPhone` — Kouros runs as the Android (or iOS) app; `useCompact` — the narrow
// layout (phone, or a narrow window / browser demo): bottom menu, compact header, rings in a row.
import { useEffect, useState } from 'react';

export const isPhone = (): boolean => typeof navigator !== 'undefined' && /Android|iPhone|iPad/i.test(navigator.userAgent);

const QUERY = '(max-width: 700px)';

export function compactNow(): boolean {
  if (isPhone()) return true;
  try { return window.matchMedia(QUERY).matches; } catch { return false; }
}

export function useCompact(): boolean {
  const [v, setV] = useState(compactNow);
  useEffect(() => {
    if (isPhone()) return;
    let mq: MediaQueryList;
    try { mq = window.matchMedia(QUERY); } catch { return; }
    const on = () => setV(mq.matches);
    mq.addEventListener('change', on);
    return () => mq.removeEventListener('change', on);
  }, []);
  useEffect(() => { document.documentElement.classList.toggle('compact', v); }, [v]);
  return v;
}
