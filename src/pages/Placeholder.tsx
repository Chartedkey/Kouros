import type { Tab } from '../components/Header';

type Soon = [string, string];

const PAGES: Record<Exclude<Tab, 'settings'>, { title: string; subtitle: string; soon: Soon[] }> = {
  today: {
    title: 'Сегодня',
    subtitle: 'Готовность, нагрузка, сон и план дня',
    soon: [
      ['Кольцо готовности и факторы: сон, HRV, пульс покоя, нагрузка', 'Этап 3'],
      ['Неделя, напоминания, стресс и самочувствие', 'Этап 4'],
      ['Карточка тренера и план дня', 'Этап 5'],
    ],
  },
  training: {
    title: 'Тренировки',
    subtitle: 'Все активности из Strava — бег, ходьба, вело, силовые и физиотерапия',
    soon: [
      ['Синхронизация со Strava и вся история тренировок', 'Этап 2'],
      ['Импорт и экспорт: .fit, .gpx, .tcx, архив Strava', 'Этап 2'],
      ['Нагрузка, зоны, рекорды, прогнозы, инвентарь', 'Этапы 3–4'],
    ],
  },
  health: {
    title: 'Здоровье',
    subtitle: 'Сон, сердце, тело, привычки и анализы — из Apple Health',
    soon: [
      ['Подключение Apple Health через freddy', 'Этап 2'],
      ['Сон, дыхание, пульс, давление, состав тела', 'Этапы 3–4'],
      ['Анализы с нормами лаборатории', 'Этап 7'],
    ],
  },
  nutrition: {
    title: 'Питание',
    subtitle: 'Калории, БЖУ, микроэлементы, вода и добавки',
    soon: [
      ['Данные YAZIO и Waterllama через Apple Health', 'Этап 2'],
      ['Цели, качество рациона, режим питания', 'Этапы 3–4'],
      ['Добавки, меню и список покупок', 'Этап 7'],
    ],
  },
  psychology: {
    title: 'Психология',
    subtitle: 'Опросники, мотивация и риск перегруза',
    soon: [
      ['Опросники: BRUMS, Хупер, RESTQ', 'Этап 4'],
      ['Тренды и риск выгорания', 'Этапы 3–4'],
    ],
  },
  coach: {
    title: 'Тренер',
    subtitle: 'Claude — отчёты, гибкий план и анализ недели',
    soon: [
      ['Утренний, дневной и ночной отчёты', 'Этап 5'],
      ['План, который подстраивается под факт', 'Этап 6'],
      ['Восстановление, журнал боли, профиль общения', 'Этапы 5–6'],
    ],
  },
};

function EmptyRing() {
  return (
    <svg className="ring-empty" viewBox="0 0 168 168" role="img" aria-label="Готовность появится после подключения данных">
      <circle cx="84" cy="84" r="70" fill="none" stroke="#22262D" strokeWidth="14" />
      <circle cx="84" cy="84" r="70" fill="none" stroke="#3DDC97" strokeWidth="14" strokeLinecap="round"
        strokeDasharray="6 434" transform="rotate(-90 84 84)" />
      <text x="84" y="92" textAnchor="middle" fill="#6B737E" fontFamily="Unbounded, sans-serif" fontWeight="700" fontSize="26">—</text>
    </svg>
  );
}

export function Placeholder({ tab, onSettings }: { tab: Exclude<Tab, 'settings'>; onSettings: () => void }) {
  const p = PAGES[tab];
  return (
    <>
      <div className="page-head">
        <div className="stack">
          <h1>{p.title}</h1>
          <p className="subtitle">{p.subtitle}</p>
        </div>
      </div>
      <div className="grid">
        <section className="card" aria-label="Что здесь будет">
          <span className="label">Что здесь будет</span>
          <ul className="soon-list">
            {p.soon.map(([text, stage]) => (
              <li key={text}>
                <span>{text}</span>
                <span className="stage">{stage}</span>
              </li>
            ))}
          </ul>
        </section>
        <section className="card" aria-label="Данные">
          {tab === 'today' ? (
            <div className="row" style={{ gap: 24, flexWrap: 'nowrap' }}>
              <EmptyRing />
              <div className="stack">
                <span className="label">Готовность</span>
                <span style={{ fontSize: 15, lineHeight: 1.5 }}>
                  Появится, когда Kouros получит сон, пульс и тренировки.
                </span>
              </div>
            </div>
          ) : (
            <span className="label">Пока нет данных</span>
          )}
          <span className="muted small" style={{ lineHeight: 1.55 }}>
            Данные уже собираются: тренировки — во вкладке «Тренировки», показатели Apple Health — во вкладке «Здоровье».
            Расчёты и экраны этой вкладки появятся на следующих этапах.
          </span>
          <button type="button" className="btn2" style={{ alignSelf: 'flex-start' }} onClick={onSettings}>
            Открыть настройки
          </button>
        </section>
      </div>
    </>
  );
}
