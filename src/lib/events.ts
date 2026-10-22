import { useEffect } from 'react';
import { inTauri } from './api';

/** Calls `cb` whenever the backend reports new data (sync or import finished). */
export function useDataChanged(cb: () => void) {
  useEffect(() => {
    if (!inTauri) return;
    let un: (() => void) | undefined;
    let alive = true;
    import('@tauri-apps/api/event').then(({ listen }) =>
      listen('data-changed', () => cb()).then((f) => { if (alive) un = f; else f(); }),
    );
    return () => { alive = false; un?.(); };
  }, [cb]);
}

/** Calls `cb` with the payload of a named backend event. */
export function useBackendEvent<T>(name: string, cb: (payload: T) => void) {
  useEffect(() => {
    if (!inTauri) return;
    let un: (() => void) | undefined;
    let alive = true;
    import('@tauri-apps/api/event').then(({ listen }) =>
      listen<T>(name, (e) => cb(e.payload)).then((f) => { if (alive) un = f; else f(); }),
    );
    return () => { alive = false; un?.(); };
  }, [name, cb]);
}
