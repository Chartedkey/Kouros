// 2.0, the phone: the menu at the bottom of the screen. Four sections you choose (kept on this
// device) and «Ещё», which lists everything else; nothing is ever out of reach.
import { useEffect, useState } from 'react';
import type { Tab } from './Header';

export type NavTab = Exclude<Tab, 'settings' | 'help' | 'more'>;

export const NAV_ALL: { id: NavTab; name: string }[] = [
  { id: 'today', name: 'Сегодня' },
  { id: 'health', name: 'Здоровье' },
  { id: 'training', name: 'Тренировки' },
  { id: 'nutrition', name: 'Питание' },
  { id: 'psychology', name: 'Психология' },
  { id: 'coach', name: 'Тренер' },
];

const KEY = 'kouros_bottom_nav';
const DEFAULT: NavTab[] = ['today', 'health', 'training', 'nutrition'];
const EVENT = 'kouros-bottom-nav';

export function navTabs(): NavTab[] {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? 'null') as NavTab[] | null;
    if (Array.isArray(v)) {
      const ok = v.filter((x) => NAV_ALL.some((n) => n.id === x)).slice(0, 4);
      if (ok.length) return ok;
    }
  } catch { /* no storage */ }
  return DEFAULT;
}

export function setNavTabs(v: NavTab[]) {
  try { localStorage.setItem(KEY, JSON.stringify(v.slice(0, 4))); } catch { /* no storage */ }
  window.dispatchEvent(new Event(EVENT));
}

export function useNavTabs(): NavTab[] {
  const [v, setV] = useState(navTabs);
  useEffect(() => {
    const on = () => setV(navTabs());
    window.addEventListener(EVENT, on);
    return () => window.removeEventListener(EVENT, on);
  }, []);
  return v;
}

function Icon({ id }: { id: NavTab | 'more' }) {
  const p = { width: 24, height: 24, viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', strokeWidth: 1.8, strokeLinecap: 'round' as const, strokeLinejoin: 'round' as const, 'aria-hidden': true };
  switch (id) {
    case 'today': return <svg {...p}><circle cx="12" cy="12" r="8" /><path d="M12 8v4l3 2" /></svg>;
    case 'health': return <svg {...p}><path d="M3 12h4l2-5 4 10 2-5h6" /></svg>;
    case 'training': return <svg {...p}><circle cx="14" cy="4.5" r="1.8" /><path d="M10 21l2-6 3 2v4M8 12l2-4 4 1 2 3 3 1" /></svg>;
    case 'nutrition': return <svg {...p}><path d="M7 3v8a3 3 0 0 0 3 3v7M7 3v5M10 3v5M17 3c-2 2-2 6 0 8v10" /></svg>;
    case 'psychology': return <svg {...p}><circle cx="12" cy="12" r="8.5" /><path d="M8.5 14.5c1 1.2 2.1 1.8 3.5 1.8s2.5-.6 3.5-1.8M9 9.5h.01M15 9.5h.01" /></svg>;
    case 'coach': return <svg {...p}><path d="M4 18V6a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H8l-4 4z" /><path d="M8 9h8M8 12h5" /></svg>;
    default: return <svg {...p}><circle cx="5" cy="12" r="1.5" /><circle cx="12" cy="12" r="1.5" /><circle cx="19" cy="12" r="1.5" /></svg>;
  }
}

export function BottomNav({ tab, onTab }: { tab: Tab; onTab: (t: Tab) => void }) {
  const tabs = useNavTabs();
  const inMore = !tabs.includes(tab as NavTab);
  return (
    <nav className="bnav" aria-label="Разделы">
      {tabs.map((id) => (
        <button key={id} type="button" aria-current={tab === id ? 'page' : undefined} onClick={() => onTab(id)}>
          <Icon id={id} />
          <span>{NAV_ALL.find((n) => n.id === id)?.name}</span>
        </button>
      ))}
      <button type="button" aria-current={inMore ? 'page' : undefined} onClick={() => onTab('more')}>
        <Icon id="more" />
        <span>Ещё</span>
      </button>
    </nav>
  );
}
