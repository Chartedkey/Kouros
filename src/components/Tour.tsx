// Обучение (1.1, плавное в 1.1.1): короткие шаги по главным кнопкам и карточкам. Запускается само
// один раз (localStorage `kouros_tour_v1`), заново — кнопкой «Пройти обучение» в справке.
// Окно с текстом всегда внизу по центру; подсветка плавно переезжает с карточки на карточку;
// страница прокручивается только если карточка не видна, и подсветка встаёт, когда прокрутка
// закончилась. Шаги идут сверху вниз: сначала шапка, потом карточки «Сегодня».
import { useCallback, useEffect, useRef, useState } from 'react';
import type { Go } from '../lib/nav';
import { TOUR_EVENT } from '../lib/helptips';
import { getZoom } from '../lib/zoom';

type Step = { tab?: string; sel?: string; title: string; text: string; optional?: boolean };

const STEPS: Step[] = [
  { title: 'Привет! Это Kouros', text: 'Личный тренер и дневник здоровья. Он сам забирает тренировки и данные часов, считает нагрузку и готовность и каждый день подсказывает, как тренироваться. Покажу главное — это минута. Можно пропустить и вернуться позже из справки.' },
  { tab: 'today', sel: 'button[aria-label="Открыть справку"]', title: 'Справка', text: 'Здесь объяснено всё: что показывает каждая карточка и как Kouros считает. Знак «?» у названия карточки открывает нужный абзац сразу.' },
  { tab: 'today', sel: 'button[aria-label="Изменить расположение карточек"]', title: 'Своя раскладка', text: 'Эта кнопка включает режим расположения: перетаскивай карточки, как тебе удобно. Раскладка запоминается для каждой вкладки.' },
  { tab: 'today', sel: 'header nav button:nth-child(2)', title: 'Тренировки', text: 'Все тренировки, нагрузка на сердце и мышцы, рекорды и прогнозы, карта мышц, кроссовки, резинки и библиотека упражнений.' },
  { tab: 'today', sel: 'header nav button:nth-child(6)', title: 'Тренер', text: '«Как мой день?», анализ недели, план на 3 недели, дневник боли, цель и «Спросить Claude». Начни со знакомства — из него строится план.' },
  { tab: 'today', sel: 'section[aria-label="Готовность"]', title: 'Готовность', text: 'Насколько тело восстановилось — по ночному HRV, пульсу покоя, сну, дыханию и свежести после нагрузки. 50 — твой обычный день: всё сравнивается с твоей нормой за 30 дней. Цифра фиксируется утром, а с вечера появляется вечерняя оценка.' },
  { tab: 'today', sel: 'section[aria-label="Как прошла тренировка"]', optional: true, title: 'После тренировки', text: 'Пара кроссовок, тяжесть 1–10, ощущения и боль — 15 секунд. От этого зависят пробег кроссовок, нагрузка силовых и план на ближайшие дни.' },
  { tab: 'today', sel: 'section[aria-label="Что повлияло на готовность"]', title: 'Что повлияло', text: 'Раскрой — увидишь, сколько баллов добавил или отнял каждый показатель и где твой коридор нормы.' },
  { tab: 'today', sel: 'section[aria-label="Рекомендация на день"]', title: 'Что разрешает тело', text: 'Ступень дня по готовности: от интервалов до отдыха, с объёмом. Опросы самочувствия могут её только снизить — никогда не поднимают.' },
  { tab: 'today', sel: 'section[aria-label="Тренировка на сегодня"]', optional: true, title: 'Тренировка по плану', text: '«Подтверждаю» — делаешь как в плане. «Выбрать другое» — отметь, что будешь делать. Если выбор рискованный, Kouros один раз предупредит и предложит вариант полегче, а потом сам пересчитает неделю. Решение всегда твоё.' },
  { tab: 'today', sel: 'section[aria-label="Кроссовки на сегодня"]', optional: true, title: 'Кроссовки на сегодня', text: 'Два касания — температура и дорога — и Kouros посоветует пару под тренировку, погоду, износ и самочувствие ног.' },
  { tab: 'today', sel: 'section[aria-label="Напоминания"]', title: 'Напоминания', text: 'Ранние сигналы и опросы. «Подробнее →» ведёт к причине, × скрывает на неделю — если станет хуже, напоминание вернётся.' },
  { title: 'С чего начать', text: 'Нажми кнопку с буквой в правом верхнем углу → «Профиль и зоны»: впиши максимальный пульс и пороговый темп. Потом «Синхронизация»: подключи Strava и freddy или Health Auto Export. Дальше Kouros работает сам и обновляет данные каждые 20 минут.' },
];

const KEY = 'kouros_tour_v1';
export function tourDone(): boolean { try { return localStorage.getItem(KEY) === '1'; } catch { return true; } }
function markDone() { try { localStorage.setItem(KEY, '1'); } catch { /* ignore */ } }

type Box = { top: number; left: number; width: number; height: number };
const POP_SPACE = 250; // место под окно с текстом внизу экрана (в px интерфейса)
const TOP_SPACE = 24;

function reduced(): boolean {
  try { return window.matchMedia('(prefers-reduced-motion: reduce)').matches; } catch { return false; }
}
function rectOf(el: HTMLElement): Box {
  const z = getZoom();
  const r = el.getBoundingClientRect();
  return { top: r.top / z, left: r.left / z, width: r.width / z, height: r.height / z };
}

export function Tour({ go, tab, auto }: { go: (g: Go) => void; tab: string; auto: boolean }) {
  const [on, setOn] = useState(false);
  const [i, setI] = useState(0);
  const [box, setBox] = useState<Box | null>(null);
  const [moving, setMoving] = useState(false);
  const dir = useRef(1);
  const elRef = useRef<HTMLElement | null>(null);
  const goRef = useRef(go);
  goRef.current = go;

  useEffect(() => {
    const start = () => { dir.current = 1; setI(0); setOn(true); };
    window.addEventListener(TOUR_EVENT, start);
    return () => window.removeEventListener(TOUR_EVENT, start);
  }, []);
  useEffect(() => {
    if (!auto || tourDone()) return;
    const t = setTimeout(() => { setI(0); setOn(true); }, 1500);
    return () => clearTimeout(t);
  }, [auto]);

  const close = useCallback(() => { setOn(false); setBox(null); elRef.current = null; markDone(); }, []);

  // Find the target, scroll only as much as needed, wait until the page stops moving, then show.
  useEffect(() => {
    if (!on) return;
    const st = STEPS[i];
    if (!st) { close(); return; }
    if (st.tab && st.tab !== tab) { goRef.current({ tab: st.tab }); return; }
    if (!st.sel) { elRef.current = null; setBox(null); setMoving(false); return; }
    setMoving(true);
    let stop = false;
    let tries = 0;
    const settle = (el: HTMLElement) => {
      let last = '';
      let same = 0;
      let frames = 0;
      const tick = () => {
        if (stop) return;
        const b = rectOf(el);
        const k = `${Math.round(b.top)}:${Math.round(b.left)}:${Math.round(b.height)}`;
        same = k === last ? same + 1 : 0;
        last = k;
        if (same >= 4 || ++frames > 90) { setBox(b); setMoving(false); return; }
        requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    };
    const look = () => {
      if (stop) return;
      const el = document.querySelector(st.sel!) as HTMLElement | null;
      if (el && el.offsetParent !== null) {
        elRef.current = el;
        const z = getZoom();
        const vh = window.innerHeight / z;
        const b = rectOf(el);
        const room = vh - POP_SPACE - TOP_SPACE;
        let delta = 0;
        if (b.height > room || b.top < TOP_SPACE) delta = b.top - TOP_SPACE;
        else if (b.top + b.height > vh - POP_SPACE) delta = b.top + b.height - (vh - POP_SPACE);
        if (Math.abs(delta) > 2) window.scrollBy({ top: delta * z, behavior: reduced() ? 'auto' : 'smooth' });
        // Glide the highlight to where the card will be after the scroll, at the same time.
        setBox({ ...b, top: b.top - delta });
        settle(el);
        return;
      }
      if (++tries < 16) { setTimeout(look, 150); return; }
      if (st.optional) setI((x) => Math.max(0, Math.min(STEPS.length, x + dir.current)));
      else { elRef.current = null; setBox(null); setMoving(false); }
    };
    look();
    return () => { stop = true; };
  }, [on, i, tab, close]);

  useEffect(() => {
    if (!on) return;
    const upd = () => { if (elRef.current?.isConnected) setBox(rectOf(elRef.current)); };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close();
      if (e.key === 'ArrowRight' || e.key === 'Enter') { dir.current = 1; setI((x) => x + 1); }
      if (e.key === 'ArrowLeft') { dir.current = -1; setI((x) => Math.max(0, x - 1)); }
    };
    window.addEventListener('resize', upd);
    window.addEventListener('keydown', key);
    return () => { window.removeEventListener('resize', upd); window.removeEventListener('keydown', key); };
  }, [on, close]);

  if (!on) return null;
  const st = STEPS[i];
  if (!st) return null;
  const last = i === STEPS.length - 1;
  const z = getZoom();
  const vw = window.innerWidth / z;
  const W = Math.min(440, vw - 32);
  // Without a target the hole shrinks to a point above the text — the dim stays, nothing flashes.
  const hole = box ?? { top: window.innerHeight / z / 2 - 140, left: vw / 2, width: 0, height: 0 };
  return (
    <div className={`tour${reduced() ? ' tour-still' : ''}`} role="dialog" aria-modal="true" aria-label="Обучение">
      <div className={`tour-hole${box ? '' : ' none'}`} style={box
        ? { top: hole.top - 6, left: hole.left - 6, width: hole.width + 12, height: hole.height + 12 }
        : { top: hole.top, left: hole.left, width: 0, height: 0, borderWidth: 0 }} />
      {/* 1.1.2: placed from the top in interface px (divided by the zoom) — «bottom» drifted off-screen when the UI was scaled. */}
      <div className={`tour-pop card${moving ? ' moving' : ''}`} style={{ width: W, left: (vw - W) / 2, top: window.innerHeight / z - 24, transform: 'translateY(-100%)' }}>
        <span className="muted tiny">Шаг {i + 1} из {STEPS.length}</span>
        <b style={{ fontSize: 17 }}>{st.title}</b>
        <span className="small" style={{ lineHeight: 1.55, color: 'var(--text-2)' }}>{st.text}</span>
        <div className="between">
          <button type="button" className="linkbtn small" onClick={close}>{last ? 'Закрыть' : 'Пропустить'}</button>
          <span className="row" style={{ gap: 8 }}>
            {i > 0 && <button type="button" className="btn2" onClick={() => { dir.current = -1; setI(i - 1); }}>Назад</button>}
            <button type="button" className="btn btn-green" onClick={() => { if (last) close(); else { dir.current = 1; setI(i + 1); } }}>{last ? 'Готово' : 'Дальше'}</button>
          </span>
        </div>
      </div>
    </div>
  );
}
