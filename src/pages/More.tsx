// 2.0, the phone: «Ещё» — the bottom menu editor and every section that is not in the menu.
import type { Tab } from '../components/Header';
import { NAV_ALL, setNavTabs, useNavTabs, type NavTab } from '../components/BottomNav';
import type { Go } from '../lib/nav';

const EXTRA: { label: string; go: Go }[] = [
  { label: 'План тренировок', go: { tab: 'coach', sub: 'plan' } },
  { label: 'Витамины и лекарства', go: { tab: 'health', sub: 'meds' } },
  { label: 'Анализы', go: { tab: 'health', sub: 'labs' } },
  { label: 'Откуда брать данные', go: { tab: 'settings', card: 'Откуда брать данные' } },
];

export function More({ onTab, go }: { onTab: (t: Tab) => void; go: (g: Go) => void }) {
  const tabs = useNavTabs();
  const rest = NAV_ALL.filter((n) => !tabs.includes(n.id));
  const full = tabs.length >= 4;
  const remove = (id: NavTab) => { if (tabs.length > 1) setNavTabs(tabs.filter((x) => x !== id)); };
  const add = (id: NavTab) => { if (!full) setNavTabs([...tabs, id]); };
  return (
    <>
      <div className="page-head"><h1>Ещё</h1></div>
      <section className="card" aria-label="Нижнее меню">
        <div className="stack" style={{ gap: 2 }}>
          <span className="label">Нижнее меню</span>
          <span className="muted small">Выбери до 4 разделов. «Ещё» всегда последний.</span>
        </div>
        <div className="more-slots">
          {tabs.map((id, i) => (
            <button key={id} type="button" className="more-slot" onClick={() => remove(id)} disabled={tabs.length <= 1} aria-label={`Убрать «${NAV_ALL.find((n) => n.id === id)?.name}» из меню`}>
              <span className="tiny" style={{ color: 'var(--green)' }}>{i + 1}</span>
              <b className="small">{NAV_ALL.find((n) => n.id === id)?.name}</b>
              <span className="tiny muted">убрать</span>
            </button>
          ))}
        </div>
        {rest.length > 0 && (
          <div className="row" style={{ gap: 6, flexWrap: 'wrap' }}>
            {rest.map((n) => <button key={n.id} type="button" className="pill sm" disabled={full} onClick={() => add(n.id)}>+ {n.name}</button>)}
          </div>
        )}
        <span className="muted tiny">{full ? 'Чтобы поставить другой раздел, сначала убери один из четырёх.' : 'Нажми «+», чтобы поставить раздел в меню.'}</span>
      </section>
      <section className="card more-list" aria-label="Все разделы">
        {rest.map((n) => (
          <button key={n.id} type="button" className="more-row" onClick={() => onTab(n.id)}>{n.name}<span aria-hidden="true">›</span></button>
        ))}
        {EXTRA.map((x) => (
          <button key={x.label} type="button" className="more-row" onClick={() => go(x.go)}>{x.label}<span aria-hidden="true">›</span></button>
        ))}
        <button type="button" className="more-row" onClick={() => onTab('settings')}>Настройки и профиль<span aria-hidden="true">›</span></button>
        <button type="button" className="more-row" onClick={() => onTab('help')}>Справка<span aria-hidden="true">›</span></button>
      </section>
    </>
  );
}
