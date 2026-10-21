// The small help window opened by «?» on a card (1.1).
import { useEffect, useState } from 'react';
import { helpSection } from '../help/content';
import { HelpText } from '../help/HelpText';
import { HELP_EVENT, openHelpPage } from '../lib/helptips';

export function HelpLayer() {
  const [id, setId] = useState<string | null>(null);
  useEffect(() => {
    const on = (e: Event) => setId((e as CustomEvent<string>).detail);
    window.addEventListener(HELP_EVENT, on);
    return () => window.removeEventListener(HELP_EVENT, on);
  }, []);
  useEffect(() => {
    if (!id) return;
    const key = (e: KeyboardEvent) => { if (e.key === 'Escape') setId(null); };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, [id]);
  const hit = id ? helpSection(id) : null;
  if (!hit) return null;
  return (
    <div className="ex-modal" role="dialog" aria-modal="true" aria-label={hit.section.title} onClick={() => setId(null)}>
      <div className="card ex-modal-card help-pop" onClick={(e) => e.stopPropagation()}>
        <div className="between" style={{ alignItems: 'flex-start' }}>
          <span className="stack" style={{ gap: 2 }}>
            <span className="muted tiny">Справка · {hit.chapter.title}</span>
            <b style={{ fontSize: 17 }}>{hit.section.title}</b>
          </span>
          <button type="button" className="xbtn" aria-label="Закрыть" onClick={() => setId(null)}>×</button>
        </div>
        <HelpText text={hit.section.body} />
        <button type="button" className="linkbtn small" style={{ alignSelf: 'flex-start' }} onClick={() => { const s = id; setId(null); openHelpPage(s ?? undefined); }}>Вся справка →</button>
      </div>
    </div>
  );
}
