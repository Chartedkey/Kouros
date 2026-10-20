// Muscle map: a simple own silhouette, front and back, groups coloured by how much load is still
// "in" them against their usual level (from `engine_summary.muscleMap`).
import { useState, type ReactNode } from 'react';
import { sportOf, type MuscleGroupState } from '../lib/api';

const COLOR: Record<string, string> = { fresh: '#4A6A8A', normal: '#3DDC97', loaded: '#F5B642', over: '#FF6B6B' };
const WORD: Record<string, string> = { fresh: 'свежие', normal: 'в норме', loaded: 'нагружены', over: 'перегружены' };
const BASE = '#1E232A';
const LINE = '#2E343D';

type Shape = ['e', number, number, number, number] | ['c', number, number, number] | ['r', number, number, number, number, number];

const FRONT: [string | null, Shape[]][] = [
  [null, [['c', 100, 28, 18], ['r', 92, 45, 16, 12, 4], ['c', 42, 178, 7], ['c', 158, 178, 7], ['e', 84, 312, 9, 28], ['e', 116, 312, 9, 28], ['e', 80, 362, 11, 5], ['e', 120, 362, 11, 5]]],
  ['shoulders', [['e', 62, 70, 16, 12], ['e', 138, 70, 16, 12]]],
  ['chest', [['r', 66, 62, 33, 34, 10], ['r', 101, 62, 33, 34, 10]]],
  ['arms', [['e', 50, 102, 10, 24], ['e', 150, 102, 10, 24], ['e', 44, 148, 8, 22], ['e', 156, 148, 8, 22]]],
  ['core', [['r', 72, 98, 56, 62, 12]]],
  ['hips', [['e', 86, 170, 12, 9], ['e', 114, 170, 12, 9]]],
  ['quads', [['e', 84, 222, 15, 38], ['e', 116, 222, 15, 38]]],
  ['knees', [['c', 84, 268, 9], ['c', 116, 268, 9]]],
  ['ankles', [['e', 84, 346, 7, 7], ['e', 116, 346, 7, 7]]],
];
const BACK: [string | null, Shape[]][] = [
  [null, [['c', 100, 28, 18], ['r', 92, 45, 16, 12, 4], ['c', 42, 178, 7], ['c', 158, 178, 7], ['c', 84, 268, 8], ['c', 116, 268, 8], ['e', 80, 362, 11, 5], ['e', 120, 362, 11, 5]]],
  ['shoulders', [['e', 62, 70, 16, 12], ['e', 138, 70, 16, 12]]],
  ['back', [['r', 68, 58, 64, 58, 14], ['r', 76, 118, 48, 40, 10]]],
  ['arms', [['e', 50, 102, 10, 24], ['e', 150, 102, 10, 24], ['e', 44, 148, 8, 22], ['e', 156, 148, 8, 22]]],
  ['hips', [['e', 70, 168, 6, 10], ['e', 130, 168, 6, 10]]],
  ['glutes', [['e', 86, 180, 15, 14], ['e', 114, 180, 15, 14]]],
  ['hams', [['e', 84, 230, 14, 32], ['e', 116, 230, 14, 32]]],
  ['calves', [['e', 84, 302, 11, 26], ['e', 116, 302, 11, 26]]],
  ['ankles', [['e', 84, 340, 5, 10], ['e', 116, 340, 5, 10]]],
];

function draw(s: Shape, fill: string, k: string, onClick?: () => void, sel?: boolean): ReactNode {
  const common = { fill, stroke: sel ? '#f1f3f5' : LINE, strokeWidth: sel ? 2 : 1, onClick, style: onClick ? { cursor: 'pointer' } : undefined };
  if (s[0] === 'e') return <ellipse key={k} cx={s[1]} cy={s[2]} rx={s[3]} ry={s[4]} {...common} />;
  if (s[0] === 'c') return <circle key={k} cx={s[1]} cy={s[2]} r={s[3]} {...common} />;
  return <rect key={k} x={s[1]} y={s[2]} width={s[3]} height={s[4]} rx={s[5]} {...common} />;
}

function Figure({ parts, by, sel, onPick, title }: { parts: [string | null, Shape[]][]; by: Record<string, MuscleGroupState>; sel: string | null; onPick: (k: string) => void; title: string }) {
  return (
    <div className="stack" style={{ gap: 4, alignItems: 'center', flex: '1 1 160px' }}>
      <svg viewBox="0 0 200 372" width="100%" style={{ maxWidth: 220 }} aria-label={title}>
        {parts.map(([k, shapes], i) =>
          shapes.map((s, j) => {
            const g = k ? by[k] : null;
            const fill = k ? (g ? COLOR[g.state] : BASE) : BASE;
            return draw(s, fill, `${i}-${j}`, k ? () => onPick(k) : undefined, k != null && k === sel);
          }),
        )}
      </svg>
      <span className="muted tiny">{title}</span>
    </div>
  );
}

export function MuscleMap({ groups, onOpen }: { groups: MuscleGroupState[]; onOpen?: (id: string) => void }) {
  const by = Object.fromEntries(groups.map((g) => [g.key, g]));
  const [sel, setSel] = useState<string | null>(null);
  const g = sel ? by[sel] : null;
  const hot = groups.filter((x) => x.state === 'loaded' || x.state === 'over').sort((a, b) => (b.ratio ?? 0) - (a.ratio ?? 0));
  return (
    <section className="card" aria-label="Карта мышц">
      <div className="between">
        <span className="label">Карта мышц · что ещё не восстановилось</span>
        <span className="row tiny" style={{ gap: 10 }}>
          {(['fresh', 'normal', 'loaded', 'over'] as const).map((k) => <span key={k} className="row" style={{ gap: 4 }}><span className="dot" style={{ background: COLOR[k] }} />{WORD[k]}</span>)}
        </span>
      </div>
      <div className="row" style={{ gap: 16, alignItems: 'flex-start', flexWrap: 'wrap' }}>
        <Figure parts={FRONT} by={by} sel={sel} onPick={setSel} title="спереди" />
        <Figure parts={BACK} by={by} sel={sel} onPick={setSel} title="сзади" />
        <div className="stack" style={{ gap: 8, flex: '2 1 260px', minWidth: 0 }}>
          {g ? (
            <>
              <b>{g.name} — <span style={{ color: COLOR[g.state] }}>{WORD[g.state]}</span></b>
              <span className="muted small">{g.ratio != null ? `Сейчас ${Math.round(g.ratio * 100)}% от твоего обычного уровня` : 'Обычный уровень ещё набирается'}</span>
              {g.contrib.length ? g.contrib.map((c) => {
                const sp = sportOf(c.sport);
                return (
                  <button key={c.id} type="button" className="list-row plain small" onClick={onOpen ? () => onOpen(c.id) : undefined}>
                    <span className="badge" style={{ color: sp.color }}>{sp.letter}</span>
                    <span style={{ flex: 1, textAlign: 'left' }}>{c.name || sp.name} <span className="muted">· {new Date(c.start * 1000).toLocaleDateString('ru-RU', { weekday: 'short', day: 'numeric', month: 'short' })}</span></span>
                    <span className="num">{c.share}%</span>
                  </button>
                );
              }) : <span className="muted small">За последние дни эту группу почти не нагружал.</span>}
            </>
          ) : (
            <>
              <span className="small">{hot.length ? `Больше всего нагружены: ${hot.slice(0, 3).map((x) => x.name.toLowerCase()).join(', ')}.` : 'Ничего не перегружено.'}</span>
              <span className="muted small">Нажми на группу мышц — покажу, какие тренировки её нагрузили.</span>
            </>
          )}
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>Оценка по тренировкам: дистанция и темп, подъёмы и особенно спуски (квадрицепсы и колени), упражнения в подходах. Нагрузка постепенно «выветривается» — примерно вдвое за сутки. Мышцы часы не измеряют, так что это ориентир, а не прибор.</span>
        </div>
      </div>
    </section>
  );
}
