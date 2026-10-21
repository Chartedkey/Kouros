// Card grids without holes (0.21, owner): every `.grid` / `.grid2` becomes a masonry grid — small
// 4 px rows, each card spans as many rows as it is tall, so a short card no longer leaves an empty
// strip next to a tall neighbour. Cards keep their place in the DOM (no remounts, forms keep state).
const ROW = 4;
const GAP = 20;
const SEL = '.grid, .grid2';

export function initMasonry() {
  if (typeof ResizeObserver === 'undefined' || typeof MutationObserver === 'undefined') return;
  const seen = new WeakSet<Element>();
  const dirty = new Set<HTMLElement>();
  let frame = 0;
  const schedule = (g: HTMLElement) => {
    dirty.add(g);
    if (!frame) frame = requestAnimationFrame(flush);
  };
  const ro = new ResizeObserver((entries) => {
    for (const e of entries) {
      const t = e.target as HTMLElement;
      const g = t.matches(SEL) ? t : t.parentElement;
      if (g && g.matches(SEL)) schedule(g);
    }
  });
  function layout(g: HTMLElement) {
    const kids = Array.from(g.children) as HTMLElement[];
    if (!seen.has(g)) { seen.add(g); ro.observe(g); }
    for (const c of kids) {
      if (!seen.has(c)) { seen.add(c); ro.observe(c); }
    }
    // Read all heights first, then write, so the browser lays out once.
    const hs = kids.map((c) => c.offsetHeight);
    kids.forEach((c, i) => {
      const span = hs[i] > 0 ? Math.ceil((hs[i] + GAP) / ROW) : 1;
      const v = `span ${span}`;
      if (c.style.gridRowEnd !== v) c.style.gridRowEnd = v;
    });
    g.classList.add('mz');
  }
  function flush() {
    frame = 0;
    const list = Array.from(dirty);
    dirty.clear();
    list.forEach(layout);
  }
  const scan = () => document.querySelectorAll<HTMLElement>(SEL).forEach(schedule);
  new MutationObserver((muts) => {
    for (const m of muts) {
      if (m.type !== 'childList') continue;
      const t = m.target as HTMLElement;
      if (t.matches?.(SEL)) schedule(t);
      m.addedNodes.forEach((n) => {
        if (!(n instanceof HTMLElement)) return;
        if (n.matches(SEL)) schedule(n);
        n.querySelectorAll<HTMLElement>(SEL).forEach(schedule);
      });
    }
  }).observe(document.body, { childList: true, subtree: true });
  scan();
}
