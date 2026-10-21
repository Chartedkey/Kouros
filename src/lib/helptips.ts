// 1.1: «?» next to a card title. Cards are found by their aria-label (the same names the board
// uses); the button is added to the card's first `.label` without touching the page code. Clicking
// it opens that help section in a small window (components/HelpLayer.tsx).
import { CARD_HELP } from '../help/content';

export const HELP_EVENT = 'kouros-help';
export const HELP_PAGE_EVENT = 'kouros-help-page';
export const TOUR_EVENT = 'kouros-tour';

/** Opens one help section in the small window. */
export function openHelp(id: string) {
  window.dispatchEvent(new CustomEvent(HELP_EVENT, { detail: id }));
}
/** Opens the whole help page (optionally at a section). */
export function openHelpPage(id?: string) {
  window.dispatchEvent(new CustomEvent(HELP_PAGE_EVENT, { detail: id ?? null }));
}
/** Starts the tour. */
export function startTour() {
  window.dispatchEvent(new CustomEvent(TOUR_EVENT));
}

function decorate(root: ParentNode) {
  root.querySelectorAll<HTMLElement>('section[aria-label]').forEach((sec) => {
    const id = CARD_HELP[sec.getAttribute('aria-label') ?? ''];
    if (!id) return;
    const label = sec.querySelector<HTMLElement>('.label');
    if (!label || label.closest('section') !== sec) return;
    if (label.querySelector('.help-q')) return;
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'help-q';
    b.textContent = '?';
    b.title = 'Что это и как считается';
    b.setAttribute('aria-label', 'Справка');
    b.addEventListener('click', (e) => { e.preventDefault(); e.stopPropagation(); openHelp(id); });
    label.appendChild(b);
  });
}

export function initHelpTips() {
  let queued = false;
  const run = () => { queued = false; decorate(document); };
  const mo = new MutationObserver(() => {
    if (queued) return;
    queued = true;
    requestAnimationFrame(run);
  });
  const start = () => { run(); mo.observe(document.body, { childList: true, subtree: true, characterData: true }); };
  if (document.body) start(); else window.addEventListener('DOMContentLoaded', start);
}
