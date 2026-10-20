import { Logo } from './Logo';

export type Tab = 'today' | 'training' | 'health' | 'nutrition' | 'psychology' | 'coach' | 'settings';

export const TABS: { id: Exclude<Tab, 'settings'>; name: string }[] = [
  { id: 'today', name: 'Сегодня' },
  { id: 'training', name: 'Тренировки' },
  { id: 'health', name: 'Здоровье' },
  { id: 'nutrition', name: 'Питание' },
  { id: 'psychology', name: 'Психология' },
  { id: 'coach', name: 'Тренер' },
];

function today(): string {
  const s = new Date().toLocaleDateString('ru-RU', { weekday: 'long', day: 'numeric', month: 'long' });
  return s.charAt(0).toUpperCase() + s.slice(1);
}

export function Header({ tab, onTab }: { tab: Tab; onTab: (t: Tab) => void }) {
  return (
    <header className="header">
      <button type="button" className="brand" onClick={() => onTab('today')} aria-label="Kouros — на главную">
        <Logo />
        <span className="wordmark">KOUROS</span>
      </button>
      <nav className="nav" aria-label="Разделы">
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            aria-current={tab === t.id ? 'page' : undefined}
            onClick={() => onTab(t.id)}
          >
            {t.name}
          </button>
        ))}
      </nav>
      <div className="header-right">
        <span className="muted small">{today()}</span>
        <button
          type="button"
          className="avatar"
          aria-label="Настройки и профиль"
          aria-current={tab === 'settings' ? 'page' : undefined}
          onClick={() => onTab('settings')}
        >
          Д
        </button>
      </div>
    </header>
  );
}
