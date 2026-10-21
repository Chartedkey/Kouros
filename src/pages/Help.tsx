// «Справка» (1.1): chapters by tab, the method in plain words, FAQ, search, and the tour restart.
import { useEffect, useMemo, useState } from 'react';
import { HELP, type HelpChapter } from '../help/content';
import { HelpText } from '../help/HelpText';
import { startTour } from '../lib/helptips';

function norm(s: string) { return s.toLowerCase().replace(/ё/g, 'е'); }

export function Help({ focus, onFocusUsed }: { focus?: string | null; onFocusUsed?: () => void }) {
  const [q, setQ] = useState('');
  const [chap, setChap] = useState<string>(() => (focus ? focus.split('.')[0] : HELP[0].id));
  // A chapter id may differ from the section prefix — find the chapter that owns the section.
  useEffect(() => {
    if (!focus) return;
    const c = HELP.find((x) => x.sections.some((s) => s.id === focus));
    if (c) setChap(c.id);
    setQ('');
    const t = setTimeout(() => {
      const el = document.getElementById(`help-${focus}`);
      el?.scrollIntoView({ behavior: 'smooth', block: 'start' });
      el?.classList.add('nav-hit');
      setTimeout(() => el?.classList.remove('nav-hit'), 1600);
    }, 120);
    onFocusUsed?.();
    return () => clearTimeout(t);
  }, [focus]); // eslint-disable-line react-hooks/exhaustive-deps

  const found = useMemo(() => {
    const n = norm(q.trim());
    if (n.length < 2) return null;
    const words = n.split(/\s+/);
    return HELP.flatMap((c) => c.sections.filter((s) => {
      const hay = norm(`${s.title} ${s.body} ${c.title}`);
      return words.every((w) => hay.includes(w));
    }).map((s) => ({ c, s })));
  }, [q]);

  const cur: HelpChapter = HELP.find((c) => c.id === chap) ?? HELP[0];
  return (
    <div className="help-page">
      <div className="page-head" style={{ marginBottom: 16 }}>
        <div className="stack">
          <h1>Справка</h1>
          <p className="subtitle">Что показывает каждая карточка, откуда берутся цифры и что с ними делать</p>
        </div>
        <button type="button" className="btn2" onClick={startTour}>Пройти обучение</button>
      </div>
      <div className="help-grid">
        <nav className="help-nav card" aria-label="Главы справки">
          <input className="input" placeholder="Поиск по справке…" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Поиск по справке" />
          {HELP.map((c) => (
            <button key={c.id} type="button" className="help-nav-item" aria-pressed={!found && c.id === chap} onClick={() => { setQ(''); setChap(c.id); window.scrollTo({ top: 0, behavior: 'smooth' }); }}>{c.title}</button>
          ))}
        </nav>
        <div className="stack" style={{ gap: 16, minWidth: 0 }}>
          {found ? (
            <>
              <span className="muted small">{found.length ? `Нашлось: ${found.length}` : 'Ничего не нашлось — попробуй другое слово.'}</span>
              {found.map(({ c, s }) => (
                <article key={s.id} className="card help-sec" id={`help-${s.id}`}>
                  <span className="muted tiny">{c.title}</span>
                  <h2 className="help-h">{s.title}</h2>
                  <HelpText text={s.body} mark={q.trim().split(/\s+/)[0]} />
                </article>
              ))}
            </>
          ) : (
            <>
              <div className="stack" style={{ gap: 4 }}>
                <h2 className="help-chapter">{cur.title}</h2>
                {cur.intro && <span className="muted">{cur.intro}</span>}
              </div>
              {cur.sections.map((s) => (
                <article key={s.id} className="card help-sec" id={`help-${s.id}`}>
                  <h2 className="help-h">{s.title}</h2>
                  <HelpText text={s.body} />
                </article>
              ))}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
