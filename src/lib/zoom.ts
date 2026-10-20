// Interface scale: Settings slider, Ctrl + wheel, Ctrl + "+" / "−", Ctrl + 0.
// Kept per computer in local storage — it is a display preference, not data.
import { useEffect, useState } from 'react';

const KEY = 'kouros.zoom';
export const ZOOM_MIN = 0.7;
export const ZOOM_MAX = 1.3;
const listeners = new Set<(z: number) => void>();

function clampZoom(z: number) {
  return Math.round(Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, z)) * 20) / 20;
}

export function getZoom(): number {
  try {
    const v = Number(localStorage.getItem(KEY));
    return v ? clampZoom(v) : 1;
  } catch {
    return 1;
  }
}

export function setZoom(z: number) {
  const v = clampZoom(z);
  document.documentElement.style.setProperty('zoom', String(v));
  try { localStorage.setItem(KEY, String(v)); } catch { /* not important */ }
  listeners.forEach((f) => f(v));
}

/** Applies the saved scale and wires the keyboard and wheel shortcuts. Call once. */
export function initZoom() {
  setZoom(getZoom());
  window.addEventListener('keydown', (e) => {
    if (!(e.ctrlKey || e.metaKey)) return;
    if (e.key === '=' || e.key === '+') { e.preventDefault(); setZoom(getZoom() + 0.05); }
    else if (e.key === '-' || e.key === '_') { e.preventDefault(); setZoom(getZoom() - 0.05); }
    else if (e.key === '0') { e.preventDefault(); setZoom(1); }
  });
  window.addEventListener('wheel', (e) => {
    if (!e.ctrlKey) return;
    e.preventDefault();
    setZoom(getZoom() + (e.deltaY < 0 ? 0.05 : -0.05));
  }, { passive: false });
}

export function useZoom(): number {
  const [z, setZ] = useState(getZoom);
  useEffect(() => { listeners.add(setZ); return () => { listeners.delete(setZ); }; }, []);
  return z;
}
