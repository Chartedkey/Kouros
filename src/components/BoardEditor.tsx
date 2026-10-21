// «Изменить расположение» (0.21.1): drag cards between the two columns and up/down, switch a card
// between narrow and full width, reset the tab to the default. The arrangement itself lives in
// lib/board.ts; this file only draws the edit layer and the toolbar.
import { useEffect, useRef, useState, type PointerEvent as RPointerEvent } from 'react';
import { board, type Card, type Col } from '../lib/board';

export function useBoard() {
  const [, set] = useState(0);
  useEffect(() => board.subscribe(() => set((n) => n + 1)), []);
  return board;
}

type Drag = { key: string; dx: number; dy: number; x: number; y: number; w: number; h: number; last: string };

/** Overlay boxes over every movable card while editing. Rendered as the last child of `.wrap`. */
export function BoardLayer() {
  const b = useBoard();
  const [drag, setDrag] = useState<Drag | null>(null);
  const dragRef = useRef<Drag | null>(null);
  dragRef.current = drag;
  if (!b.editing) return <div className="bd-layer" aria-hidden="true" />;
  const wrap = b.wrap;
  const toLocal = (e: { clientX: number; clientY: number }) => {
    if (!wrap) return { x: 0, y: 0 };
    const r = wrap.getBoundingClientRect();
    const k = wrap.offsetWidth / (r.width || 1);
    return { x: (e.clientX - r.left) * k, y: (e.clientY - r.top) * k };
  };
  const down = (c: Card) => (e: RPointerEvent<HTMLDivElement>) => {
    if ((e.target as HTMLElement).closest('button')) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    const p = toLocal(e);
    setDrag({ key: c.key, dx: p.x - c.left, dy: p.y - c.top, x: p.x, y: p.y, w: c.width, h: c.height, last: '' });
  };
  const move = (e: RPointerEvent<HTMLDivElement>) => {
    const d = dragRef.current;
    if (!d || !wrap) return;
    const p = toLocal(e);
    if (e.clientY < 70) window.scrollBy(0, -14);
    else if (e.clientY > window.innerHeight - 70) window.scrollBy(0, 14);
    const me = b.cards.find((c) => c.key === d.key);
    if (!me) return;
    const col: Col = me.col === 'w' || b.narrow ? me.col : p.x < wrap.offsetWidth / 2 ? 'l' : 'r';
    const cy = p.y - d.dy + d.h / 2;
    const others = b.cards.filter((c) => c.run === me.run && c.key !== me.key);
    const before = others.find((c) => (col === 'w' || c.col === 'w' || c.col === col) && c.top + c.height / 2 > cy)?.key ?? null;
    const sig = `${before}|${col}`;
    const next = { ...d, x: p.x, y: p.y, last: sig };
    setDrag(next);
    if (sig !== d.last) {
      // Only move when the target differs from where the card already is.
      const idx = b.cards.findIndex((c) => c.key === me.key);
      const after = b.cards.slice(idx + 1).find((c) => c.run === me.run && (col === 'w' || c.col === 'w' || c.col === col))?.key ?? null;
      if (!(col === me.col && before === after)) b.move(me.key, before, col);
    }
  };
  const up = () => setDrag(null);
  return (
    <div className="bd-layer">
      {b.cards.map((c) => (
        <div
          key={c.key}
          className={`bd-box${drag?.key === c.key ? ' bd-hold' : ''}`}
          style={{ top: c.top, left: c.left, width: c.width, height: c.height }}
          onPointerDown={down(c)}
          onPointerMove={move}
          onPointerUp={up}
          onPointerCancel={up}
        >
          <span className="bd-handle">⠿ перетащи</span>
          {!b.narrow && (
            <button type="button" className="bd-width" onClick={() => b.setCol(c.key, c.col === 'w' ? 'l' : 'w')}>
              {c.col === 'w' ? '↔ узкая' : '↔ на всю ширину'}
            </button>
          )}
        </div>
      ))}
      {drag && <div className="bd-ghost" style={{ top: drag.y - drag.dy, left: drag.x - drag.dx, width: drag.w, height: Math.min(drag.h, 240) }} />}
    </div>
  );
}

/** Toolbar at the bottom of the window while editing. */
export function BoardBar() {
  const b = useBoard();
  useEffect(() => {
    if (!b.editing) return;
    const k = (e: KeyboardEvent) => { if (e.key === 'Escape') b.setEditing(false); };
    window.addEventListener('keydown', k);
    return () => window.removeEventListener('keydown', k);
  }, [b.editing]);
  if (!b.editing) return null;
  return (
    <div className="bd-bar" role="toolbar" aria-label="Расположение карточек">
      <span className="small">Перетаскивай карточки мышкой. Раскладка запоминается для этой вкладки.</span>
      <button type="button" className="btn2" disabled={!b.hasSaved()} onClick={() => b.reset()}>Вернуть как было</button>
      <button type="button" className="btn btn-green" onClick={() => b.setEditing(false)}>Готово</button>
    </div>
  );
}
