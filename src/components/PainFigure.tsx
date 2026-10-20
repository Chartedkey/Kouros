// Body figure for marking pain (front / back, left / right) — the same simple silhouette as the
// muscle map, with joints and feet as their own areas.
import type { ReactNode } from 'react';

type Shape = ['e', number, number, number, number] | ['c', number, number, number] | ['r', number, number, number, number, number];
const BASE = '#1E232A';
const LINE = '#2E343D';

const FRONT: [string | null, Shape[]][] = [
  [null, [['c', 100, 28, 18], ['c', 42, 178, 7], ['c', 158, 178, 7]]],
  ['neck', [['r', 92, 45, 16, 12, 4]]],
  ['shoulders', [['e', 62, 70, 16, 12], ['e', 138, 70, 16, 12]]],
  ['chest', [['r', 66, 62, 33, 34, 10], ['r', 101, 62, 33, 34, 10]]],
  ['arms', [['e', 50, 102, 10, 24], ['e', 150, 102, 10, 24], ['e', 44, 148, 8, 22], ['e', 156, 148, 8, 22]]],
  ['core', [['r', 72, 98, 56, 62, 12]]],
  ['hips', [['e', 86, 170, 12, 9], ['e', 114, 170, 12, 9]]],
  ['quads', [['e', 84, 222, 15, 38], ['e', 116, 222, 15, 38]]],
  ['knees', [['c', 84, 268, 9], ['c', 116, 268, 9]]],
  ['shin', [['e', 84, 312, 9, 28], ['e', 116, 312, 9, 28]]],
  ['ankles', [['e', 84, 346, 7, 7], ['e', 116, 346, 7, 7]]],
  ['foot', [['e', 80, 362, 11, 5], ['e', 120, 362, 11, 5]]],
];
const BACK: [string | null, Shape[]][] = [
  [null, [['c', 100, 28, 18], ['c', 42, 178, 7], ['c', 158, 178, 7]]],
  ['neck', [['r', 92, 45, 16, 12, 4]]],
  ['shoulders', [['e', 62, 70, 16, 12], ['e', 138, 70, 16, 12]]],
  ['back', [['r', 68, 58, 64, 58, 14], ['r', 76, 118, 48, 40, 10]]],
  ['arms', [['e', 50, 102, 10, 24], ['e', 150, 102, 10, 24], ['e', 44, 148, 8, 22], ['e', 156, 148, 8, 22]]],
  ['glutes', [['e', 86, 180, 15, 14], ['e', 114, 180, 15, 14]]],
  ['hams', [['e', 84, 230, 14, 32], ['e', 116, 230, 14, 32]]],
  ['knees', [['c', 84, 268, 8], ['c', 116, 268, 8]]],
  ['calves', [['e', 84, 302, 11, 26], ['e', 116, 302, 11, 26]]],
  ['ankles', [['e', 84, 340, 5, 10], ['e', 116, 340, 5, 10]]],
  ['foot', [['e', 80, 362, 11, 5], ['e', 120, 362, 11, 5]]],
];

export function painColor(level: number | undefined): string {
  if (level == null || level <= 0) return BASE;
  if (level <= 3) return '#8A7A3A';
  if (level <= 6) return '#F5B642';
  return '#FF6B6B';
}

const cx = (s: Shape) => (s[0] === 'r' ? s[1] + s[3] / 2 : s[1]);

/** Side of the body for a shape: on the front view the athlete's right is on the left of the picture. */
function sideOf(s: Shape, back: boolean): string {
  const x = cx(s);
  if (Math.abs(x - 100) < 8) return '';
  const left = x < 100;
  return back ? (left ? 'L' : 'R') : (left ? 'R' : 'L');
}

function Fig({ parts, back, levels, sel, onPick, title }: {
  parts: [string | null, Shape[]][]; back: boolean; levels: Record<string, number>; sel: string | null; onPick?: (area: string, side: string) => void; title: string;
}) {
  const draw = (s: Shape, k: string, area: string | null): ReactNode => {
    const side = area ? sideOf(s, back) : '';
    const key = area ? `${area}:${side}` : '';
    const lv = area ? Math.max(levels[key] ?? 0, levels[`${area}:`] ?? 0) : 0;
    const on = sel === key;
    const common = {
      fill: area ? painColor(lv) : BASE, stroke: on ? '#f1f3f5' : LINE, strokeWidth: on ? 2 : 1,
      onClick: area && onPick ? () => onPick(area, side) : undefined, style: area && onPick ? { cursor: 'pointer' } : undefined,
    };
    if (s[0] === 'e') return <ellipse key={k} cx={s[1]} cy={s[2]} rx={s[3]} ry={s[4]} {...common} />;
    if (s[0] === 'c') return <circle key={k} cx={s[1]} cy={s[2]} r={s[3]} {...common} />;
    return <rect key={k} x={s[1]} y={s[2]} width={s[3]} height={s[4]} rx={s[5]} {...common} />;
  };
  return (
    <div className="stack" style={{ gap: 4, alignItems: 'center', flex: '1 1 140px' }}>
      <svg viewBox="0 0 200 372" width="100%" style={{ maxWidth: 200 }} aria-label={title}>
        {parts.map(([a, shapes], i) => shapes.map((s, j) => draw(s, `${i}-${j}`, a)))}
      </svg>
      <span className="muted tiny">{title}</span>
    </div>
  );
}

/** `levels` keys are `area:side` (side '' = both / middle). */
export function PainFigure({ levels, sel, onPick }: { levels: Record<string, number>; sel?: string | null; onPick?: (area: string, side: string) => void }) {
  return (
    <div className="row" style={{ gap: 12, alignItems: 'flex-start', flexWrap: 'wrap', justifyContent: 'center' }}>
      <Fig parts={FRONT} back={false} levels={levels} sel={sel ?? null} onPick={onPick} title="спереди" />
      <Fig parts={BACK} back levels={levels} sel={sel ?? null} onPick={onPick} title="сзади" />
    </div>
  );
}

export const sideText = (s: string) => (s === 'L' ? 'слева' : s === 'R' ? 'справа' : '');
