// Cards you can move (0.21.1, owner). The page column `.wrap` is a two-column grid with 4 px rows.
// Cards — top-level `section`s and the children of top-level `.grid` / `.grid2` (those grids and
// card-only `.stack`s inside them become display: contents) — are placed by this module: each one
// in the left column, the right column or across both, one under another without holes. Other
// top-level elements (page head, tabs, banners, rings, footer) stay full width where they are and
// split the cards into groups; a card moves inside its group. The order and columns you choose
// are kept per tab and subtab in localStorage. Nothing is remounted, so forms keep their state.

export type Col = 'l' | 'r' | 'w';
export type Card = { key: string; el: HTMLElement; run: number; col: Col; top: number; left: number; width: number; height: number };
type Saved = { order: string[]; col: Record<string, Col> };

const ROW = 4;
const GAP = 20;
const SEL = '.grid, .grid2';
const STORE = 'kouros_layout_v1';
const NARROW = 1000;

let editing = false;
let cards: Card[] = [];
let page = '';
let wrapEl: HTMLElement | null = null;
const listeners = new Set<() => void>();
let schedule: () => void = () => {};

function loadAll(): Record<string, Saved> {
  try { return JSON.parse(localStorage.getItem(STORE) ?? '{}') as Record<string, Saved>; } catch { return {}; }
}
function saveAll(v: Record<string, Saved>) {
  try { localStorage.setItem(STORE, JSON.stringify(v)); } catch { /* storage unavailable */ }
}

export const board = {
  get editing() { return editing; },
  get cards() { return cards; },
  get page() { return page; },
  get wrap() { return wrapEl; },
  get narrow() { return !!wrapEl && wrapEl.offsetWidth < NARROW; },
  subscribe(fn: () => void) { listeners.add(fn); return () => { listeners.delete(fn); }; },
  setEditing(on: boolean) { editing = on; document.documentElement.classList.toggle('bd-edit', on); listeners.forEach((f) => f()); schedule(); },
  /** Freeze the current arrangement of this page (so later moves don't shuffle the rest). */
  snapshot() {
    const all = loadAll();
    all[page] = { order: cards.map((c) => c.key), col: Object.fromEntries(cards.map((c) => [c.key, c.col])) };
    saveAll(all);
  },
  /** Put `key` before `before` (null = end of its group) in column `col`. */
  move(key: string, before: string | null, col: Col) {
    const all = loadAll();
    const cur = all[page] ?? { order: cards.map((c) => c.key), col: Object.fromEntries(cards.map((c) => [c.key, c.col])) };
    const known = new Set(cur.order);
    const order = [...cur.order, ...cards.map((c) => c.key).filter((k) => !known.has(k))].filter((k) => k !== key);
    if (before && order.includes(before)) order.splice(order.indexOf(before), 0, key);
    else {
      // End of its group: after the last card of the same group.
      const run = cards.find((c) => c.key === key)?.run;
      const lastOfRun = [...cards].reverse().find((c) => c.run === run && c.key !== key)?.key;
      const at = lastOfRun ? order.indexOf(lastOfRun) + 1 : order.length;
      order.splice(at, 0, key);
    }
    all[page] = { order, col: { ...cur.col, [key]: col } };
    saveAll(all);
    schedule();
  },
  setCol(key: string, col: Col) {
    const all = loadAll();
    const cur = all[page] ?? { order: cards.map((c) => c.key), col: Object.fromEntries(cards.map((c) => [c.key, c.col])) };
    all[page] = { ...cur, col: { ...cur.col, [key]: col } };
    saveAll(all);
    schedule();
  },
  reset() {
    const all = loadAll();
    delete all[page];
    saveAll(all);
    schedule();
  },
  hasSaved() { return !!loadAll()[page]; },
};

function pageId(wrap: HTMLElement): string {
  const sub = wrap.querySelector(':scope > nav.subtabs button[aria-pressed="true"]')?.textContent?.trim() ?? '';
  return `${wrap.dataset.page ?? ''}|${sub}`;
}

const isFlat = (el: Element) => el.classList.contains('stack') && el.children.length > 0 && Array.from(el.children).every((c) => c.tagName === 'SECTION');

type Item = { el: HTMLElement; movable: boolean; fromGrid: boolean; side: number };
function collect(wrap: HTMLElement): Item[] {
  const out: Item[] = [];
  for (const c of Array.from(wrap.children) as HTMLElement[]) {
    if (c.classList.contains('bd-layer')) continue;
    if (c.matches(SEL)) {
      c.classList.add('bd-in');
      (Array.from(c.children) as HTMLElement[]).forEach((g, gi) => {
        if (isFlat(g)) {
          g.classList.add('bd-in');
          for (const s of Array.from(g.children) as HTMLElement[]) out.push({ el: s, movable: true, fromGrid: true, side: gi % 2 });
        } else {
          g.classList.remove('bd-in');
          out.push({ el: g, movable: true, fromGrid: true, side: gi % 2 });
        }
      });
    } else {
      out.push({ el: c, movable: c.tagName === 'SECTION', fromGrid: false, side: 0 });
    }
  }
  return out;
}

function keyOf(el: HTMLElement): string {
  return el.getAttribute('aria-label') || el.dataset.card || `${el.tagName.toLowerCase()}.${el.classList[0] ?? ''}`;
}

function heightOf(el: HTMLElement): number {
  if (getComputedStyle(el).display === 'none') return 0;
  const cs = getComputedStyle(el);
  return el.offsetHeight + (parseFloat(cs.marginTop) || 0) + (parseFloat(cs.marginBottom) || 0);
}

function layout(wrap: HTMLElement) {
  wrapEl = wrap;
  if (!wrap.classList.contains('bd')) wrap.classList.add('bd');
  page = pageId(wrap);
  const narrow = wrap.offsetWidth < NARROW;
  const saved = loadAll()[page];
  const items = collect(wrap);
  // Keys, unique per page.
  const seenKeys = new Map<string, number>();
  const keys = items.map((it) => {
    const k = keyOf(it.el);
    const n = (seenKeys.get(k) ?? 0) + 1;
    seenKeys.set(k, n);
    return n > 1 ? `${k}#${n}` : k;
  });
  // Groups: runs of cards between fixed elements.
  type Seq = { kind: 'fixed'; i: number } | { kind: 'run'; is: number[] };
  const seq: Seq[] = [];
  items.forEach((it, i) => {
    if (!it.movable) { seq.push({ kind: 'fixed', i }); return; }
    const last = seq[seq.length - 1];
    if (last && last.kind === 'run') last.is.push(i);
    else seq.push({ kind: 'run', is: [i] });
  });
  // Saved order inside each group; new cards keep their default place after the card before them.
  const rankOf = new Map<string, number>();
  saved?.order.forEach((k, i) => rankOf.set(k, i));
  for (const s of seq) {
    if (s.kind !== 'run' || !saved) continue;
    let prev = -1;
    const ranks = new Map<number, number>();
    s.is.forEach((i, j) => {
      const r = rankOf.get(keys[i]);
      const v = r != null ? r : prev + 0.001 * (j + 1);
      ranks.set(i, v);
      if (r != null) prev = r;
    });
    s.is.sort((a, b) => (ranks.get(a)! - ranks.get(b)!) || a - b);
  }
  const hs = items.map((it) => heightOf(it.el));
  const span = (i: number) => (hs[i] > 0 ? Math.ceil((hs[i] + GAP) / ROW) : 0);
  const b = [0, 0];
  const next: Card[] = [];
  const put = (el: HTMLElement, col: Col, row: number, n: number) => {
    const gc = col === 'w' ? '1 / -1' : col === 'l' ? '1' : '2';
    const gr = n > 0 ? `${row + 1} / span ${n}` : `${row + 1} / span 1`;
    if (el.style.gridColumn !== gc) el.style.gridColumn = gc;
    if (el.style.gridRow !== gr) el.style.gridRow = gr;
    el.style.visibility = n > 0 ? '' : 'hidden';
  };
  let run = 0;
  for (const s of seq) {
    if (s.kind === 'fixed') {
      const row = Math.max(b[0], b[1]);
      const n = span(s.i);
      put(items[s.i].el, 'w', row, n);
      b[0] = b[1] = row + n;
      continue;
    }
    run++;
    for (const i of s.is) {
      const it = items[i];
      // Default: cards of a grid keep their old side (first left, second right…), stacked without holes.
      let col: Col = saved?.col[keys[i]] ?? (it.fromGrid ? (it.side === 0 ? 'l' : 'r') : 'w');
      if (narrow) col = 'w';
      const n = span(i);
      let row: number;
      if (col === 'w') { row = Math.max(b[0], b[1]); b[0] = b[1] = row + n; } else { const c = col === 'l' ? 0 : 1; row = b[c]; b[c] = row + n; }
      put(it.el, col, row, n);
      next.push({ key: keys[i], el: it.el, run, col: narrow ? (saved?.col[keys[i]] ?? col) : col, top: 0, left: 0, width: 0, height: 0 });
    }
    b[0] = b[1] = Math.max(b[0], b[1]);
  }
  // Positions for the editor (layout px, relative to .wrap).
  for (const c of next) { c.top = c.el.offsetTop; c.left = c.el.offsetLeft; c.width = c.el.offsetWidth; c.height = c.el.offsetHeight; }
  cards = next;
  listeners.forEach((f) => f());
}

export function initBoard() {
  if (typeof ResizeObserver === 'undefined' || typeof MutationObserver === 'undefined') return;
  let frame = 0;
  const seen = new WeakSet<Element>();
  const ro = new ResizeObserver(() => schedule());
  schedule = () => { if (!frame) frame = requestAnimationFrame(run); };
  function run() {
    frame = 0;
    const wrap = document.querySelector<HTMLElement>('.wrap');
    if (!wrap) return;
    if (!seen.has(wrap)) { seen.add(wrap); ro.observe(wrap); }
    layout(wrap);
    for (const c of cards) if (!seen.has(c.el)) { seen.add(c.el); ro.observe(c.el); }
    for (const c of Array.from(wrap.children)) if (!seen.has(c)) { seen.add(c); ro.observe(c); }
  }
  new MutationObserver((muts) => {
    for (const m of muts) {
      const t = m.target as HTMLElement;
      if (m.type === 'childList' && (t.classList?.contains('wrap') || t.parentElement?.classList.contains('wrap') || t.parentElement?.parentElement?.classList.contains('wrap'))) { schedule(); return; }
      if (m.type === 'attributes' && t.closest?.('.wrap > nav.subtabs')) { schedule(); return; }
    }
  }).observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ['aria-pressed'] });
  schedule();
}
