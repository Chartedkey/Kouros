import type { StressDay } from '../lib/api';

export const STRESS_COLORS = ['#3DDC97', '#7FD3FF', '#F5B642', '#FF6B6B'];
export const STRESS_NAMES = ['покой', 'низкий', 'средний', 'высокий'];
export const stressColor = (v: number) => STRESS_COLORS[v < 25 ? 0 : v < 50 ? 1 : v < 75 ? 2 : 3];

function hmin(m: number) {
  const h = Math.floor(m / 60), r = Math.round(m % 60);
  return h ? `${h} ч${r ? ` ${r} мин` : ''}` : `${r} мин`;
}

/** Day stress: score vs your usual, 15-minute timeline, minutes by level. */
export function StressCard({ st, title = 'Стресс днём' }: { st: StressDay | null | undefined; title?: string }) {
  if (!st || st.score == null) {
    return (
      <section className="card" aria-label={title}>
        <span className="label">{title}</span>
        <span className="muted small">Пока мало данных: нужен пульс с часов в спокойные минуты. Тренировки, ходьба и сон не считаются.</span>
      </section>
    );
  }
  const c = stressColor(st.score);
  const total = st.levels.reduce((a, b) => a + b, 0) || 1;
  return (
    <section className="card" aria-label={title}>
      <div className="between">
        <span className="label">{title}</span>
        <span className="chip" style={{ background: '#1B1F25', color: c }}>{st.level}</span>
      </div>
      <div className="row" style={{ gap: 16, alignItems: 'baseline' }}>
        <b className="num" style={{ fontSize: 34, color: c }}>{st.score}</b>
        <span className="muted small">из 100{st.norm != null ? ` · обычно ${st.norm}` : ''} · {st.from}–{st.to}</span>
      </div>
      <div className="stress-line" aria-hidden="true">
        {st.bins.map(([t, v]) => (
          <span key={t} style={{ height: `${Math.max(6, v)}%`, background: stressColor(v) }} title={`${new Date(t * 1000).toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' })} · ${v}`} />
        ))}
      </div>
      <div className="stress-split" aria-hidden="true">
        {st.levels.map((m, i) => (m > 0 ? <span key={i} style={{ flex: m / total, background: STRESS_COLORS[i] }} /> : null))}
      </div>
      <div className="row" style={{ gap: 14, flexWrap: 'wrap' }}>
        {st.levels.map((m, i) => (
          <span key={i} className="tiny row" style={{ gap: 6 }}>
            <span style={{ width: 8, height: 8, borderRadius: 999, background: STRESS_COLORS[i], display: 'inline-block' }} />
            {STRESS_NAMES[i]} {hmin(m)}
          </span>
        ))}
      </div>
      <span className="muted tiny">По пульсу и HRV в спокойные минуты. Тренировки (и 20 минут после них), ходьба и сон не считаются.</span>
    </section>
  );
}
