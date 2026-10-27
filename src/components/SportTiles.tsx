// 1.4.3: the summary tiles of a workout show what makes sense for its kind — pace for runs, speed
// for rides, pace per 100 m for swims, sets and tonnage for strength, calories and effort for
// the elliptical / stepper / yoga. Relative Effort from Strava goes small under the pulse.
import { fmtDur, fmtKm, fmtPace, type ActivityRow } from '../lib/api';
import { sportKey } from './SportIcon';

export type Tile = { v: string; l: string; sub?: string };

export function sportTiles(a: ActivityRow, x?: { weightLiftedKg?: number | null; setsCount?: number | null } | null): Tile[] {
  const moving = a.movingS ?? a.elapsedS;
  const time: Tile = { v: moving ? fmtDur(moving) : '—', l: a.movingS ? 'в движении' : 'длительность' };
  const hr: Tile = { v: `${a.avgHr ? Math.round(a.avgHr) : '—'} / ${a.maxHr ? Math.round(a.maxHr) : '—'}`, l: 'пульс ср / макс', sub: a.relativeEffort ? `Relative Effort ${Math.round(a.relativeEffort)} (Strava)` : undefined };
  const dist: Tile = { v: a.distanceM ? fmtKm(a.distanceM, 2) : '—', l: 'дистанция' };
  const k = sportKey(a);
  if (k === 'run' || k === 'treadmill') {
    return [dist, time, { v: a.distanceM && moving ? fmtPace(moving / (a.distanceM / 1000)) : '—', l: 'темп, /км' }, hr];
  }
  if (k === 'walk' || k === 'hike') return [dist, time, { v: a.elevGainM ? `${Math.round(a.elevGainM)} м` : '—', l: 'набор высоты' }, hr];
  if (k === 'ride' || k === 'ride_virtual') {
    return [dist, time, { v: a.distanceM && moving ? `${String(Math.round((a.distanceM / moving) * 36) / 10).replace('.', ',')} км/ч` : '—', l: 'средняя скорость' }, hr];
  }
  if (k === 'swim') {
    const p = a.distanceM && moving ? moving / (a.distanceM / 100) : null;
    return [dist, time, { v: p ? fmtPace(p) : '—', l: 'темп, /100 м' }, hr];
  }
  if (k === 'strength' || k === 'workout') {
    const sets = x?.setsCount ?? a.totalSets;
    const t = x?.weightLiftedKg;
    return [{ v: sets ? String(sets) : '—', l: 'подходов' }, { v: t ? `≈${Math.round(t).toLocaleString('ru-RU')} кг` : '—', l: 'тоннаж' }, time, hr];
  }
  return [time, { v: a.calories ? `${Math.round(a.calories)}` : '—', l: 'ккал' }, hr, { v: a.rpe ? String(a.rpe) : '—', l: 'тяжесть, 1–10' }];
}

export function SportTiles({ a, x, small }: { a: ActivityRow; x?: { weightLiftedKg?: number | null; setsCount?: number | null } | null; small?: boolean }) {
  return (
    <div className="tiles4">
      {sportTiles(a, x).map((t) => (
        <div key={t.l} className="tile">
          <span className={`num ${small ? 'tile-v2' : 'tile-v'}`}>{t.v}</span>
          <span className="muted tiny">{t.l}</span>
          {t.sub && <span className="muted tiny" style={{ opacity: 0.8 }}>{t.sub}</span>}
        </div>
      ))}
    </div>
  );
}
