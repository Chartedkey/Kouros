import catalog from '../data/exercise-catalog.json';
import type { ExerciseItem, ExerciseLibrary } from './api';

export type CatalogVariant = (typeof catalog.variants)[number];
const variants = new Map(catalog.variants.map((x) => [x.id, x]));
export const catalogFamilies: Record<string, string> = catalog.families;
export const catalogVariant = (id: string) => variants.get(id);

/** Presentation only: existing anatomy, load coefficients and IDs remain authoritative. */
export function catalogExercise(ex: ExerciseItem): ExerciseItem {
  const v = variants.get(ex.id);
  if (!v) return ex;
  const displayEquipment: Record<string,string> = { bodyweight:'body only', 'bodyweight+platform':'body only', parallel_bars:'body only', back_extension_bench:'body only', long_band:'bands', 'long_band+pullup_bar':'bands with pullup bar', mini_band:'mini bands', kettlebell:'kettlebells', kettlebell_or_dumbbell:'kettlebell or dumbbell', dumbbell:'dumbbell', barbell:'barbell', ez_bar:'e-z curl bar', machine:'machine', cable:'cable' };
  return { ...ex, ru: v.name, eq: displayEquipment[v.equipment] ?? ex.eq, syn: [...new Set([...ex.syn, ex.ru, ...v.search_aliases])],
    media: 'media' in v ? v.media : ex.media, mediaType: 'media' in v ? 'illustration' : ex.mediaType,
    purpose: v.purpose, family: v.family, catalogEquipment: v.equipment, visibility: v.visibility,
    measurement: ex.measurement ?? (ex.st ? 'seconds' : v.measurement === 'contacts' ? 'contacts' : 'reps') };
}
export function catalogItems(lib: ExerciseLibrary | null): ExerciseItem[] {
  const order = Object.keys(catalogFamilies);
  return (lib?.items ?? []).map(catalogExercise).sort((a,b) => {
    const i = order.indexOf(a.family ?? ''), j = order.indexOf(b.family ?? '');
    return (i < 0 ? 999 : i) - (j < 0 ? 999 : j) || a.ru.localeCompare(b.ru, 'ru');
  });
}
export function equipmentGroup(x: ExerciseItem): string {
  const v = x.catalogEquipment;
  if (v) return ({ bodyweight: 'body', 'bodyweight+platform': 'body', parallel_bars: 'body', back_extension_bench: 'body', long_band: 'bands', 'long_band+pullup_bar': 'bands', mini_band: 'mini', kettlebell: 'kb', kettlebell_or_dumbbell: x.eq === 'dumbbell' ? 'dumbbell' : 'kb', dumbbell: 'dumbbell', barbell: 'barbell', ez_bar: 'barbell', machine: 'machine', cable: 'machine' } as Record<string,string>)[v] ?? 'other';
  if (x.eq === 'mini bands' || x.id.startsWith('k_mb_')) return 'mini';
  return ({ 'body only': 'body', bands: 'bands', kettlebells: 'kb', dumbbell: 'dumbbell', barbell: 'barbell', 'e-z curl bar': 'barbell', machine: 'machine', cable: 'machine' } as Record<string,string>)[x.eq ?? ''] ?? 'other';
}
export function hasPurpose(x: ExerciseItem, kind: string, lib: ExerciseLibrary | null): boolean {
  if (kind === 'ofp') return x.purpose?.includes('ofp_running') || (lib?.runnerOfp ?? []).includes(x.id) || /^k_(rb|fb)_/.test(x.id);
  if (kind === 'stretch') return x.cat === 'stretching' || !!x.purpose?.includes('mobility');
  return x.cat !== 'stretching';
}
export function filterCatalog(items: ExerciseItem[], scope: 'primary' | 'expanded' | 'legacy'): ExerciseItem[] {
  return items.filter((x) => x.own || scope === 'legacy' || x.visibility === 'primary' || scope === 'expanded' && x.visibility === 'expanded');
}
export function exerciseImageStyle(ex: ExerciseItem) {
  return { backgroundImage: `url("${ex.media ?? `ex/${ex.id}.webp`}")`, ...(ex.media ? { backgroundSize: 'contain', backgroundPosition: 'center', backgroundRepeat: 'no-repeat', backgroundColor: '#0b0d10' } : {}) };
}
export const hasExerciseImage = (ex: ExerciseItem) => !!ex.media || ex.img > 0;
