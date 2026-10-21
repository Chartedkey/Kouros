// 1.0: «перейти к карточке». After a tab switch the page renders and loads its data, so the
// card is looked for a few times (up to ~4 s) by its aria-label and scrolled into view with a
// short highlight.
export function scrollToCard(label: string) {
  let tries = 0;
  const tick = () => {
    const el = document.querySelector(`[aria-label="${label.replace(/"/g, '\\"')}"]`) as HTMLElement | null;
    if (el && el.offsetParent !== null) {
      el.scrollIntoView({ behavior: 'smooth', block: 'start' });
      el.classList.add('nav-hit');
      setTimeout(() => el.classList.remove('nav-hit'), 1600);
      return;
    }
    if (++tries < 27) setTimeout(tick, 150);
  };
  setTimeout(tick, 120);
}

/** Where a link leads: a tab, optionally its subtab, optionally a card on it. */
export type Go = { tab: string; sub?: string; card?: string };
