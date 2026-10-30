import { Logo } from './Logo';
import { useBoard } from './BoardEditor';

export type Tab = 'today' | 'training' | 'health' | 'nutrition' | 'psychology' | 'coach' | 'settings' | 'help' | 'more';

export const TABS: { id: Exclude<Tab, 'settings' | 'help' | 'more'>; name: string }[] = [
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

/** 2.0, the phone: logo, date and the profile button; sections are in the bottom menu. */
function CompactHeader({ tab, onTab }: { tab: Tab; onTab: (t: Tab) => void }) {
  return (
    <header className="header header-c">
      <button type="button" className="brand" onClick={() => onTab('today')} aria-label="Kouros — на главную">
        <Logo />
        <span className="stack" style={{ gap: 0, alignItems: 'flex-start' }}>
          <span className="wordmark" style={{ fontSize: 15 }}>KOUROS</span>
          <span className="muted tiny">{today()}</span>
        </span>
      </button>
      <button type="button" className="avatar" aria-label="Настройки и профиль" aria-current={tab === 'settings' ? 'page' : undefined} onClick={() => onTab('settings')}>Д</button>
    </header>
  );
}

export function Header({ tab, onTab, compact }: { tab: Tab; onTab: (t: Tab) => void; compact?: boolean }) {
  const b = useBoard();
  if (compact) return <CompactHeader tab={tab} onTab={onTab} />;
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
          title="Справка: как пользоваться Kouros"
          aria-label="Открыть справку"
          aria-current={tab === 'help' ? 'page' : undefined}
          onClick={() => onTab('help')}
        >
          ?
        </button>
        <button
          type="button"
          className="avatar"
          title="Изменить расположение карточек на этой вкладке"
          aria-label="Изменить расположение карточек"
          aria-pressed={b.editing}
          onClick={() => b.setEditing(!b.editing)}
        >
          ⠿
        </button>
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
