import type { VitalRow } from '../lib/api';

type Comp = { key: string; score: number | null; weight: number };

/** Points a factor adds to or takes from the neutral 50: weight × (score − 50) / Σ weights. */
export function pointsOf(c: Comp, all: Comp[]): number | null {
  if (c.score == null) return null;
  const wsum = all.filter((x) => x.score != null).reduce((a, x) => a + x.weight, 0);
  if (!wsum) return null;
  return (c.weight * (c.score - 50)) / wsum;
}

export function fmtPts(p: number | null): string {
  if (p == null) return '—';
  const a = Math.abs(p);
  if (a < 0.5) return '0';
  const t = a < 1 ? a.toFixed(1).replace('.', ',') : String(Math.round(a));
  return `${p > 0 ? '+' : '−'}${t}`;
}

const STATE: Record<string, [string, string]> = {
  normal: ['var(--text-2)', 'в норме'],
  better: ['#3DDC97', 'лучше обычного'],
  worse: ['#F5B642', 'хуже обычного'],
  bad: ['#FF6B6B', 'заметно хуже'],
  unknown: ['var(--muted)', 'норма набирается'],
};
const f = (x: number | null) => (x == null ? '—' : String(x).replace('.', ','));

export const vitalColor = (state: string) => (STATE[state] ?? STATE.unknown)[0];

/** One vital with its corridor: «норма 12,0–13,2», coloured by where today's value falls. */
export function Vital({ v }: { v: VitalRow }) {
  const [c, word] = STATE[v.state] ?? STATE.unknown;
  return (
    <div className="vital">
      <span className="muted tiny">{v.label}</span>
      <span className="row" style={{ gap: 6, alignItems: 'baseline' }}>
        <span className="num" style={{ fontSize: 18, fontWeight: 700, color: v.state === 'normal' || v.state === 'unknown' ? undefined : c }}>{f(v.value)}</span>
        <span className="muted tiny">{v.unit}</span>
      </span>
      <span className="tiny" style={{ fontWeight: 600, color: c }} title={word}>
        {v.lo == null || v.hi == null ? 'норма набирается' : `норма ${f(v.lo)}–${f(v.hi)}`}
      </span>
    </div>
  );
}
