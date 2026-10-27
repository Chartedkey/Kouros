// Workout icons (1.4.1): drawings by a friend of the owner, cut by tools/icons/cut.py into
// public/sport/<kind>.webp (256 px, transparent). Used from ~28 px up; a thin ring around the
// icon shows how hard the workout felt (RPE 1–10) when it is known.
import { sportOf } from '../lib/api';

const HAS = new Set(['run', 'treadmill', 'walk', 'hike', 'ride', 'ride_virtual', 'swim', 'strength', 'workout', 'yoga', 'elliptical', 'stepper', 'other']);

/** Running indoors: Strava's VirtualRun, or a run with distance and no GPS track. */
export function sportKey(a: { sport: string; sportLabel?: string | null; hasGps?: boolean | null; distanceM?: number | null }): string {
  if (a.sport === 'run' && (a.sportLabel?.toLowerCase() === 'virtualrun' || (a.hasGps === false && (a.distanceM ?? 0) > 0))) return 'treadmill';
  return a.sport;
}

const rpeColor = (r: number) => (r <= 3 ? '#3DDC97' : r <= 6 ? '#F5B642' : r <= 8 ? '#FF8A65' : '#FF6B6B');

export function SportIcon({ sport, size = 32, rpe, title }: { sport: string; size?: number; rpe?: number | null; title?: string }) {
  const sp = sportOf(sport === 'treadmill' ? 'run' : sport);
  const key = HAS.has(sport) ? sport : 'other';
  const name = title ?? (sport === 'treadmill' ? 'Беговая дорожка' : sp.name);
  const ring = rpe != null && rpe > 0;
  const pad = ring ? Math.max(3, Math.round(size * 0.09)) : 0;
  const outer = size + pad * 2;
  const r = outer / 2 - 1.5, c = 2 * Math.PI * r;
  return (
    <span className="sport-icon" style={{ width: outer, height: outer }} title={ring ? `${name} · тяжесть ${rpe}` : name} role="img" aria-label={name}>
      {ring && (
        <svg width={outer} height={outer} viewBox={`0 0 ${outer} ${outer}`} aria-hidden="true">
          <circle cx={outer / 2} cy={outer / 2} r={r} fill="none" stroke="#22262D" strokeWidth="2.5" />
          <circle cx={outer / 2} cy={outer / 2} r={r} fill="none" stroke={rpeColor(rpe!)} strokeWidth="2.5" strokeLinecap="round"
            strokeDasharray={`${(c * Math.min(10, rpe!)) / 10} ${c}`} transform={`rotate(-90 ${outer / 2} ${outer / 2})`} />
        </svg>
      )}
      <img src={`sport/${key}.webp`} alt="" draggable={false} style={{ width: size * 1.12, height: size * 1.12 }} />
    </span>
  );
}
