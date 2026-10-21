// Обучение (1.1): 13 коротких шагов по главным карточкам и кнопкам. Запускается само один раз
// (localStorage `kouros_tour_v1`), заново — кнопкой «Пройти обучение» в справке.
import { useCallback, useEffect, useRef, useState, type CSSProperties } from 'react';
import type { Go } from '../lib/nav';
import { TOUR_EVENT } from '../lib/helptips';
import { getZoom } from '../lib/zoom';

type Step = { tab?: string; sel?: string; title: string; text: string; optional?: boolean };

const STEPS: Step[] = [
  { title: 'Привет! Это Kouros', text: 'Личный тренер и дневник здоровья. Он сам забирает тренировки и данные часов, считает нагрузку и готовность и каждый день подсказывает, как тренироваться. Покажу главное — это минута. Можно пропустить и вернуться позже из справки.' },
  { tab: 'today', sel: 'section[aria-label="Готовность"]', title: 'Готовность', text: 'Насколько тело восстановилось — по ночному HRV, пульсу покоя, сну, дыханию и свежести после нагрузки. 50 — твой обычный день: всё сравнивается с твоей нормой за 30 дней. Цифра фиксируется утром, а с вечера появляется вечерняя оценка.' },
  { tab: 'today', sel: 'section[aria-label="Что повлияло на готовность"]', title: 'Что повлияло', text: 'Раскрой — увидишь, сколько баллов добавил или отнял каждый показатель и где твой коридор нормы.' },
  { tab: 'today', sel: 'section[aria-label="Рекомендация на день"]', title: 'Что разрешает тело', text: 'Ступень дня по готовности: от интервалов до отдыха, с объёмом. Опросы самочувствия могут её только снизить — никогда не поднимают.' },
  { tab: 'today', sel: 'section[aria-label="Тренировка на сегодня"]', optional: true, title: 'Тренировка по плану', text: '«Подтверждаю» — делаешь как в плане. «Выбрать другое» — отметь, что будешь делать. Если выбор рискованный, Kouros один раз предупредит и предложит вариант полегче, а потом сам пересчитает неделю. Решение всегда твоё.' },
  { tab: 'today', sel: 'section[aria-label="Кроссовки на сегодня"]', optional: true, title: 'Кроссовки на сегодня', text: 'Два касания — температура и дорога — и Kouros посоветует пару под тренировку, погоду, износ и самочувствие ног.' },
  { tab: 'today', sel: 'section[aria-label="Как прошла тренировка"]', optional: true, title: 'После тренировки', text: 'Пара кроссовок, тяжесть 1–10, ощущения и боль — 15 секунд. От этого зависят пробег кроссовок, нагрузка силовых и план на ближайшие дни.' },
  { tab: 'today', sel: 'section[aria-label="Напоминания"]', title: 'Напоминания', text: 'Ранние сигналы и опросы. «Подробнее →» ведёт к причине, × скрывает на неделю — если станет хуже, напоминание вернётся.' },
  { sel: 'button[aria-label="Изменить расположение карточек"]', title: 'Своя раскладка', text: 'Эта кнопка включает режим расположения: перетаскивай карточки, как тебе удобно. Раскладка запоминается для каждой вкладки.' },
  { sel: 'header nav button:nth-child(2)', title: 'Тренировки', text: 'Все тренировки, нагрузка на сердце и мышцы, рекорды и прогнозы, карта мышц, кроссовки, резинки и библиотека упражнений.' },
  { sel: 'header nav button:nth-child(6)', title: 'Тренер', text: '«Как мой день?», анализ недели, план на 3 недели, дневник боли, цель и «Спросить Claude». Начни со знакомства — из него строится план.' },
  { sel: 'button[aria-label="Открыть справку"]', title: 'Справка', text: 'Здесь объяснено всё: что показывает каждая карточка и как Kouros считает. Знак «?» у названия карточки открывает нужный абзац сразу.' },
  { sel: 'button[aria-label="Настройки и профиль"]', title: 'С чего начать', text: 'Заполни профиль (максимальный пульс, пороговый темп) и подключи Strava и freddy в «Синхронизации». Дальше Kouros работает сам — обновляет данные каждые 20 минут.' },
];

const KEY = 'kouros_tour_v1';
export function tourDone(): boolean { try { return localStorage.getItem(KEY) === '1'; } catch { return true; } }
function markDone() { try { localStorage.setItem(KEY, '1'); } catch { /* ignore */ } }

type Box = { top: number; left: number; width: number; height: number };

export function Tour({ go, tab, auto }: { go: (g: Go) => void; tab: string; auto: boolean }) {
  const [on, setOn] = useState(false);
  const [i, setI] = useState(0);
  const [box, setBox] = useState<Box | null>(null);
  const dir = useRef(1);
  const goRef = useRef(go);
  goRef.current = go;
  const elRef = useRef<HTMLElement | null>(null);

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

  const measure = useCallback(() => {
    const el = elRef.current;
    if (!el || !el.isConnected) { setBox(null); return; }
    const z = getZoom();
    const r = el.getBoundingClientRect();
    setBox({ top: r.top / z, left: r.left / z, width: r.width / z, height: r.height / z });
  }, []);

  const close = useCallback(() => { setOn(false); setBox(null); elRef.current = null; markDone(); }, []);

  // Find the step target (after switching the tab), skip optional steps that are not on screen.
  useEffect(() => {
    if (!on) return;
    const st = STEPS[i];
    if (!st) { close(); return; }
    if (st.tab && st.tab !== tab) { goRef.current({ tab: st.tab }); return; }
    elRef.current = null; setBox(null);
    if (!st.sel) return;
    let tries = 0; let stop = false;
    const look = () => {
      if (stop) return;
      const el = document.querySelector(st.sel!) as HTMLElement | null;
      if (el && el.offsetParent !== null) {
        elRef.current = el;
        el.scrollIntoView({ behavior: 'smooth', block: 'center' });
        setTimeout(measure, 380);
        return;
      }
      if (++tries < 16) { setTimeout(look, 150); return; }
      if (st.optional) setI((x) => Math.max(0, Math.min(STEPS.length, x + dir.current)));
    };
    look();
    return () => { stop = true; };
  }, [on, i, tab, measure, close]);

  useEffect(() => {
    if (!on) return;
    const upd = () => measure();
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close();
      if (e.key === 'ArrowRight' || e.key === 'Enter') { dir.current = 1; setI((x) => x + 1); }
      if (e.key === 'ArrowLeft') { dir.current = -1; setI((x) => Math.max(0, x - 1)); }
    };
    window.addEventListener('scroll', upd, true);
    window.addEventListener('resize', upd);
    window.addEventListener('keydown', key);
    return () => { window.removeEventListener('scroll', upd, true); window.removeEventListener('resize', upd); window.removeEventListener('keydown', key); };
  }, [on, measure, close]);

  if (!on) return null;
  const st = STEPS[i];
  if (!st) return null;
  const z = getZoom();
  const vw = window.innerWidth / z, vh = window.innerHeight / z;
  const W = Math.min(380, vw - 32);
  let pop: CSSProperties = { width: W, left: (vw - W) / 2, top: Math.max(16, vh / 2 - 120) };
  if (box) {
    const below = box.top + box.height + 14;
    const left = Math.min(Math.max(16, box.left + box.width / 2 - W / 2), vw - W - 16);
    pop = below + 220 < vh ? { width: W, left, top: below } : { width: W, left, top: Math.max(16, box.top - 14 - 220) };
  }
  const last = i === STEPS.length - 1;
  return (
    <div className="tour" role="dialog" aria-modal="true" aria-label="Обучение">
      {box
        ? <div className="tour-hole" style={{ top: box.top - 6, left: box.left - 6, width: box.width + 12, height: box.height + 12 }} />
        : <div className="tour-dim" />}
      <div className="tour-pop card" style={pop}>
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
