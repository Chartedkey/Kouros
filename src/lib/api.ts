// Thin wrapper over Tauri commands. Outside Tauri (plain `npm run dev` in a
// browser) it answers with demo data so the UI can be worked on without the app.
import { invoke } from '@tauri-apps/api/core';

export type Status = {
  version: string;
  platform: string;
  dataDir: string | null;
  dbOk: boolean;
  dbError: string | null;
  schemaVersion: number;
  dbSize: number;
  cipherVersion: string | null;
  hello: 'available' | 'not_configured' | 'unavailable';
  lockEnabled: boolean;
  unlocked: boolean;
};

export type Source = {
  id: string;
  name: string;
  connected: boolean;
  lastSync: string | null;
  lastError: string | null;
};

export type BackupInfo = { name: string; size: number; modified: number };

export type SecretName =
  | 'strava_client_id'
  | 'strava_client_secret'
  | 'freddy_api_key'
  | 'anthropic_api_key';

export const inTauri =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in (window as object);

// ---------- demo backend for the browser ----------
const demo = {
  secrets: {} as Record<string, boolean>,
  backups: [{ name: 'kouros-2026-10-17_005000.db', size: 98304, modified: Date.now() / 1000 }] as BackupInfo[],
  lock: true,
  unlocked: false,
  imported: true,
  cond: null as { date: string; temp: string; surface: string } | null,
  ofp: {} as Record<string, OfpBlock[]>,
  complexes: [
    { id: 'x_demo1', name: 'Пример: колени', note: 'Демо-комплекс', areas: ['knees'], items: [{ exId: 'Bodyweight_Squat' }, { exId: 'k_bird_dog' }, { exId: 'Plank' }] },
    { id: 'x_demo2', name: 'Пример: стопы', note: '', areas: ['foot', 'ankles'], items: [{ exId: 'k_eccentric_heel_drop' }, { exId: 'c_demo1' }] },
  ] as Complex[],
  custom: [
    { id: 'c_demo1', ru: 'Сбор полотенца пальцами (пример)', en: '', syn: [], tip: 'Сидя, пальцами стопы подтягивай полотенце к себе.', cat: 'strength', eq: 'body only', lvl: '', mech: '', force: '', p: ['calves'], s: [], bw: null, st: false, leg: true, img: 0, own: true },
  ] as ExerciseItem[],
};

const DEMO_STRESS = {
  score: 38, level: 'низкий', norm: 33, hr: 41, hrv: 33, from: '12:40', to: '23:10', levels: [210, 240, 120, 30] as [number, number, number, number],
  bins: Array.from({ length: 42 }, (_, k) => [1792252800 + k * 900, Math.round(Math.max(0, 30 + 25 * Math.sin(k / 4) + (k % 7) * 4))] as [number, number]),
};

const DEMO_SUMMARY = {
  today: '2026-10-17', hrRestUsed: 53,
  steps: { today: 6420, usual: 9100, avg7: 8650, last14: [] as [string, number][] },
  muscleMissing: [{ id: 's3', sport: 'strength', start: 1792300000, name: 'Силовая', free: 18, total: 18 }],
  // Fictional muscle state for the demo map (1.2).
  muscleMap: ([
    ['calves', 'Икры', 'loaded', 1.38], ['quads', 'Квадрицепсы', 'over', 1.74], ['hams', 'Задняя поверхность бедра', 'loaded', 1.31],
    ['glutes', 'Ягодицы', 'loaded', 1.25], ['hips', 'Таз и бёдра (стабилизаторы)', 'normal', 1.05], ['core', 'Кор', 'normal', 0.9],
    ['back', 'Спина', 'fresh', 0.4], ['chest', 'Грудь', 'fresh', 0.3], ['shoulders', 'Плечи', 'fresh', 0.35], ['arms', 'Руки', 'fresh', 0.5],
    ['knees', 'Колени', 'loaded', 1.4], ['ankles', 'Ахиллы и голеностоп', 'normal', 1.1],
  ] as const).map(([key, name, state, ratio]) => ({
    key, name, state, ratio, now: Math.round(ratio * 40), usual: 40,
    contrib: state === 'fresh' ? [] : [{ id: 's2', sport: 'run', start: 1792180000, name: 'Длительная', share: 64 }, { id: 's1', sport: 'run', start: 1792440000, name: 'Вечерний бег', share: 36 }],
  })),

  profile: { hrMax: 184, hrRest: null, zones: [110, 129, 147, 160, 171, 184], heightCm: 178, birthYear: 1990, male: true, proteinGPerKg: 1.8, zonesDate: null, thresholdPaceS: 330 },
  readiness: {
    score: 51, level: 'yellow',
    components: [
      { key: 'hrv', label: 'HRV', value: 38.3, baseline: 53, score: 32, weight: 0.3, note: '38 мс при норме 53' },
      { key: 'rhr', label: 'Пульс покоя', value: 50, baseline: 55, score: 74, weight: 0.2, note: '50 уд/мин при норме 55' },
      { key: 'sleep', label: 'Сон', value: null, baseline: null, score: null, weight: 0.25, note: 'сон ещё не записан' },
      { key: 'form', label: 'Свежесть после нагрузки', value: -3.1, baseline: null, score: 57, weight: 0.15, note: 'форма -3' },
    ],
    penalties: [],
    history: [['2026-10-12', 52], ['2026-10-13', 60], ['2026-10-14', 66], ['2026-10-15', 24], ['2026-10-16', 35], ['2026-10-17', 51]],
    evening: { score: 44, level: 'yellow', morning: 51, delta: -7, items: [{ text: 'Нагрузка за день — 92', points: -4.8 }, { text: 'Стресс днём 38 — выше обычного (33)', points: -1 }, { text: 'Питание по плану', points: 1 }, { text: 'Твоё восстановление (TQR) — 11', points: -2 }] },
  },
  stress: { today: DEMO_STRESS, history: [['2026-10-15', 41], ['2026-10-16', 29], ['2026-10-17', 38]] },
  load: {
    today: 0, atl: 82.5, ctl: 89.9, tsb: -3.1, acwr: 0.72, week: 513, state: 'low', stateText: 'Ниже обычного — можно добавлять',
    series: Array.from({ length: 60 }, (_, i) => [`d${i}`, (i * 37) % 160, 80 + 20 * Math.sin(i / 5), 88 + i / 10, -5 + 8 * Math.cos(i / 6)]),
  },
  intensity: { d7: { low: 94, mid: 4, high: 2, hours: 9.8 }, d28: { low: 92, mid: 6, high: 2, hours: 41 }, zones28Min: [1300, 600, 420, 40, 12, 2] },
  weeks: [{ week: '2026-10-12', load: 513, hours: 9.8, runKm: 10, count: 9 }],
  records: [{ distanceM: 1000, timeS: 255, date: '2026-10-16', activityId: 's1' }, { distanceM: 5000, timeS: 1380, date: '2026-10-16', activityId: 's1' }],
  predictions: [{ distanceM: 21097.5, timeS: 6100, fromDistanceM: 10000, fromTimeS: 2860, fromDate: '2026-10-16' }],
  sleep: [{ date: '2026-10-16', totalS: 24950, deepS: 7000, remS: 3922, coreS: 14029, awakeS: 3228, score: 74 }],
  body: { weight: { latest: ['2026-10-14', 78.4], smooth: 78.5, change7d: 0.3, change30d: -0.6 }, fatPct: { latest: null, smooth: null, change7d: null, change30d: null }, lean: { latest: null, smooth: null, change7d: null, change30d: null } },
  nutrition: {
    days: [{ date: '2026-10-16', kcal: 2100, protein: 120, carbs: 210, fat: 80, fiber: 14, waterMl: 1800, target: { kcal: 2850, protein: 141, carbs: 380, fat: 71, fiber: 38, waterMl: 3400 } }],
    targetToday: { kcal: 2500, protein: 141, carbs: 310, fat: 71, fiber: 35, waterMl: 3000, bmr: 1760 }, weightUsed: 78.4,
  },
  signals: [{ level: 'warn', code: 'hrv_down', text: 'HRV 38 мс — на 31% ниже нормы' }, { level: 'info', code: 'zones_old', text: 'Зонам 128 дней — пора повторить тест или уточнить пороги' }],
  meta: { activities: 248, byHr: 240, byAvgHr: 6, estimated: 2, ms: 840 },
  day: {
    advice: { title: 'Обычный аэробный день', text: 'Обычный объём во второй зоне, пульс до 147. Силовая — можно.', tier: 'aerobic', bodyTier: 'tempo', reasons: ['по TQR ты восстановлен на 12 из 20'], hint: null, evening: false },
    targetLoad: 72, usualLoad: 90, todayLoad: 38,
    target: { lo: 45, hi: 72, headroom: 72, capped: true, cap: 1.3, left: 21 },
    options: [
      { kind: 'run', title: 'Лёгкий бег ≈ 25 мин', text: 'Вторая зона, пульс до 147' },
      { kind: 'walk', title: 'Ходьба ≈ 45 мин', text: 'Спокойный темп, пульс до 129' },
    ],
    todayActivities: [{ id: 'a', sport: 'walk', name: 'Дневная прогулка', startUtc: 1792235000, durationS: 3600, distanceM: 5400, load: 38, relativeEffort: 6, method: 'hr' }],
    recent: [
      { id: 'a', sport: 'walk', name: 'Дневная прогулка', startUtc: 1792235000, durationS: 3600, distanceM: 5400, load: 38, relativeEffort: 6, method: 'hr' },
      { id: 'b', sport: 'run', name: 'Утренний бег', startUtc: 1792130000, durationS: 2940, distanceM: 8200, load: 118, relativeEffort: 36, method: 'hr' },
    ],
    week: ['2026-10-12', '2026-10-13', '2026-10-14', '2026-10-15', '2026-10-16', '2026-10-17', '2026-10-18'].map((d, i) => ({
      date: d, load: [60, 23, 59, 45, 164, 38, 0][i], sports: [['walk'], ['walk'], ['workout', 'walk'], ['walk'], ['run', 'walk'], ['walk'], []][i],
      minutes: [90, 40, 80, 70, 120, 60, 0][i], readiness: [52, 60, 66, 24, 41, 68, null][i],
    })),
    usualReadiness: 55, alcoholYesterday: null,
    vitals: [
      { key: 'hrv', label: 'HRV', value: 63, unit: 'мс', base: 55, lo: 49, hi: 61, state: 'better' },
      { key: 'rhr', label: 'Пульс покоя', value: 47, unit: 'уд/мин', base: 46, lo: 44, hi: 48, state: 'normal' },
      { key: 'resp', label: 'Дыхание во сне', value: 12.8, unit: '/мин', base: 12.5, lo: 11.9, hi: 13.1, state: 'normal' },
      { key: 'spo2', label: 'Кислород (SpO₂)', value: 94.2, unit: '%', base: 95.6, lo: 94.6, hi: 96.6, state: 'worse' },
    ],
  },
} as unknown as EngineSummary;

const DEMO_ACTS: ActivityRow[] = [
  ['walk', 'Вечерняя прогулка', 1792170000, 2400, 3500, 104],
  ['run', 'Утренний бег', 1792130000, 2940, 8200, 141],
  ['walk', 'Дневная прогулка', 1792050000, 5400, 6500, 98],
  ['workout', 'Растяжка и мобилити', 1791960000, 1800, null, 96],
  ['strength', 'Силовая тренировка', 1791800000, 3300, null, 108],
].map(([sport, name, startUtc, elapsedS, distanceM, avgHr], i) => ({
  id: `s${i}`, stravaId: 1000000000 + i, sport: sport as string, sportLabel: null, name: name as string,
  startUtc: startUtc as number, elapsedS: elapsedS as number, movingS: null, distanceM: distanceM as number | null,
  elevGainM: null, avgHr: avgHr as number, maxHr: null, calories: null, relativeEffort: null, totalSets: null,
  totalReps: null, device: 'Demo', source: 'strava_archive', hasStreams: true, hasGps: !!distanceM, filePath: null,
  rpe: null, load: [60, 141, 70, 30, 40][i],
}));

function demoDetail(id: string): ActivityDetail {
  const a = DEMO_ACTS.find((x) => x.id === id) ?? DEMO_ACTS[1];
  const n = 600, run = a.sport === 'run' || a.sport === 'walk';
  const t = Array.from({ length: n }, (_, i) => i * 6);
  const hr = t.map((x, i) => Math.round((a.avgHr ?? 120) + 10 * Math.sin(i / 40) + (i < 40 ? i / 4 - 10 : 0)));
  const speed = t.map((_, i) => (a.sport === 'run' ? 2.7 : 1.5) + 0.25 * Math.sin(i / 25));
  let d = 0;
  const dist = speed.map((v) => (d += v * 6));
  const strength = !run;
  return {
    activity: a, description: null, notes: null, weightLiftedKg: null, gear: null, load: a.load, loadMethod: 'hr',
    zoneS: [300, 900, 1500, 200, 40, 0],
    charts: run ? { t, hr, dist, speed, alt: t.map((_, i) => 150 + 8 * Math.sin(i / 60)), cad: [], lat: t.map((_, i) => 59.94 + 0.01 * Math.sin(i / 90)), lng: t.map((_, i) => 30.31 + 0.015 * Math.cos(i / 90)) } : null,
    splits: run ? Array.from({ length: Math.floor(d / 1000) }, (_, k) => ({ km: k + 1, timeS: 1000 / (a.sport === 'run' ? 2.7 : 1.5) + (k % 3) * 8, hr: 140 + k, full: true })) : [],
    ofp: demo.ofp[id] ?? [],
    sets: strength ? Array.from({ length: 8 }, (_, k) => ({ idx: k, startUtc: a.startUtc + k * 150, durationS: k % 2 ? 90 : 40, reps: k % 2 ? null : 10, weightKg: k % 2 ? null : 40, kind: (k % 2 ? 'rest' : 'active') as 'rest' | 'active', exercise: null })) : [],
    bodyKg: strength ? 78.4 : null,
  };
}

/** Fictional lab results for the browser demo. */
function demoLabs(): LabsSummary {
  const t = (key: string, name: string, group: string, unit: string, series: [string, number][], lo: number | null, hi: number | null, note: LabNote | null): LabTest => {
    const v = series[series.length - 1][1];
    const status = lo != null && v < lo ? 'low' : hi != null && v > hi ? 'high' : 'ok';
    return { key, known: true, name, printed: name, group, unit, count: series.length, series, note,
      latest: { date: series[series.length - 1][0], value: v, text: String(v), lo, hi, ref: lo != null && hi != null ? `${lo}-${hi}` : hi != null ? `<${hi}` : '', status, comment: '' } };
  };
  const d: LabNote = { level: 'info', title: 'Витамин D 27,5 нг/мл — недостаточно (20–30).', text: 'Целевой уровень 30–60 нг/мл. Осенью и зимой солнца мало — обсуди с врачом профилактическую дозу.', todo: 'Это не диагноз — обсуди результат с врачом.' };
  const ck: LabNote = { level: 'info', title: 'КФК 310 Ед/л — выше нормы.', text: 'КФК выходит из мышц после нагрузки. За 3 дня до сдачи была нагрузка 240 (самая длинная тренировка 95 мин). Перед следующей сдачей — 2–3 дня без тяжёлых и длинных тренировок.', todo: 'Это не диагноз — обсуди результат с врачом.' };
  return {
    reports: [{ id: 2, date: '2026-10-14', lab: 'Пример', file: null, source: 'pdf', count: 5 }, { id: 1, date: '2026-04-02', lab: 'Пример', file: null, source: 'claude', count: 4 }],
    groups: ['Железо', 'Кровь', 'Витамины', 'Мышцы и печень', 'Воспаление'],
    findings: [{ ...d, key: 'vitd', date: '2026-10-14' }, { ...ck, key: 'ck', date: '2026-10-14' }],
    tests: [
      t('ferritin', 'Ферритин', 'Железо', 'мкг/л', [['2026-04-02', 34], ['2026-10-14', 41]], 30, 400, null),
      t('hgb', 'Гемоглобин', 'Кровь', 'г/л', [['2026-04-02', 148], ['2026-10-14', 151]], 130, 170, null),
      t('vitd', 'Витамин D (25-OH)', 'Витамины', 'нг/мл', [['2026-04-02', 21], ['2026-10-14', 27.5]], 30, 100, d),
      t('ck', 'Креатинкиназа (КФК)', 'Мышцы и печень', 'Ед/л', [['2026-04-02', 180], ['2026-10-14', 310]], null, 190, ck),
      t('crp', 'С-реактивный белок', 'Воспаление', 'мг/л', [['2026-04-02', 0.8], ['2026-10-14', 0.6]], null, 5, null),
    ],
  };
}

function demoNutrition(): NutritionSummary {
  const days: NutDay[] = Array.from({ length: 90 }, (_, k) => {
    const d = new Date(Date.UTC(2026, 6, 20) + k * 86400000).toISOString().slice(0, 10);
    const has = k > 60;
    const kc = has ? 1900 + ((k * 397) % 1100) : null;
    return { date: d, kcal: kc, protein: has ? 120 + (k % 40) : null, carbs: has ? 250 + (k % 80) : null, fat: has ? 90 + (k % 50) : null, fiber: has ? 10 + (k % 12) : null, sugar: has ? 40 : null, satFat: has ? 30 : null, waterMl: has ? 1800 + (k % 9) * 200 : null, spent: has ? 2700 + (k % 7) * 120 : null, status: (has ? (k % 9 === 0 ? 1 : 2) : 0) as 0 | 1 | 2, target: { kcal: 2650, protein: 141, carbs: 334, fat: 99, fiber: 42, waterMl: 4243 } };
  });
  return {
    today: days[89].date, tz: 10800, weight: 78.4,
    targets: { kcal: 2650, protein: 141, carbs: 340, fat: 71, fiber: 37, waterMl: 3200, bmr: 1760, neat: 530, train: 209, goalKcal: 0, trainMin: 60 },
    eaten: { kcal: 1840, protein: 112, carbs: 198, fat: 61, fiber: 9, fiberEst: 6.4, waterMl: 1800 },
    goal: { goal: 'maintain', rate: 0.5, targetWeight: null, etaWeeks: null, weekly: 0, proteinPerKg: 1.8, proteinManual: null, monthly: [['2026-05', 80.1], ['2026-06', 79.6], ['2026-07', 79.0], ['2026-08', 78.8], ['2026-09', 78.6], ['2026-10', 78.5]] },
    days,
    quality: { fullDays: 13, fiber: 14.6, fiberWithEst: 22.3, sugar: 40, sugarPct: 5, protein: 199, proteinPerKg: 1.8, fatPct: 35, satFat: 38, vegDays: 7, top: [['Греческий йогурт', 9], ['Гречка', 6], ['Куриная грудка', 5], ['Яблоки', 4]] },
    window: { first: '08:10', last: '21:30', afterMidnight: 0, days: 16, hours: 10.5, parts: [0, 32, 68], byDay: days.slice(-16).reverse().map((d, i) => [d.date, 540 + (i % 4) * 30, 1180 + (i % 5) * 20, '08:00', '21:20'] as [string, number, number, string, string]) },
    deficits: { days: 21, need: 14, items: [
      { key: 'dietary_magnesium_total', name: 'Магний', unit: 'мг', pct: 52, avg: 218, food: 218, supp: 0, rda: 420, level: 'warn',
        text: 'Магний: в среднем 52% нормы за 21 дн. с полными записями (218 из 420 мг), добавок не отмечено.', advice: 'Если отдельно не принимаешь — может стоить сдать анализ: магний в крови (показывает дефицит не всегда).',
        test: 'магний в крови (показывает дефицит не всегда)', signs: 'судороги и подёргивания мышц, плохой сон, раздражительность', foods: 'гречка, овсянка, орехи, семечки, бобовые, тёмный шоколад', lab: null },
      { key: 'dietary_vitamin_d_total', name: 'Витамин D', unit: 'мкг', pct: 22, avg: 3.3, food: 3.3, supp: 0, rda: 15, level: 'bad',
        text: 'Витамин D: в среднем 22% нормы за 21 дн. с полными записями (3,3 из 15 мкг), добавок не отмечено.', advice: 'Анализ от 14 окт тоже ниже нормы — обсуди с врачом.',
        test: 'витамин D (25-OH)', signs: 'частые простуды, боль в мышцах и костях, медленное восстановление, стрессовые переломы', foods: 'из еды витамина D всегда мало — основной источник солнце; жирная рыба, печень трески, яйца, грибы',
        lab: { date: '2026-10-14', value: 27.5, unit: 'нг/мл', status: 'low' } },
    ] },
    micro: [['Витамин C', 120], ['Витамин D', 22], ['Магний', 74], ['Калий', 81], ['Железо', 160], ['Кальций', 65], ['B12', 210], ['Цинк', 115]].map(([n, p]) => ({ key: String(n), name: String(n), unit: 'мг', rda: 100, avg: Number(p), pct: Number(p), days: 7, avgEst: Number(p) < 100 ? Number(p) + 18 : null, pctEst: Number(p) < 100 ? Number(p) + 18 : null })),
    estimate: { entries7: 84, estimated7: 31, unknown7: ['домашний пирог'] },
  };
}

function demoHealth(): HealthSummary {
  const days = Array.from({ length: 30 }, (_, k) => new Date(Date.UTC(2026, 8, 18) + k * 86400000).toISOString().slice(0, 10));
  const hrv: Pt[] = days.map((d, i) => [d, Math.round(52 + 9 * Math.sin(i / 3) + (i % 5) * 1.5)]);
  const rhr: Pt[] = days.map((d, i) => [d, Math.round(51 + 3 * Math.cos(i / 4))]);
  const start = 1792195200 - 3 * 3600 + 1800; // ~23:40
  const segs: [string, number, number][] = [];
  const pattern: [string, number][] = [['core', 15], ['deep', 45], ['core', 25], ['rem', 20], ['awake', 4], ['core', 40], ['deep', 30], ['core', 30], ['rem', 30], ['core', 50], ['awake', 6], ['core', 35], ['rem', 40], ['core', 42]];
  let t = start;
  for (const [st, m] of pattern) { segs.push([st, t, t + m * 60]); t += m * 60; }
  const nights: HealthNight[] = days.slice(-14).map((d, i) => ({ date: d, totalS: 23000 + (i % 4) * 1800, deepS: 5200, coreS: 12000, remS: 7000, awakeS: 1100, score: 70 + (i % 4) * 5, bedUtc: null, wakeUtc: null }));
  nights[nights.length - 1] = { date: days[29], totalS: 27720, deepS: 5280, coreS: 14100, remS: 8340, awakeS: 1080, score: 86, bedUtc: start, wakeUtc: t };
  return {
    today: days[29], tz: 10800,
    signals: [
      { key: 'resp', name: 'Частота дыхания', unit: '/мин', value: 14.2, date: days[29], norm: [14.3, 13.6, 15], level: 0 },
      { key: 'temp', name: 'Температура запястья', unit: '°C', value: 0.1, date: days[29], norm: [0, -0.5, 0.5], level: 0 },
      { key: 'rhr', name: 'Пульс покоя', unit: 'уд/мин', value: 48, date: days[29], norm: [49, 46, 51], level: 0 },
      { key: 'spo2', name: 'Кислород в крови', unit: '%', value: 97, date: days[29], norm: [97, 95, 99], level: 0 },
    ],
    signalMatrix: ['resp', 'temp', 'rhr', 'spo2', 'hrv'].map((k, j) => ({ key: k, name: ['Дыхание', 'Температура', 'Пульс покоя', 'SpO₂', 'HRV'][j], unit: '', days: days.slice(-14).map((d, i) => [d, 1, i === 6 ? 2 : (i + j) % 7 === 0 ? 1 : 0] as [string, number | null, number]) })),
    night: nights[nights.length - 1],
    hypnogram: { date: days[29], start, end: t, segs },
    wakeups: { count: 3, min: 18 }, regularityMin: 25, nights,
    need: { baseMin: 465, loadMin: 10, repayMin: 20, napMin: 0, illMin: 0, totalMin: 495, debtMin: 258 },
    plan: { wake: '07:30', bed: '23:30', lights: '23:00', weekend: true, caffeineCut: '14:50', date: days[29], accepted: null },
    schedule: { weekday: '07:30', weekend: '08:30', baseMin: 465 }, scheduleSource: 'watch', rhythm: { bed: '23:15', wake: '07:30', nights: 11 },
    suggestion: { min: 30, reasons: ['долг сна за неделю 4 ч 18 мин', 'HRV ниже нормы второе утро подряд'], date: days[29], earlierBed: '23:00', laterWake: '08:00' },
    nightState: { phase: 'before_sleep', now: 1792282000, bedAbs: 1792292000, wakeAbs: 1792321000, minToBed: 166, sleptToday: false, useYesterday: true, debtMin: 258 },
    hrv: { series: hrv, last30: hrv, band: [56, 52, 60], nightly: true }, rhr: { series: rhr, last30: rhr, band: [49, 46, 51], nightly: true },
    stress: { series: hrv.map(([d, v]) => [d, Math.round(20 + (v % 25))] as Pt), today: DEMO_STRESS, norm: 33 },
    vo2: { series: days.slice(-10).map((d, i) => [d, 48 + (i % 3) * 0.2] as Pt), lab: { value: 48.5, date: '2026-05-20', where: 'лаборатория' } },
    body: {
      weighings: [{ date: '2026-10-14', weight: 78.4, fat: 17.8, lean: 64.4 }, { date: '2026-10-10', weight: 78.7, fat: 18.0, lean: 64.5 }], steps7: 11400, steps: Array.from({ length: 30 }, (_, k) => [`2026-10-${String(k + 1).padStart(2, '0')}`, Math.round(8000 + 3500 * Math.sin(k / 2.3) + (k % 4) * 600)] as Pt),
      weight: Array.from({ length: 120 }, (_, k) => [new Date(Date.UTC(2026, 5, 20) + k * 86400000).toISOString().slice(0, 10), 80 - k / 60 + Math.sin(k) * 0.6] as Pt).filter((_, k) => k % 2 === 0),
      fat: Array.from({ length: 60 }, (_, k) => [new Date(Date.UTC(2026, 5, 20) + k * 2 * 86400000).toISOString().slice(0, 10), 18.5 - k / 60 + Math.cos(k) * 0.5] as Pt),
      lean: Array.from({ length: 60 }, (_, k) => [new Date(Date.UTC(2026, 5, 20) + k * 2 * 86400000).toISOString().slice(0, 10), 64.5 + Math.sin(k) * 0.6] as Pt),
    },
    alcohol: { days: days.map((d, i) => [d, [0, 0, 3, 0, 0, 0, 5][i % 7]] as Pt), known: 8, hrvAfterPct: -18, rhrAfter: 4.5, session: [1792950000, 1792953600].map((ts) => ({ ts, units: 1.6, volumeMl: 300 })), weightKg: 78.4 },
    caffeine: { avg: 60, latest: '17:41', lateDays: 2, days: [[days[29], '14:00', 45], [days[28], '14:02', 85]], known: 8 },
    habits: [
      { key: 'alcohol', name: 'Алкоголь', auto: 'Waterllama', today: 0, effect: { with: 5, without: 9, pct: -18 } },
      { key: 'caffeine_late', name: 'Кофеин после 16:00', auto: 'Waterllama', today: 0, effect: { with: 2, without: 6, pct: null } },
      { key: 'late_dinner', name: 'Поздний ужин (после 21:00)', auto: 'YAZIO', today: 1, effect: { with: 6, without: 7, pct: -4 } },
      { key: 'screens', name: 'Экраны в кровати', auto: null, today: null, effect: { with: 0, without: 0, pct: null } },
      { key: 'sauna', name: 'Сауна или баня', auto: null, today: null, effect: { with: 0, without: 0, pct: null } },
    ],
  };
}

async function demoCall(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  await new Promise((r) => setTimeout(r, 120));
  switch (cmd) {
    case 'app_status':
      return {
        version: '1.0.0', platform: 'browser', dataDir: 'C:\\Users\\User\\Kouros', dbOk: true, dbError: null,
        schemaVersion: 1, dbSize: 98304, cipherVersion: '4.6.1 community', hello: 'available',
        lockEnabled: demo.lock, unlocked: demo.unlocked,
      } satisfies Status;
    case 'unlock': demo.unlocked = true; return true;
    case 'set_lock_enabled': demo.lock = Boolean(args.enabled); return true;
    case 'secret_status':
      return Object.fromEntries(
        ['strava_client_id', 'strava_client_secret', 'freddy_api_key', 'anthropic_api_key'].map((n) => [n, !!demo.secrets[n]]),
      );
    case 'secret_set': demo.secrets[String(args.name)] = true; return null;
    case 'secret_delete': delete demo.secrets[String(args.name)]; return null;
    case 'sources_list':
      return [
        { id: 'strava', name: 'Strava', connected: false, lastSync: null, lastError: null },
        { id: 'freddy', name: 'Apple Health (freddy)', connected: false, lastSync: null, lastError: null },
        { id: 'files', name: 'Файлы тренировок', connected: false, lastSync: null, lastError: null },
        { id: 'anthropic', name: 'Claude API', connected: false, lastSync: null, lastError: null },
      ] satisfies Source[];
    case 'backups_list': return demo.backups;
    case 'backup_now': {
      const name = `kouros-${new Date().toISOString().slice(0, 19).replace(/[-:]/g, '').replace('T', '_')}.db`;
      demo.backups.unshift({ name, size: 98304, modified: Date.now() / 1000 });
      return name;
    }
    case 'open_data_dir': return null;
    case 'recovery_key': return '3f9c1a7e5b2d4c8f0a6e9b1d7c3f5a2e8b4d6f0c1e3a5b7d9f2c4e6a8b0d1f3e';
    case 'activities_list': {
      const items = demo.imported ? DEMO_ACTS : [];
      const off = Number(args.offset ?? 0), lim = Number(args.limit ?? 50);
      const q = String(args.query ?? '').toLowerCase();
      const f = items.filter((a) => (!args.sport || a.sport === args.sport) && (!q || (a.name ?? '').toLowerCase().includes(q)));
      return { total: f.length, items: f.slice(off, off + lim) };
    }
    case 'activity_get': return demoDetail(String(args.id));
    case 'activity_update': case 'strength_sets_save': case 'shoes_set': return null;
    case 'ofp_save': demo.ofp[String(args.id)] = args.blocks as OfpBlock[]; return null;
    case 'ofp_last': return { 'x_demo1': { date: '2026-10-20', block: { cid: 'x_demo1', name: 'Пример: колени', rounds: 3, items: [
      { exId: 'Bodyweight_Squat', name: 'Приседания без веса', reps: 15, sec: null, leg: false, load: { kind: 'none' }, skip: false },
      { exId: 'k_bird_dog', name: 'Охотничья собака', reps: 10, sec: null, leg: true, load: { kind: 'none' }, skip: false },
      { exId: 'Plank', name: 'Планка', reps: null, sec: 45, leg: false, load: { kind: 'none' }, skip: false },
    ] } } } satisfies OfpLast;
    case 'device_change_decide': case 'manual_save': return null;
    case 'manual_state': return { hrvOn: true, vitalsOn: false, kind: 'rmssd', today: '2026-10-26', values: { hrv: null, resp: null, spo2: null }, usual: { hrv: 62, resp: null, spo2: null }, history: [] } satisfies ManualState;
    case 'complexes_get': return { complexes: demo.complexes, custom: demo.custom };
    case 'complexes_save': demo.complexes = args.complexes as Complex[]; demo.custom = (args.custom as ExerciseItem[]).map((x) => ({ ...x, en: '', cat: 'strength', eq: 'body only', lvl: '', mech: '', force: '', img: 0, own: true, syn: x.syn ?? [] })); return null;
    case 'complexes_import': return { complexes: 0, exercises: 0 };
    case 'exercises_list': return ['Жим штанги лёжа', 'Приседания со штангой', 'Тяга верхнего блока'];
    case 'exercise_library': return { source: 'демо', version: 'demo', items: [
      { id: 'Pushups', ru: 'Отжимания', en: 'Pushups', syn: ['отжимания от пола'], tip: 'Руки на ширине плеч, тело прямое; опускай грудь к полу и выжимай.', cat: 'strength', eq: 'body only', lvl: 'beginner', mech: 'compound', force: 'push', p: ['chest'], s: ['shoulders', 'triceps'], bw: 0.64, st: false, img: 2 },
      { id: 'Pullups', ru: 'Подтягивания', en: 'Pullups', syn: ['подтягивания на турнике'], tip: 'Хват шире плеч; подтягивайся до подбородка над перекладиной.', cat: 'strength', eq: 'body only', lvl: 'beginner', mech: 'compound', force: 'pull', p: ['lats'], s: ['biceps', 'middle back'], bw: 0.95, st: false, img: 2 },
      { id: 'Barbell_Squat', ru: 'Приседания со штангой', en: 'Barbell Squat', syn: ['присед'], tip: 'Штанга на трапециях; садись до параллели, спина прямая.', cat: 'strength', eq: 'barbell', lvl: 'beginner', mech: 'compound', force: 'push', p: ['quadriceps'], s: ['glutes', 'hamstrings', 'calves'], bw: null, st: false, img: 2 },
      { id: 'Plank', ru: 'Планка', en: 'Plank', syn: [], tip: 'Упор на предплечьях и носках; тело прямое.', cat: 'strength', eq: 'body only', lvl: 'beginner', mech: 'isolation', force: 'static', p: ['abdominals'], s: [], bw: null, st: true, img: 2 },
      { id: 'Bodyweight_Squat', ru: 'Приседания без веса', en: 'Bodyweight Squat', syn: [], tip: 'Стопы на ширине плеч; садись, отводя таз назад, и вставай.', cat: 'strength', eq: 'body only', lvl: 'beginner', mech: 'compound', force: 'push', p: ['quadriceps'], s: ['glutes', 'hamstrings'], bw: 0.8, st: false, img: 2 },
      { id: 'k_bird_dog', ru: 'Охотничья собака', en: 'Bird dog', syn: [], tip: 'На четвереньках вытяни руку и противоположную ногу, задержись и вернись.', cat: 'strength', eq: 'body only', lvl: 'beginner', mech: 'compound', force: 'static', p: ['lower back'], s: ['glutes', 'abdominals'], bw: null, st: false, leg: true, img: 0 },
      { id: 'k_eccentric_heel_drop', ru: 'Медленное опускание пятки (эксцентрика)', en: 'Eccentric heel drop', syn: [], tip: 'На ступеньке поднимись на двух ногах и медленно опустись на одной.', cat: 'strength', eq: 'body only', lvl: 'beginner', mech: 'isolation', force: 'push', p: ['calves'], s: [], bw: 0.9, st: false, leg: true, img: 0 },
      ...demo.custom,
      { id: 'Band_Pull_Apart', ru: 'Разведение резины перед собой', en: 'Band Pull Apart', syn: [], tip: 'Прямые руки перед собой; растягивай резину в стороны.', cat: 'strength', eq: 'bands', lvl: 'beginner', mech: 'isolation', force: 'pull', p: ['shoulders'], s: ['middle back', 'traps'], bw: null, st: false, img: 2 },
    ], runnerOfp: ['Bodyweight_Squat', 'Plank', 'Pushups', 'k_bird_dog', 'k_eccentric_heel_drop'] } satisfies ExerciseLibrary;
    case 'bands_get': return [{ id: 'b1', name: 'Зелёная', color: '#3DDC97', lo: 15, hi: 35, unit: 'kg' }, { id: 'b2', name: 'Красная', color: '#FF6B6B', lo: 5, hi: 15, unit: 'kg' }] satisfies ResBand[];
    case 'bands_set': return null;
    case 'activity_set_shoe': case 'setting_list_set': case 'map_set_enabled': case 'map_cache_clear': return null;
    case 'health_summary': return demoHealth();
    case 'habit_mark': case 'health_setting_set': return null;
    case 'nutrition_summary': return demoNutrition();
    case 'sources_state': return demoSources();
    case 'sources_preview': return { text: 'Garmin и Apple Health меряют HRV по-разному — цифры будут другого уровня. Норма HRV пересчитается заново.', historyFrom: '2024-03-01', appleHistory: false };
    case 'sources_set': case 'sources_map_set': case 'sources_set_all': return null;
    case 'hae_state': return demoHae.state();
    case 'hae_set': Object.assign(demoHae, (args?.patch ?? {}) as object); return demoHae.state();
    case 'hae_import_file': return { samples: 412530, first: '2025-10-01', last: '2026-10-18', metrics: 41, unknown: [] } satisfies HaeImport;
    case 'hae_scan': return 0;
    case 'workouts_state': return { mode: 'both', providers: [], rules: {}, enabledTs: 1792400000, groups: [['run', 'Бег'], ['walk', 'Ходьба и походы'], ['strength', 'Силовые и тренировки'], ['ride', 'Велосипед'], ['other', 'Остальное']].map(([key, name]) => ({ key, name, rule: 'ask' })), available: [{ provider: 'APPLE_HEALTH', name: 'Apple Health' }], counts: { strava: 812, freddy: 56, pending: 1, deleted: 1 } };
    case 'dups_pending': return [{ group: 'g1', date: '2026-10-18', sportGroup: 'run', records: [
      { id: 's1', sport: 'run', name: 'Вечерний бег', start: 1792440000, min: 48, km: 8.1, avgHr: 141, source: 'strava_api', sourceName: 'Strava', detail: 'пульс, темп и высота по секундам, трек GPS', counted: true },
      { id: 'h1', sport: 'run', name: 'Бег · Apple Watch', start: 1792440020, min: 48, km: 8.08, avgHr: 142, source: 'freddy:APPLE_HEALTH', sourceName: 'Apple Health · Apple Watch', detail: 'пульс по ходу (каждые ~15 с), без трека', counted: false }] }];
    case 'strava_gone': return [] satisfies StravaGone[];
    case 'strava_gone_decide': return null;
    case 'workouts_deleted': return [{ id: 'x1', sport: 'walk', name: 'Прогулка', start: 1792300000, min: 35, km: 3.1, sourceName: 'Strava', deletedTs: 1792400000 }];
    case 'workouts_save': case 'dup_decide': case 'workout_choose': case 'workouts_delete': case 'workout_restore': case 'workout_purge': return null;
    case 'body_state': return demoBody();
    case 'checkin_save': case 'pain_mark': case 'pain_delete': case 'return_start': case 'return_stop': return null;
    case 'plan_state': return demoPlan();
    case 'plan_decide': case 'plan_set_day': case 'plan_reset_day': case 'plan_template_save': return null;
    case 'coach_state': return demoCoach.state();
    case 'coach_profile_save': demoCoach.profile = (args?.profile as CoachProfile) ?? null; return null;
    case 'coach_notes_save': demoCoach.notes = String(args?.text ?? ''); return null;
    case 'coach_day': return demoCoach.add(demoCoach.dayReport());
    case 'coach_week': return demoCoach.add(demoCoach.weekReport());
    case 'coach_package': return '[Kouros · Как мой день]\nЗадача: коротко разбери мой день…\n\nСводка Kouros:\n- Готовность утром: 58';
    case 'coach_save_answer': return demoCoach.add({ id: 0, kind: 'claude', ts: Math.floor(Date.now() / 1000), from: null, to: null, title: 'Ответ Claude', body: { text: String(args?.text ?? '') }, parent: (args?.parent as number) ?? null });
    case 'coach_delete': demoCoach.reports = demoCoach.reports.filter((r) => r.id !== args?.id && r.parent !== args?.id); return null;
    case 'meds_summary': return {
      items: [
        { id: 1, kind: 'supplement', name: 'Мультивитамины', form: 'таблетка', dose: 1, schedule: 'daily', components: [{ key: 'dietary_vitamin_d_total', name: 'Витамин D', amount: 125, unit: 'мкг' }, { key: 'dietary_zinc_total', name: 'Цинк', amount: 10, unit: 'мг' }], courseFrom: null, courseTo: null, notes: null, archived: false, today: 1, active: true, days: Array.from({ length: 14 }, (_, k) => [`2026-10-${String(5 + k).padStart(2, '0')}`, k % 3 ? 1 : 0] as [string, number]) },
        { id: 2, kind: 'supplement', name: 'Омега-3', form: 'капсула', dose: 2, schedule: 'daily', components: [{ key: '', name: 'EPA', amount: 400, unit: 'мг' }, { key: '', name: 'DHA', amount: 300, unit: 'мг' }], courseFrom: null, courseTo: null, notes: null, archived: false, today: 0, active: true, days: [] },
      ],
      today: [['dietary_vitamin_d_total', 125]], supp7: { dietary_vitamin_d_total: 89 },
      warnings: [{ key: 'dietary_vitamin_d_total', avg: 92, supp: 89, limit: 100, note: '' }],
      timing: [{ code: 'fat:2', title: 'Жирорастворимые без еды', text: 'Омега-3 принята без еды рядом (±1 ч) — A, D, E, K и омега-3 лучше с едой, где есть жир.', days: 3, today: false, last: '2026-10-17' }],
      notes: [{ group: 'nsaid', groupName: 'Противовоспалительное обезболивающее (НПВП)', med: 'Нурофен', level: 'warn', text: 'Нурофен (НПВП) за последние сутки, а сегодня длительная тренировка — лучше не принимать «на всякий случай».' }],
      unknown: [],
    } satisfies MedsSummary;
    case 'catalog_brands': return { version: '2026-10-20', brands: [
      { key: 'maxler', name: 'Maxler', count: 3, kinds: { vitamin: 1, protein: 1, isotonic: 1, amino: 0, other: 0 } },
      { key: 'now', name: 'NOW', count: 1, kinds: { vitamin: 1, protein: 0, isotonic: 0, amino: 0, other: 0 } },
      { key: 'solgar', name: 'Solgar', count: 1, kinds: { vitamin: 1, protein: 0, isotonic: 0, amino: 0, other: 0 } },
    ] } satisfies CatalogBrands;
    case 'catalog_search': return ([
      { id: 'maxler:vitamen', brand: 'maxler', name: 'VitaMen', serving: '2 tablets', form: 'tablet', kind: 'vitamin', preview: 'Витамин A, Витамин C, Витамин D, Витамин E', count: 30 },
      { id: 'maxler:whey', brand: 'maxler', name: '100% Golden Whey', serving: '30 g', form: 'powder', kind: 'protein', preview: 'Энергия, Белок, Углеводы, Жиры', count: 6 },
      { id: 'maxler:iso', brand: 'maxler', name: 'Isotonic', serving: '1 scoop with 500 ml water', form: 'powder', kind: 'isotonic', preview: 'Энергия, Углеводы, Натрий, Калий', count: 5 },
      { id: 'now:d3', brand: 'now', name: 'Vitamin D-3 5,000 IU', serving: '1 softgel', form: 'softgel', kind: 'vitamin', preview: 'Витамин D', count: 1 },
      { id: 'solgar:mg', brand: 'solgar', name: 'Magnesium Citrate', serving: '2 tablets', form: 'tablet', kind: 'vitamin', preview: 'Магний', count: 1 },
    ] satisfies CatalogItem[]).filter((x) => (!args?.brand || x.brand === args.brand) && (!args?.kind || x.kind === args.kind) && x.name.toLowerCase().includes(String(args?.q ?? '').toLowerCase()));
    case 'catalog_product': return { kind: 'supplement', name: 'Maxler Isotonic', form: 'порция порошка', dose: 1, schedule: 'asneeded',
      components: [{ key: 'dietary_energy_total', name: 'Энергия', amount: 120, unit: 'ккал' }, { key: 'dietary_carbohydrates_total', name: 'Углеводы', amount: 30, unit: 'г' }, { key: 'dietary_sodium_total', name: 'Натрий', amount: 300, unit: 'мг' }],
      extra: { source: String(args?.id ?? ''), brand: 'maxler', serving: '1 scoop', catalogKind: 'isotonic', countFood: true, waterMl: 500 } } satisfies Partial<Med>;
    case 'catalog_online': return [{ id: 'dsld:1', brand: 'Thorne', name: 'Basic Nutrients 2/Day', serving: '2 Capsule(s)', offMarket: 0,
      prep: { kind: 'supplement', name: 'Thorne Basic Nutrients 2/Day', form: 'капсула', dose: 1, schedule: 'daily', components: [{ key: 'dietary_vitamin_d_total', name: 'Витамин D', amount: 25, unit: 'мкг' }], extra: { source: 'dsld:1', countFood: false } } }] satisfies OnlineItem[];
    case 'catalog_label': return { kind: 'supplement', name: 'Из ответа Claude', form: 'таблетка', dose: 1, schedule: 'daily', components: [{ key: 'dietary_vitamin_d_total', name: 'Витамин D', amount: 25, unit: 'мкг' }], extra: { source: 'label', countFood: false } } satisfies Partial<Med>;
    case 'drug_search': return [{ trade: 'Нурофен', inn: 'Ибупрофен', form: 'таблетки, покрытые оболочкой', dose: '200 мг', group: { key: 'nsaid', name: 'Противовоспалительное обезболивающее (НПВП)', note: 'Снимает боль, но не лечит её причину.' }, source: 'registry' }] satisfies DrugHit[];
    case 'drug_online': return { classes: ['M01AE01'], group: { key: 'nsaid', name: 'Противовоспалительное обезболивающее (НПВП)', note: '' } };
    case 'drug_registry_info': return { builtin: 14000, builtinVersion: 'Перечень_(октябрь_2026).xls', builtinSource: 'grls.rosminzdrav.ru', local: 0, localVersion: null, online: true } satisfies DrugRegistry;
    case 'drug_registry_update': return 14200;
    case 'supp_online_set': return null;
    case 'meds_enabled': return true;
    case 'labs_summary': return demoLabs();
    case 'labs_parse': case 'labs_parse_text': return {
      date: '2026-10-14', lab: 'Пример', file: null, source: args?.path ? 'pdf' : 'claude', rows: [
        { name: 'Ферритин', valueText: '41', value: 41, cmp: '', flag: '', unit: 'мкг/л', refText: '30-400', lo: 30, hi: 400, section: 'Железо', comment: '', key: 'ferritin', keyName: 'Ферритин' },
        { name: '25-OH витамин D', valueText: '27.5', value: 27.5, cmp: '', flag: '-', unit: 'нг/мл', refText: '30-100', lo: 30, hi: 100, section: 'Витамины', comment: '', key: 'vitd', keyName: 'Витамин D (25-OH)' },
        { name: 'Креатинкиназа (КФК)', valueText: '310', value: 310, cmp: '', flag: '+', unit: 'Ед/л', refText: '<190', lo: null, hi: 190, section: 'Биохимия', comment: '', key: 'ck', keyName: 'Креатинкиназа (КФК)' },
      ] } satisfies LabPreview;
    case 'labs_save': return 2;
    case 'labs_delete': case 'labs_open': case 'deficit_snooze': return null;
    case 'meds_enabled_set': return null;
    case 'med_save': return 3;
    case 'med_delete': case 'med_take': return null;
    case 'psych_summary': return {
      today: '2026-10-18', trainedToday: 0, tqr: [], stressrec: [],
      hooper: Array.from({ length: 14 }, (_, k) => { const d = new Date(Date.UTC(2026, 9, 5) + k * 86400000).toISOString().slice(0, 10); const a = [2 + (k % 3), 3 + (k % 2), 2 + (k % 4), 2]; const sum = a.reduce((x, y) => x + y, 0); return { date: d, ts: 0, answers: a, score: sum, extra: { feel: Math.round(((28 - sum) / 24) * 100) } }; }),
      brums: [{ date: '2026-10-12', ts: 0, answers: [], score: 4, extra: { tension: 4, depression: 1, anger: 2, vigour: 11, fatigue: 6, confusion: 2 } }],
      unrated: [{ id: 's0', sport: 'walk', name: 'Вечерняя прогулка', startUtc: 1792280000 }],
      due: { hooper: true, tqr: true, brumsDays: 6, stressrecDays: null },
    } satisfies PsychSummary;
    case 'survey_save': return { score: 10, extra: null };
    case 'survey_delete': return null;
    case 'nutrition_day': return {
      date: String(args.date), kcal: 2520, protein: 162, carbs: 300, fat: 69, fiber: 28, sugar: 45, waterMl: 2400, latePct: 27,
      meals: [
        { time: '08:10', ts: 0, label: 'Завтрак', names: ['Овсянка с ягодами', 'Кофе с молоком'], kcal: 520, protein: 22, carbs: 80, fat: 12 },
        { time: '11:30', ts: 0, label: 'Перекус', names: ['Греческий йогурт', 'Яблоко'], kcal: 230, protein: 15, carbs: 30, fat: 5 },
        { time: '14:00', ts: 0, label: 'Обед', names: ['Куриная грудка', 'Гречка', 'Салат из овощей'], kcal: 780, protein: 55, carbs: 85, fat: 22 },
        { time: '17:00', ts: 0, label: 'Перекус', names: ['Творог', 'Банан'], kcal: 300, protein: 25, carbs: 35, fat: 6 },
        { time: '20:00', ts: 0, label: 'Ужин', names: ['Запечённая рыба', 'Картофель', 'Овощи на пару'], kcal: 690, protein: 45, carbs: 70, fat: 24 },
      ],
    } satisfies NutritionDay;
    case 'health_day': return {
      date: String(args.date), tz: 10800, sleep: { totalS: 26000, deepS: 5000, coreS: 14000, remS: 7000, awakeS: 1100 }, hypnogram: demoHealth().hypnogram,
      hrv: 58, rhr: 49, hrRecovery: 31, walkHr: 98, resp: 14.1, breath: 2.4, spo2: 96.5, tempShift: 0.12, steps: 12400, activeKcal: 820,
      kcal: 2650, protein: 160, carbs: 280, fatG: 85, waterMl: 2600, alcohol: 0, caffeine: 85, caffeineLast: '11:05', vo2: 48.4, weight: 78.4, fatPct: 17.8,
      activities: [{ id: 's1', sport: 'run', name: 'Утренний бег', startUtc: 1792130000, durationS: 2940, distanceM: 8200, avgHr: 141, load: 118 }], habits: [['sauna', 1]],
      stress: DEMO_STRESS,
      readiness: {
        date: String(args.date),
        morning: { score: 61, level: 'yellow', frozen: true, penalties: [], components: [
          { key: 'hrv', label: 'HRV', value: 78, baseline: 76, score: 50, weight: 0.3, note: '78 мс при норме 76' },
          { key: 'rhr', label: 'Пульс покоя', value: 44, baseline: 47, score: 62, weight: 0.2, note: '44 уд/мин при норме 47' },
          { key: 'sleep', label: 'Сон', value: 27000, baseline: 28000, score: 78, weight: 0.25, note: '7 ч 30 мин' },
          { key: 'resp', label: 'Дыхание', value: 12.8, baseline: 12.5, score: 50, weight: 0.05, note: '12,8 вдохов/мин' },
          { key: 'form', label: 'Свежесть после нагрузки', value: -4, baseline: null, score: 56, weight: 0.15, note: 'форма -4' },
        ] },
        evening: { score: 55, level: 'yellow', morning: 61, delta: -6, items: [{ text: 'Нагрузка за день — 92', points: -4.8 }, { text: 'Твоё восстановление (TQR) — 12', points: -1 }] },
        psych: [{ kind: 'hooper', score: 13, extra: { feel: 63 }, ts: 1792252800 }, { kind: 'tqr', score: 12, extra: null, ts: 1792282800 }],
        vitals: (DEMO_SUMMARY.day as unknown as { vitals: VitalRow[] }).vitals,
      },
    } satisfies HealthDay;
    case 'map_cache_info': return { enabled: false, tiles: 0, bytes: 0 };
    case 'training_overview': return {
      tolerance: { days: ['2026-10-11', '2026-10-12', '2026-10-13', '2026-10-14', '2026-10-15', '2026-10-16', '2026-10-17'].map((d, i) => ({ date: d, km: [0, 8, 0, 5, 0, 8.2, 0][i], extra: [0, 1.2, 0, 0.3, 0, 1.6, 0][i], walk: [1.5, 2.4, 1.6, 1.8, 2.5, 1.3, 2.2][i] })), avgWeekRun: 30.5, avgWeekWalk: 12.1 },
      history: Array.from({ length: 52 }, (_, k) => ({ week: new Date(Date.UTC(2025, 9, 20) + k * 7 * 86400000).toISOString().slice(0, 10), s: (k < 34 ? { run: [3, 10800, 30000, 120, 300] as WeekAgg } : { run: [3, 11000, 31000, 140, 320] as WeekAgg, walk: [9, 30000, 50000, 400, 210] as WeekAgg, strength: [1, 2700, 0, 0, 40] as WeekAgg, workout: [2, 4000, 0, 0, 50] as WeekAgg }) as Record<string, WeekAgg> })),
      sportsYear: ['run', 'walk', 'strength', 'workout'],
      intensityWeeks: Array.from({ length: 8 }, (_, k) => ({ week: new Date(Date.UTC(2026, 7, 31) + k * 7 * 86400000).toISOString().slice(0, 10), run: [10, 60, 110, 8, 3, 0], all: [300, 500, 160, 10, 3, 0], runs: 3 })),
      shoeQuestion: { id: 's1', sport: 'run', name: 'Утренний бег', startUtc: 1792130000, distanceM: 8200 },
    } satisfies TrainingOverview;
    case 'records_extra': return {
      strength: [{ exercise: 'Жим штанги лёжа', orm: 76, weightKg: 60, reps: 8, startUtc: 1784450000, activityId: 's4', manual: false }],
      manual: [], longestRun: { activityId: 's1', distanceM: 15200, timeS: 5400, startUtc: 1783000000 },
      longestWalk: { activityId: 's0', distanceM: 9800, timeS: 6000, startUtc: 1789000000 },
      longestSession: { activityId: 's2', distanceM: 15200, timeS: 5400, startUtc: 1783000000 }, hidden: [],
    } satisfies RecordsExtra;
    case 'shoes_get': return [
      { id: 'x', name: 'Кроссовки для лёгкого бега', kind: 'train', startDate: '2026-06-01', initialKm: 0, limitKm: 700, walks: false, retired: false, km: 412.4, workouts: 48, main: true,
        spec: { pace: ['daily'], sa: 140, er: 62, traction: 0.48, arch: 'neutral', weightG: 290, dropMm: 9, softness: 22, softUnit: 'AC', coldPct: 25, breath: 3, season: 'all' } },
      { id: 'y', name: 'Лёгкие темповые', kind: 'race', startDate: '2026-08-01', initialKm: 0, limitKm: 400, walks: false, retired: false, km: 96, workouts: 9,
        spec: { pace: ['tempo', 'race'], sa: 118, er: 71, traction: 0.6, arch: 'neutral', weightG: 225, dropMm: 7, coldPct: 12, rocker: true } },
    ] satisfies Shoe[];
    case 'shoe_runs': return [
      { id: 's1', sport: 'run', name: 'Вечерний бег', startUtc: 1792440000, distanceM: 9400 },
      { id: 's2', sport: 'run', name: 'Длительная', startUtc: 1792180000, distanceM: 15200 },
    ] satisfies ShoeRun[];
    case 'shoe_advice': return { need: { kind: 'easy', title: 'лёгкий бег', min: 45 }, shoes: 2, warn: [], conditions: demo.cond,
      picks: [
        { id: 'x', name: 'Кроссовки для лёгкого бега', score: 52, plus: ['подходит для «лёгкий бег»', 'хорошая амортизация (140 SA)'], minus: demo.cond?.temp === '-10..0' ? ['пена на холоде твердеет на 25%'] : [], km: 412.4, limitKm: 700, spec: true },
        { id: 'y', name: 'Лёгкие темповые', score: 9, plus: [], minus: ['стартовую пару лучше беречь для ключевых дней'], km: 96, limitKm: 400, spec: true },
      ] } satisfies ShoeAdvice;
    case 'shoe_conditions_set': demo.cond = { date: '2026-10-17', temp: String(args.temp), surface: String(args.surface) }; return null;
    case 'plan_check_choice': {
      const items = (args.items as PlanItem[]) ?? [];
      const hardOne = items.some((i) => ['long', 'tempo', 'intervals', 'test'].includes(i.kind));
      return hardOne
        ? { ok: false, warnings: ['Вчера уже был тяжёлый день — двум подряд лучше не быть, мышцам и связкам нужно 36–48 часов.'], suggestion: [{ kind: 'easy', min: 40 }], suggestionText: 'Лёгкий бег 40 мин' }
        : { ok: true, warnings: [], suggestion: null, suggestionText: null };
    }
    case 'plan_choose': return { changes: [{ date: '2026-10-20', dateRu: 'вт 20 окт', before: 'Лёгкий бег 60 мин', after: 'Лёгкий бег 45 мин', why: 'неделя ≈700 при безопасных 640 — день короче' }] } satisfies ChoiceResult;
    case 'plan_confirm': case 'open_link': return null;
    case 'activities_stats': {
      const items = demo.imported ? DEMO_ACTS : [];
      return {
        count: items.length, firstUtc: items.length ? items[items.length - 1].startUtc : null,
        lastUtc: items.length ? items[0].startUtc : null, withStreams: items.length,
        bySport: items.length ? [{ sport: 'walk', count: 104, distanceM: 520000, seconds: 300000 }, { sport: 'run', count: 82, distanceM: 690000, seconds: 260000 }] : [],
      };
    }
    case 'import_preview':
      return {
        title: 'export_83112406.zip', kind: 'strava_archive', found: 248, existing: demo.imported ? 248 : 0,
        new: demo.imported ? 0 : 248, mergeable: 0, errors: [], newRows: demo.imported ? [] : DEMO_ACTS.slice(0, 5),
        firstUtc: 1768844490, lastUtc: 1792179287,
      };
    case 'import_commit': demo.imported = true; return { added: 248, merged: 0, skipped: 0, errors: 0 };
    case 'import_cancel': return null;
    case 'import_history':
      return demo.imported ? [{ at: '2026-10-17 01:50:00', title: 'export_83112406.zip', kind: 'strava_archive', found: 248, added: 248, merged: 0, skipped: 0, errors: 0 }] : [];
    case 'export_activities': return { folder: 'C:\\Users\\User\\Kouros\\exports\\kouros-csv-2026-10-17_0150', files: 1, skipped: 0 };
    case 'strava_status':
    case 'freddy_status':
      return { keys: true, connected: false, athlete: null, lastOk: null, lastError: null, extra: null, busy: false };
    case 'strava_connect': return 'Алекс';
    case 'strava_sync': return { added: 0, merged: 0, streams: 0, streamsLeft: 0, rateLimited: false, needsChoice: !demo.imported && !args.full };
    case 'freddy_sync': return { rows: 0, pages: 1, lastDate: null };
    case 'health_overview': return { stats: { rows: 5400, metrics: 42, firstDate: '2025-01-01', lastDate: '2026-10-17' }, latest: [] };
    case 'profile_get':
      return { name: 'Алекс', filled: true, hrMax: 184, hrRest: null, hrRestAuto: 55, zones: [110, 129, 147, 160, 171, 184], heightCm: 178, birthYear: 1990, male: true, proteinGPerKg: 1.8, zonesDate: null, thresholdPaceS: 330, weekCap: 1.3, paceZones: [573, 457, 383, 354, 331, 312] };
    case 'profile_set': return null;
    case 'engine_summary': return DEMO_SUMMARY;
    default: throw new Error(`Нет команды ${cmd}`);
  }
}

const demoHae = {
  enabled: false, port: 8765, folder: '', newToken: false,
  state(): HaeState {
    const url = 'http://192.168.1.20:8765';
    return { enabled: this.enabled, port: this.port, token: 'k7pm2xq9ra4fbn3t', folder: this.folder, ip: '192.168.1.20', url: `${url}/hae?key=k7pm2xq9ra4fbn3t`, checkUrl: `${url}/check?key=k7pm2xq9ra4fbn3t`,
      running: this.enabled, error: null, lastPush: this.enabled ? Math.floor(Date.now() / 1000) - 600 : null, lastCheck: null, progress: null,
      lastImport: null, waiting: 0, data: this.enabled ? { first: '2026-10-01', last: '2026-10-22', rows: 48210 } : { first: null, last: null, rows: 0 },
      coverage: this.enabled ? [['Сон по фазам', 1, '2026-10-22'], ['HRV', 1, '2026-10-22'], ['Пульс', 1, '2026-10-22'], ['Пульс покоя', 1, '2026-10-22'], ['Дыхание во сне', 1, null], ['Кислород', 0, '2026-10-22'], ['Температура запястья', 0, '2026-10-22'], ['Шаги', 1, '2026-10-22'], ['Активные калории', 1, '2026-10-22'], ['VO₂max', 0, '2026-10-20'], ['Вес', 0, null], ['Еда', 0, '2026-10-22'], ['Вода', 0, '2026-10-22'], ['Кофеин', 0, '2026-10-21'], ['Беговая динамика', 0, '2026-10-21'], ['Ходьба', 0, '2026-10-21']].map(([name, imp, last], i) => ({ key: `m${i}`, name: name as string, important: !!imp, last: last as string | null })) : [],
      groups: [['sleep', 'Сон'], ['hrv', 'HRV'], ['rhr', 'Пульс покоя'], ['vitals', 'Дыхание, SpO₂ и температура'], ['activity', 'Шаги, калории и VO₂max'], ['body', 'Вес и состав тела'], ['pulse', 'Пульс днём и стресс'], ['other', 'Питание, вода и остальное']].map(([key, name]) => ({ key, name, current: 'APPLE_HEALTH', from: '' })) };
  },
};

function demoSources(): SourcesState {
  const g = (key: string, name: string, open = true): SourceGroup => ({
    key, name, open, current: 'APPLE_HEALTH', currentName: 'Apple Health', segs: [{ provider: 'APPLE_HEALTH', name: 'Apple Health', from: '' }], cut: null,
    options: [{ provider: 'APPLE_HEALTH', name: 'Apple Health', matched: 1, of: 1 }, ...(open ? [{ provider: 'GARMIN', name: 'Garmin', matched: key === 'body' ? 0 : 1, of: 1 }] : [])],
    writers: key === 'hrv' ? [{ name: 'Apple Watch', count: 812 }] : [], fields: [],
  });
  return {
    groups: [g('sleep', 'Сон'), g('hrv', 'HRV'), g('rhr', 'Пульс покоя'), g('vitals', 'Дыхание, SpO₂ и температура'), g('activity', 'Шаги, калории и VO₂max'), g('body', 'Вес и состав тела'), g('pulse', 'Пульс днём и стресс', false)],
    known: [{ provider: 'APPLE_HEALTH', name: 'Apple Health', connected: true }, { provider: 'GARMIN', name: 'Garmin', connected: true }, { provider: 'SUUNTO', name: 'Suunto', connected: false }, { provider: 'GOOGLE_HEALTH', name: 'Fitbit (Google Health)', connected: false }],
    providers: [{ provider: 'GARMIN', name: 'Garmin', metrics: 24, first: '2024-03-01', last: '2026-10-18' }], today: '2026-10-19',
  };
}

function demoBody(): BodyState {
  return {
    areas: [['foot', 'Стопа'], ['ankles', 'Ахилл и голеностоп'], ['shin', 'Голень'], ['calves', 'Икры'], ['knees', 'Колено'], ['quads', 'Передняя поверхность бедра'], ['hams', 'Задняя поверхность бедра'], ['hips', 'Тазобедренный сустав'], ['glutes', 'Ягодицы'], ['back', 'Спина'], ['core', 'Живот и кор'], ['chest', 'Грудь'], ['shoulders', 'Плечо'], ['arms', 'Рука'], ['neck', 'Шея'], ['other', 'Другое']].map(([key, name]) => ({ key, name })),
    pains: [{ area: 'knees', side: 'R', name: 'Колено (правая сторона)', level: 4, date: '2026-10-19', active: true, trend: 'better', days14: 5, series: [['2026-10-13', 6], ['2026-10-15', 5], ['2026-10-17', 4], ['2026-10-19', 4]], entries: [{ id: 1, date: '2026-10-19', level: 4, context: 'morning', note: null }],
      advice: { phase: 'sub', phaseText: 'вторая неделя — заживление', days: 7, remove: ['Убери интервалы, темп, длительную и прыжки; бег — только лёгкий и короткий, если боль не растёт во время и на следующее утро.', 'Спуски и глубокие приседания с весом отложи — они сильнее всего нагружают колено.'],
        complexes: [{ id: 'x_demo1', name: 'Пример: колени' }], complexText: 'Из твоих комплексов подойдут — без боли во время и на следующее утро; если болит сильнее, сократи круги.',
        temp: 'Если отёка нет — тепло на 10–15 минут перед нагрузкой и мягкая подвижность; холод — после, если место ноет. Сауна — по самочувствию, без отёка.',
        ointment: 'Мази и гели: разогревающие — перед нагрузкой, если нет отёка; с противовоспалительным (НПВС) — строго по инструкции и коротко, не чтобы тренироваться через боль; обсуди с врачом, особенно если пьёшь другие лекарства.',
        doctor: [], redFlags: 'К врачу сразу, если есть отёк, боль ночью или в покое, онемение или покалывание, не можешь опереться на ногу, или боль появилась после удара или падения.' } }],
    pending: [{ id: 's1', sport: 'run', name: 'Вечерний бег', start: 1792440000, min: 52, km: 9.4, rpe: null, shoeId: null, askShoe: true }],
    return: { active: false, steps: ['Бодрая ходьба 30 минут', '5 × (1 мин бег / 2 мин ходьба)', '6 × (2 мин бег / 1 мин ходьба)', '5 × (4 мин бег / 1 мин ходьба)', 'Лёгкий бег 20 минут без остановок', 'Лёгкий бег 30 минут'], stageText: '' },
    tools: { have: 4, best: [{ name: 'Роллер', text: '10 минут: икры, квадрицепсы — по 1–2 минуты на группу, медленно.' }, { name: 'Компрессионная одежда', text: 'Надень на 2–3 часа после тренировки.' }, { name: 'Растяжка', text: '10–15 минут: икры, задняя поверхность бедра.' }], more: [], not: [{ name: 'Перкуссионный массажёр', text: 'не по больному месту (ахилл, 5/10)' }] },
    goal: { goal: { title: 'Весенний полумарафон', distanceKm: 21.1, target: '1:45:00', date: '2027-04-25' }, distanceM: 21100, targetS: 6300, daysLeft: 188, predS: 6520, predText: '1:48:40', fromDistanceM: 10000, fromTimeS: 2950, fromDate: '2026-10-11', gapS: 220, trend: [['2026-07-26', 6700], ['2026-08-23', 6640], ['2026-09-20', 6580], ['2026-10-19', 6520]], ratePerWeek: -15, verdict: 'При нынешнем темпе прогресса (0:15 в неделю) цель достижима к старту.', tests: [{ date: '2026-09-24', time: 1380, timeText: '23:00', activity: 'x' }], nextTest: '2026-10-29' },
  };
}

function demoPlan(): PlanState {
  const KIND_T: Record<string, string> = { rest: 'Отдых', walk: 'Долгая прогулка', mobility: 'Растяжка и мобилити', easy: 'Лёгкий бег', ofp: 'Укрепляющие упражнения', bike: 'Велосипед, спокойно', long: 'Длительный бег', gym: 'Силовая в зале', tempo: 'Темповый бег', intervals: 'Интервалы' };
  const tpl: PlanItem[][] = [[], [{ kind: 'easy', min: 60 }], [], [{ kind: 'tempo', min: 60 }], [{ kind: 'gym', min: 60 }], [{ kind: 'ofp', min: 30 }], [{ kind: 'long', min: 90 }]];
  const today = '2026-10-19';
  const wd = ['пн', 'вт', 'ср', 'чт', 'пт', 'сб', 'вс'];
  const days: PlanDay[] = Array.from({ length: 28 }, (_, k) => {
    const d = new Date(Date.UTC(2026, 9, 13 + k));
    const date = d.toISOString().slice(0, 10);
    const w = (d.getUTCDay() + 6) % 7;
    const items = tpl[w];
    const text = items.length ? items.map((i) => `${KIND_T[i.kind]} ${i.min} мин`).join(' + ') : 'Отдых';
    return { date, dateRu: `${wd[w]} ${d.getUTCDate()} окт`, wd: wd[w], items, text, source: 'template', note: null, hard: items.some((i) => ['long', 'tempo', 'intervals'].includes(i.kind)), load: items.reduce((a, i) => a + i.min * 1.2, 0),
      status: !items.length ? 'rest' : date < today ? (k % 3 ? 'done' : 'missed') : 'planned', doneActs: [], block: 1, week: 2, deload: false,
      detail: items.map((i) => ({ kind: i.kind, title: KIND_T[i.kind], min: i.min, warmup: '15 минут в Z1–Z2 и беговые упражнения.', main: '3 × 8 мин в Z3–Z4: пульс 147–160, темп 5:31–5:54 /км; между отрезками 2 мин трусцы.', cooldown: '10 минут трусцы в Z1 и растяжка.', drills: i.kind === 'tempo' ? 'Беговые упражнения по 2×20 м.' : null, notes: [] })) };
  });
  const prop: PlanChange = { id: 1, ts: 1792400000, date: '2026-10-22', dateRu: 'чт 22 окт', before: [{ kind: 'tempo', min: 60 }], after: [{ kind: 'easy', min: 45 }], beforeText: 'Темповый бег 60 мин', afterText: 'Лёгкий бег 45 мин', reason: 'совет Claude: темп → лёгкий бег 45 мин', origin: 'claude', state: 'proposed', group: 1 };
  return { today, days, template: tpl, templateAuto: true, phase: { name: 'Поддержание формы', block: 1, week: 2, deload: false }, week: { plan: 520, cap: 640, usual: 490 }, changes: [prop], proposals: [prop], applyMode: 'ask', todayText: 'Отдых',
    kinds: Object.entries(KIND_T).map(([key, title]) => ({ key, title, hard: ['long', 'tempo', 'intervals'].includes(key) })) };
}

const demoCoach = {
  profile: null as CoachProfile | null,
  notes: '',
  reports: [] as CoachReport[],
  next: 1,
  add(r: CoachReport) { r.id = this.next++; this.reports.unshift(r); return r; },
  state(): CoachState {
    const lastWeek = this.reports.find((r) => r.kind === 'week');
    const nextTs = lastWeek ? lastWeek.ts + 7 * 86400 : null;
    return { profile: this.profile, notes: this.notes, week: { from: '2026-10-12', to: '2026-10-18', ready: !nextTs || Date.now() / 1000 >= nextTs, nextTs }, reports: this.reports, rules: 'Ты — тренер в приложении Kouros…' };
  },
  dayReport(): CoachReport {
    return { id: 0, kind: 'day', ts: Math.floor(Date.now() / 1000), from: '2026-10-18', to: '2026-10-18', title: 'Как мой день · 15:40', parent: null, body: {
      phase: 'day', headline: 'Готовность 58 · Аэробная тренировка. Обрати внимание: вода, стресс.',
      sections: [
        { key: 'sleep', title: 'Сон', level: 'ok', text: 'Сон 7 ч 52 мин — достаточно.', why: null, todo: null },
        { key: 'ready', title: 'Готовность', level: 'ok', text: 'Готовность 58 — обычный день.', why: null, todo: null },
        { key: 'plan', title: 'Тренировка', level: 'info', text: 'Аэробная тренировка. 40–60 минут в 2-й зоне.', why: null, todo: null },
        { key: 'stress', title: 'Стресс', level: 'warn', text: 'Стресс днём 48 — выше твоего обычного (34).', why: 'Высокий стресс держит пульс выше, а HRV ниже — ночью восстановление будет хуже, даже если тренировка лёгкая.', todo: '10–15 минут спокойной прогулки без телефона или 5 минут медленного дыхания.' },
        { key: 'food', title: 'Еда', level: 'ok', text: 'Питание по плану: 1450 ккал из 2700.', why: null, todo: null },
        { key: 'water', title: 'Вода', level: 'warn', text: 'Выпито 0,9 л из 3,2 — мало для этого часа.', why: 'Даже небольшое обезвоживание поднимает пульс и ощущение усилия.', todo: 'Выпей 0,5 л в ближайший час.' },
        { key: 'meds', title: 'Витамины и лекарства', level: 'info', text: 'Ещё не отмечено сегодня: Омега-3.', why: null, todo: 'Если уже принял — отметь.' },
      ] } };
  },
  weekReport(): CoachReport {
    return { id: 0, kind: 'week', ts: Math.floor(Date.now() / 1000), from: '2026-10-12', to: '2026-10-18', title: 'Анализ недели · 2026-10-12 — 2026-10-18', parent: null, body: {
      headline: 'Главное на следующую неделю: Сон в среднем 7 ч 05 мин при потребности 7 ч 50 мин', from: '2026-10-12', to: '2026-10-18', days: 7,
      lines: [{ k: 'Нагрузка на сердце', v: '410 в неделю', note: 'обычно 450' }, { k: 'Мышцы и связки', v: '380 в неделю', note: 'обычно 360' }, { k: 'Тренировки', v: 'бег 3, силовые 1, прогулки 5', note: '' }, { k: 'Сон', v: '7 ч 05 мин', note: 'нужно 7 ч 50 мин' }, { k: 'Белок', v: '128 г', note: 'цель 150' }],
      recs: [{ code: 'sleep_short', text: 'Сон в среднем 7 ч 05 мин при потребности 7 ч 50 мин — это главный резерв восстановления. Сдвинь отбой на 30–45 минут раньше.' }, { code: 'protein_low', text: 'Белок 128 г при цели 150 — добавь белковый продукт в 1–2 приёма.' }],
      follow: [], notes: [] } };
  },
};

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) return (await demoCall(cmd, args)) as T;
  return invoke<T>(cmd, args);
}

// ---------- data sources (0.14) ----------
export type SourceOption = { provider: string; name: string; matched: number; of: number };
export type SourceField = { canon: string; label: string; metric: string | null; manual: boolean; unit: string | null; last: string | null; count: number | null; candidates: { metric: string; unit: string; count: number }[] };
export type SourceGroup = {
  key: string; name: string; open: boolean; current: string; currentName: string;
  segs: { provider: string; name: string; from: string }[]; cut: string | null;
  options: SourceOption[]; writers: { name: string; count: number }[]; fields: SourceField[];
};
export type SourcesState = {
  groups: SourceGroup[]; known: { provider: string; name: string; connected: boolean }[];
  providers: { provider: string; name: string; metrics: number; first: string | null; last: string | null }[]; today: string;
};
export type SourcePreview = { text: string; historyFrom: string | null; appleHistory: boolean; sameData?: boolean };

// ---------- Health Auto Export (1.3) ----------
export type HaeImport = { samples: number; first: string | null; last: string | null; metrics: number; unknown: string[] };
export type HaeState = {
  enabled: boolean; port: number; token: string; folder: string; ip: string | null; url: string | null; checkUrl: string | null;
  running: boolean; error: string | null; lastPush: number | null; lastCheck: number | null;
  progress: { label: string; samples: number; first: string | null; last: string | null } | null;
  lastImport: { at: number; how: string; stats: HaeImport } | null; waiting: number;
  data: { first: string | null; last: string | null; rows: number };
  coverage: { key: string; name: string; important: boolean; last: string | null }[];
  groups: { key: string; name: string; current: string; from: string }[];
};

// ---------- workouts deleted in Strava (1.3.2) ----------
export type StravaGone = { id: string; name: string | null; start: number; sport: string };

// ---------- workout sources (0.17) ----------
export type WorkoutRecord = { id: string; sport: string; name: string | null; start: number; min: number; km: number | null; avgHr: number | null; source: string; sourceName: string; detail: string; counted: boolean };
export type DupGroup = { group: string; date: string; sportGroup: string; records: WorkoutRecord[] };
export type WorkoutSources = {
  mode: 'strava' | 'freddy' | 'both'; providers: string[] | null; rules: Record<string, string> | null; enabledTs: number | null;
  groups: { key: string; name: string; rule: string }[]; available: { provider: string; name: string }[];
  counts: { strava: number; freddy: number; pending: number; deleted: number };
};
export type DeletedWorkout = { id: string; sport: string; name: string | null; start: number; min: number; km: number | null; sourceName: string; deletedTs: number | null };
export type WorkoutAlt = { id: string; sourceName: string; detail: string; min: number; km: number | null; avgHr: number | null; counted: boolean };

// ---------- body: check-in, pain, return to run, tools, goal (0.16) ----------
export type PendingCheckin = { id: string; sport: string; name: string | null; start: number; min: number; km: number | null; rpe: number | null; shoeId?: string | null; askShoe?: boolean };
export type ShoeRun = { id: string; sport: string; name: string | null; startUtc: number; distanceM: number | null };
export type PainEntry = { id: number; date: string; level: number; context: string; note: string | null };
export type PainAdvice = {
  phase: 'acute' | 'sub' | 'long'; phaseText: string; days: number; remove: string[]; complexes: { id: string; name: string }[]; complexText: string | null;
  temp: string; ointment: string; doctor: string[]; redFlags: string;
};
export type PainArea = {
  /** 1.4: what to do with an active pain. */
  advice?: PainAdvice | null;
  area: string; side: string; name: string; level: number; date: string; active: boolean; trend: 'better' | 'worse' | 'same' | 'new';
  days14: number; series: [string, number][]; entries: PainEntry[];
};
export type ReturnState = {
  active: boolean; area?: string; areaName?: string; stage?: number; started?: string; finished?: string; steps: string[]; stageText: string;
  history?: { date: string; stage: number; why: string }[]; session?: { date: string; stage: number; during: number };
};
export type ToolTip = { name: string; text: string };
export type ToolsState = { have: number; best: ToolTip[]; more: ToolTip[]; not: ToolTip[] };
export type GoalRace = { title?: string; distanceKm?: number; target?: string; date?: string };
export type GoalState = {
  goal: GoalRace | null; distanceM: number | null; targetS: number | null; daysLeft: number | null;
  predS: number | null; predText: string | null; fromDistanceM: number | null; fromTimeS: number | null; fromDate: string | null;
  gapS: number | null; trend: [string, number][]; ratePerWeek: number | null; verdict: string;
  tests: { date: string; time: number | null; timeText: string | null; activity: string | null }[]; nextTest: string | null;
};
export type BodyState = { areas: { key: string; name: string }[]; pains: PainArea[]; pending: PendingCheckin[]; return: ReturnState; tools: ToolsState; goal: GoalState };
export type CheckinData = { shoe?: string; rpe?: number | null; feel?: number | null; pain?: { area: string; side: string; level: number }[]; note?: string; skip?: boolean };

// ---------- plan (0.15) ----------
export type PlanItem = { kind: string; min: number };
export type PlanDetail = { kind: string; title: string; min: number; warmup: string; main: string; cooldown: string; drills: string | null; notes: string[] };
export type PlanDay = {
  date: string; dateRu: string; wd: string; items: PlanItem[]; text: string; source: string | null; note: string | null;
  hard: boolean; load: number; status: 'done' | 'partial' | 'missed' | 'planned' | 'rest'; confirmed?: boolean;
  doneActs: { id: string; sport: string; min: number; load: number; unplanned?: boolean }[]; block: number; week: number; deload: boolean; detail: PlanDetail[];
};
export type PlanChange = {
  id: number; ts: number; date: string; dateRu: string; before: PlanItem[]; after: PlanItem[]; beforeText: string; afterText: string;
  reason: string; origin: 'kouros' | 'claude' | 'manual'; state: 'proposed' | 'accepted' | 'kept'; group: number | null;
};
export type PlanState = {
  today: string; days: PlanDay[]; template: PlanItem[][]; templateAuto: boolean;
  phase: { name: string; block: number; week: number; deload: boolean };
  week: { plan: number; cap: number | null; usual: number | null };
  changes: PlanChange[]; proposals: PlanChange[]; applyMode: 'auto' | 'ask'; todayText: string;
  kinds: { key: string; title: string; hard: boolean }[];
};

// ---------- coach (0.13) ----------
export type CoachProfile = {
  goals?: string[]; mode?: string; week?: Record<string, number>; days?: string[]; time?: string; when?: string;
  where?: string[]; injuries?: string[]; tools?: string[]; other?: string; style?: 'short' | 'long'; done?: boolean; planChanges?: 'auto' | 'ask'; goalRace?: GoalRace;
};
export type CoachSection = { key: string; title: string; level: 'ok' | 'warn' | 'bad' | 'info'; text: string; why: string | null; todo: string | null };
export type CoachLine = { k: string; v: string; note: string };
export type CoachBody = {
  phase?: 'morning' | 'day' | 'evening' | 'night'; headline?: string; sections?: CoachSection[];
  from?: string; to?: string; days?: number; lines?: CoachLine[]; recs?: { code: string; text: string }[];
  follow?: { text: string; done: boolean }[]; notes?: string[]; text?: string; plan?: number;
};
export type CoachReport = { id: number; kind: 'day' | 'week' | 'claude'; ts: number; from: string | null; to: string | null; title: string; body: CoachBody; parent: number | null };
export type CoachState = {
  profile: CoachProfile | null; notes: string;
  week: { from: string; to: string; ready: boolean; nextTs: number | null };
  reports: CoachReport[]; rules: string;
};

// ---------- stage 2 types ----------

export type ActivityRow = {
  id: string;
  stravaId: number | null;
  sport: string;
  sportLabel: string | null;
  name: string | null;
  startUtc: number;
  elapsedS: number | null;
  movingS: number | null;
  distanceM: number | null;
  elevGainM: number | null;
  avgHr: number | null;
  maxHr: number | null;
  calories: number | null;
  relativeEffort: number | null;
  totalSets: number | null;
  totalReps: number | null;
  device: string | null;
  source: string;
  hasStreams: boolean;
  hasGps: boolean;
  filePath: string | null;
  rpe: number | null;
  load: number | null;
};
export type ActivityPage = { total: number; items: ActivityRow[] };
export type SportCount = { sport: string; count: number; distanceM: number; seconds: number };
export type ActivityStats = {
  count: number; firstUtc: number | null; lastUtc: number | null; withStreams: number; bySport: SportCount[];
};
export type PreviewRow = {
  startUtc: number; sport: string; sportLabel: string | null; name: string | null;
  distanceM: number | null; elapsedS: number | null; avgHr: number | null;
};
export type Preview = {
  title: string; kind: 'strava_archive' | 'files'; found: number; existing: number; new: number; mergeable: number;
  errors: { name: string; why: string }[]; newRows: PreviewRow[]; firstUtc: number | null; lastUtc: number | null;
};
export type ImportResult = { added: number; merged: number; skipped: number; errors: number };
export type ImportLog = {
  at: string; title: string; kind: string; found: number; added: number; merged: number; skipped: number; errors: number;
};
export type SourceStatus = {
  keys: boolean; connected: boolean; athlete: string | null; lastOk: string | null; lastError: string | null;
  extra: string | null; busy: boolean;
};
export type StravaSync = {
  added: number; merged: number; streams: number; streamsLeft: number; rateLimited: boolean; needsChoice: boolean;
};
export type FreddySync = { rows: number; pages: number; lastDate: string | null };
export type MetricLatest = { metric: string; date: string; num: number | null; unit: string | null; device: string; count: number };
export type HealthOverview = {
  stats: { rows: number; metrics: number; firstDate: string | null; lastDate: string | null };
  latest: MetricLatest[];
};
export type ExportResult = { folder: string; files: number; skipped: number };
export type ListFilter = { sport?: string | null; days?: number | null; query?: string; sort?: 'date' | 'distance' | 'duration' | 'load' };
export type SetRow = {
  idx?: number; startUtc: number | null; durationS: number | null; reps: number | null; weightKg: number | null;
  kind: 'active' | 'rest' | 'skip'; exercise: string | null; category?: number | null;
  /** 0.22: exercise from the library, bands used, effective load of one repetition (kg). */
  exId?: string | null; band?: { ids: string[]; mode: 'add' | 'assist' } | null; effKg?: number | null;
};
export type ExerciseItem = {
  id: string; ru: string; en: string; syn: string[]; tip: string; cat: string | null; eq: string | null; lvl: string | null;
  mech: string | null; force: string | null; p: string[]; s: string[]; bw: number | null; st: boolean; img: number;
  /** 1.4: done on each leg / side (reps are per leg); `own` = your own exercise. */
  leg?: boolean; own?: boolean;
};
export type ExerciseLibrary = { source: string; version: string; items: ExerciseItem[]; runnerOfp?: string[] };
/** 1.4: «ОФП и комплексы» — a block is a complex or one exercise; it is saved as ordinary sets. */
export type ManualState = {
  hrvOn: boolean; vitalsOn: boolean; kind: 'rmssd' | 'sdnn'; today: string;
  values: { hrv: number | null; resp: number | null; spo2: number | null }; usual: { hrv: number | null; resp: number | null; spo2: number | null };
  history: { date: string; hrv: number | null; resp: number | null; spo2: number | null }[];
};
export type DeviceChange = { group: string; name: string; from: string; to: string; since: string };
export type OfpLoad = { kind: 'none' | 'band' | 'kg'; kg?: number | null; band?: { ids: string[]; mode: 'add' | 'assist' } | null };
export type OfpItem = { exId: string | null; name: string | null; reps: number | null; sec: number | null; leg: boolean; load: OfpLoad; skip: boolean };
export type OfpBlock = { cid: string | null; name: string; rounds: number; items: OfpItem[] };
export type OfpLast = Record<string, { date: string; block: OfpBlock }>;
export type Complex = { id: string; name: string; note: string; areas: string[]; items: { exId: string; leg?: boolean | null }[] };
export type CustomExercise = { id: string; ru: string; tip: string; p: string[]; s: string[]; st: boolean; leg: boolean; bw: number | null; syn?: string[] };
export type ResBand = { id: string; name: string; color: string; lo: number; hi: number; unit: 'kg' | 'lb' };
export type KmSplit = { km: number; timeS: number; hr: number | null; full: boolean; distM?: number };
export type ActivityDetail = {
  activity: ActivityRow; description: string | null; notes: string | null; weightLiftedKg: number | null; gear: string | null;
  load: number | null; loadMethod: string | null; zoneS: number[] | null; shoeId?: string | null;
  charts: { t: number[]; hr: (number | null)[]; dist: (number | null)[]; speed: (number | null)[]; alt: (number | null)[]; cad: (number | null)[]; lat: (number | null)[]; lng: (number | null)[] } | null;
  splits: KmSplit[]; sets: SetRow[]; bodyKg?: number | null;
  sourceName?: string; detailText?: string; hidden?: number; dupState?: string | null; alternatives?: WorkoutAlt[];
  ofp?: OfpBlock[];
  muscle?: { total: number; free?: number | null; up: number | null; down: number | null; rec: { hours: number; heartH: number; muscleH: number } | null; bad?: boolean | null; groups: { key: string; name: string; value: number; pct: number }[] } | null;
};
export type Shoe = {
  id: string; name: string; kind: 'train' | 'race' | 'trail' | 'walk'; startDate: string; initialKm: number; limitKm: number;
  walks: boolean; retired: boolean; main?: boolean; km?: number; workouts?: number;
  /** 0.23: characteristics (filled by hand or pasted from a review table) and a review link. */
  spec?: ShoeSpec; url?: string;
};
export type ShoePace = 'daily' | 'recovery' | 'tempo' | 'race' | 'trail';
export type ShoeSpec = {
  pace?: ShoePace[]; sa?: number; er?: number; traction?: number; arch?: 'neutral' | 'stability'; weightG?: number; dropMm?: number;
  softness?: number; softUnit?: 'AC' | 'HA'; coldPct?: number; outsoleMm?: number; toebox?: number; heelPad?: number; breath?: number;
  season?: 'all' | 'summer' | 'winter'; widthMm?: number; toeboxMm?: number; heelMm?: number; foreMm?: number; rocker?: boolean;
  /** 0.23.1: waterproof membrane (Gore-Tex and similar). */
  membrane?: boolean;
  extra?: Record<string, string>;
};
export type ShoePick = { id: string; name: string; score: number; plus: string[]; minus: string[]; km: number; limitKm: number; spec: boolean };
export type ShoeAdvice = {
  need: { kind: string; title: string; min: number } | null; shoes: number; warn?: string[];
  conditions: { date: string; temp: string; surface: string } | null; picks: ShoePick[];
};
export type ChoiceCheck = { ok: boolean; warnings: string[]; suggestion: PlanItem[] | null; suggestionText: string | null };
export type ChoiceResult = { changes: { date: string; dateRu: string; before: string; after: string; why: string }[] };
export type Pt = [string, number];
export type Band = [number, number, number] | null;
export type HealthNight = { date: string; totalS: number; deepS: number; coreS: number; remS: number; awakeS: number; score: number; bedUtc: number | null; wakeUtc: number | null };
export type HealthSignal = { key: string; name: string; unit: string; value: number | null; date: string; norm: Band; level: number };
export type NutTargets = { kcal: number; protein: number; carbs: number; fat: number; fiber: number; waterMl: number };
export type NutDay = {
  date: string; kcal: number | null; protein: number | null; carbs: number | null; fat: number | null; fiber: number | null; sugar: number | null;
  satFat: number | null; waterMl: number | null; spent: number | null; status: 0 | 1 | 2; target: NutTargets;
};
export type NutritionSummary = {
  today: string; tz: number; weight: number;
  targets: NutTargets & { bmr: number; neat: number; train: number; goalKcal: number; trainMin: number };
  eaten: { kcal: number | null; protein: number | null; carbs: number | null; fat: number | null; fiber: number | null; fiberEst?: number; waterMl: number | null };
  goal: { goal: 'maintain' | 'lose' | 'gain'; rate: number; targetWeight: number | null; etaWeeks: number | null; weekly: number; proteinPerKg: number; proteinManual: number | null; monthly: [string, number | null][] };
  days: NutDay[];
  quality: { fullDays: number; fiber: number | null; fiberWithEst?: number | null; sugar: number | null; sugarPct: number | null; protein: number | null; proteinPerKg: number | null; fatPct: number | null; satFat: number | null; vegDays: number; top: [string, number][] };
  window: { first: string | null; last: string | null; afterMidnight: number; days: number; hours: number | null; byDay: [string, number, number, string, string][]; parts: [number, number, number] };
  micro: { key: string; name: string; unit: string; rda: number; avg: number | null; pct: number | null; days: number; avgEst?: number | null; pctEst?: number | null; covered?: number; noData?: boolean; dropped?: number; supp7?: number | null; fromSupp?: number | null; pctAll?: number | null }[];
  norms?: { country: string; list: { code: string; name: string; source: string }[] };
  estimate?: { entries7: number; estimated7: number; unknown7: string[] };
  deficits?: Deficits;
};
export type DeficitItem = { key: string; name: string; unit: string; pct: number; avg: number; food: number; supp: number; rda: number; level: 'bad' | 'warn' | 'info';
  text: string; advice: string; test: string; signs: string; foods: string; lab: { date: string; value: number | null; unit: string; status: string } | null };
export type Deficits = { days: number; need: number; items: DeficitItem[] };
// ---------- labs (0.20) ----------
export type LabRow = { name: string; valueText: string; value: number | null; cmp: string; flag: string; unit: string; refText: string; lo: number | null; hi: number | null;
  section: string; comment: string; key: string | null; keyName: string; skip?: boolean };
export type LabPreview = { date: string | null; lab: string; file: string | null; source: 'pdf' | 'claude' | 'manual'; rows: LabRow[] };
export type LabNote = { level: 'bad' | 'warn' | 'info'; title: string; text: string; todo: string };
export type LabTest = { key: string; known: boolean; name: string; printed: string; group: string; unit: string; count: number;
  latest: { date: string; value: number | null; text: string; lo: number | null; hi: number | null; ref: string; status: 'low' | 'high' | 'ok' | ''; comment: string };
  series: [string, number][]; note: LabNote | null };
export type LabReport = { id: number; date: string; lab: string | null; file: string | null; source: string | null; count: number };
export type LabsSummary = { reports: LabReport[]; tests: LabTest[]; findings: (LabNote & { key: string; date: string })[]; groups: string[] };
export type Meal = { time: string; ts: number; label: string; names: string[]; kcal: number; protein: number; carbs: number; fat: number };
export type NutritionDay = { date: string; meals: Meal[]; kcal: number | null; protein: number | null; carbs: number | null; fat: number | null; fiber: number | null; fiberEst?: number | null; estimated?: { entries: number; estimated: number; unknown: string[] } | null;
  items?: { time: string; name: string; kcal: number; kind: 'yazio' | 'history' | 'reference' | 'unknown' | 'supplement'; parts: string[]; fiber: number; potassium: number; magnesium: number; iron: number }[]; sugar: number | null; waterMl: number | null; latePct: number | null };
export type SurveyKind = 'hooper' | 'tqr' | 'brums' | 'stressrec';
export type SurveyRow = { date: string; ts: number; answers: number[]; score: number; extra: Record<string, number> | null };
export type MedComponent = { key: string; name: string; amount: number; unit: string; label?: string };
export type MedExtra = {
  source?: string; brand?: string; serving?: string; catalogKind?: CatalogKind; countFood?: boolean; waterMl?: number | null;
  inn?: string | null; group?: string | null;
};
export type MedGroup = { key: string; name: string; note: string };
export type MedEffect = { with: number; without: number; hrvPct: number | null; rhr: number | null; sleepMin: number | null; ready: number | null };
export type Med = {
  id: number; kind: 'supplement' | 'medicine'; name: string; form: string | null; dose: number; schedule: 'daily' | 'asneeded' | 'course';
  components: MedComponent[]; courseFrom: string | null; courseTo: string | null; notes: string | null; archived: boolean; extra?: MedExtra;
  today?: number; todayTime?: string; days?: [string, number][]; active?: boolean; tips?: string[];
  effect?: { nextMorning?: MedEffect; course?: MedEffect } | null; groupInfo?: MedGroup | null;
};
export type MedNote = { group: string; groupName: string; med: string; level: 'warn' | 'info'; text: string };
export type MedTiming = { code: string; title: string; text: string; days: number; today: boolean; last: string | null };
export type MedsSummary = {
  items: Med[]; today: [string, number][]; supp7: Record<string, number>;
  warnings: { key: string; avg: number; supp: number; limit: number; note: string }[];
  timing?: MedTiming[]; notes?: MedNote[]; unknown?: string[]; enabled?: boolean;
};
export type CatalogKind = 'vitamin' | 'protein' | 'isotonic' | 'amino' | 'other';
export type CatalogBrands = { version: string | null; brands: { key: string; name: string; count: number; kinds: Record<CatalogKind, number> }[] };
export type CatalogItem = { id: string; brand: string; name: string; serving: string; form: string; kind: CatalogKind; preview: string; count: number };
export type OnlineItem = { id: string; brand: string; name: string; serving: string; offMarket: number; prep: Partial<Med> };
export type DrugHit = { trade: string; inn: string; form: string; dose: string; group: MedGroup | null; source: 'registry' | 'builtin' };
export type DrugRegistry = { builtin: number; builtinVersion: string | null; builtinSource: string | null; local: number; localVersion: string | null; online: boolean };
export type MuscleGroupState = {
  key: string; name: string; state: 'fresh' | 'normal' | 'loaded' | 'over'; ratio: number | null; now: number; usual: number;
  contrib: { id: string; sport: string; start: number; name: string | null; share: number }[];
};
export type PsychSummary = {
  today: string; hooper: SurveyRow[]; tqr: SurveyRow[]; brums: SurveyRow[]; stressrec: SurveyRow[];
  unrated: { id: string; sport: string; name: string | null; startUtc: number }[]; trainedToday: number;
  due: { hooper: boolean; tqr: boolean; brumsDays: number | null; stressrecDays: number | null };
};
export type VitalRow = { key: string; label: string; value: number | null; unit: string; base: number | null; lo: number | null; hi: number | null; state: 'normal' | 'better' | 'worse' | 'bad' | 'unknown' };
export type ReadyComp = { key: string; label: string; value: number | null; baseline: number | null; score: number | null; weight: number; note: string };
export type ReadinessDay = {
  date: string;
  morning: { score: number | null; level: string; frozen: boolean; components: ReadyComp[]; penalties: { text: string; points: number }[] };
  evening: Evening | null;
  psych: { kind: string; score: number | null; extra: Record<string, number> | null; ts: number }[];
  vitals: VitalRow[];
};
export type StressDay = {
  score: number | null; level: string | null; norm: number | null; hr: number | null; hrv: number | null;
  from: string; to: string; levels: [number, number, number, number]; bins: [number, number][];
};
export type Evening = { score: number; level: 'green' | 'yellow' | 'red'; morning: number; delta: number; items: { text: string; points: number }[] };
export type HealthDay = {
  date: string; tz: number;
  sleep: { totalS: number; deepS: number | null; coreS: number | null; remS: number | null; awakeS: number | null } | null;
  hypnogram: { date: string; start: number; end: number; segs: [string, number, number][] } | null;
  hrv: number | null; rhr: number | null; hrRecovery: number | null; walkHr: number | null;
  resp: number | null; breath: number | null; spo2: number | null; tempShift: number | null;
  steps: number | null; activeKcal: number | null; kcal: number | null; protein: number | null; carbs: number | null; fatG: number | null; waterMl: number | null;
  alcohol: number | null; caffeine: number | null; caffeineLast: string | null; vo2: number | null; weight: number | null; fatPct: number | null;
  activities: { id: string; sport: string; name: string | null; startUtc: number; durationS: number | null; distanceM: number | null; avgHr: number | null; load: number | null }[];
  habits: [string, number][];
  stress?: StressDay | null;
  readiness?: ReadinessDay;
  meds?: { id: number; name: string; kind: string; servings: number }[];
};
export type Habit = { key: string; name: string; auto: string | null; today: number | null; effect: { with: number; without: number; pct: number | null } };
export type HealthSummary = {
  today: string; tz: number;
  switches?: { group: string; name: string; date: string; from: string; to: string }[];
  signals: HealthSignal[];
  signalMatrix: { key: string; name: string; unit: string; days: [string, number | null, number][] }[];
  night: HealthNight | null;
  hypnogram: { date: string; start: number; end: number; segs: [string, number, number][] } | null;
  wakeups: { count: number; min: number } | null;
  regularityMin: number | null;
  nights: HealthNight[];
  need: { baseMin: number; loadMin: number; repayMin: number; napMin: number; illMin: number; totalMin: number; debtMin: number;
    savedBaseMin?: number; personalMin?: number | null; personalNights?: number; bandMin?: number; needMode?: 'auto' | 'manual'; chronic?: string | null };
  plan: { wake: string; bed: string; lights: string; weekend: boolean; caffeineCut: string; date?: string; accepted?: { date: string; mode: 'earlier' | 'later'; min: number } | null };
  schedule: { weekday: string; weekend: string; baseMin: number; remind?: boolean; needMode?: 'auto' | 'manual' };
  scheduleSource?: 'user' | 'watch' | 'default';
  rhythm?: { bed: string; wake: string; nights: number } | null;
  suggestion?: { min: number; reasons: string[]; date: string; earlierBed: string; laterWake: string } | null;
  nightState?: { phase: 'day' | 'before_sleep' | 'evening' | 'lights' | 'sleep'; now: number; bedAbs: number; wakeAbs: number; minToBed: number; sleptToday: boolean; useYesterday: boolean; debtMin: number };
  hrv: { series: Pt[]; last30: Pt[]; band: Band; nightly?: boolean };
  stress?: { series: Pt[]; today: StressDay | null; norm: number | null };
  rhr: { series: Pt[]; last30: Pt[]; band: Band; nightly?: boolean };
  vo2: { series: Pt[]; lab: { value: number; date: string; where: string } | null };
  body: { weighings: { date: string; weight: number; fat: number | null; lean: number | null }[]; steps7: number | null; steps: Pt[]; weight: Pt[]; fat: Pt[]; lean: Pt[] };
  alcohol: { days: Pt[]; known: number; hrvAfterPct: number | null; rhrAfter: number | null; session: { ts: number; units: number; volumeMl: number | null }[]; weightKg: number | null };
  caffeine: { avg: number | null; latest: string | null; lateDays: number; days: [string, string, number | null][]; known: number };
  habits: Habit[];
};
/** [count, seconds, metres, climb m, load] */
export type WeekAgg = [number, number, number, number, number];
export type TrainingOverview = {
  tolerance: { days: { date: string; km: number; extra: number; walk: number }[]; avgWeekRun: number; avgWeekWalk: number };
  history: { week: string; s: Record<string, WeekAgg> }[];
  sportsYear: string[];
  intensityWeeks: { week: string; run: number[]; all: number[]; runs: number }[];
  shoeQuestion: { id: string; sport: string; name: string | null; startUtc: number; distanceM: number | null } | null;
};
export type StrengthRecord = { exercise: string; orm: number; weightKg: number; reps: number; startUtc?: number; date?: string; activityId: string | null; manual: boolean };
export type LongestRec = { activityId: string; distanceM: number | null; timeS: number | null; startUtc: number } | null;
export type RecordsExtra = {
  strength: StrengthRecord[]; manual: { exercise: string; weightKg: number; reps: number; date: string }[];
  longestRun: LongestRec; longestWalk: LongestRec; longestSession: LongestRec; hidden: string[];
};

export const api = {
  status: () => call<Status>('app_status'),
  unlock: () => call<boolean>('unlock'),
  setLockEnabled: (enabled: boolean) => call<boolean>('set_lock_enabled', { enabled }),
  secretStatus: () => call<Record<SecretName, boolean>>('secret_status'),
  secretSet: (name: SecretName, value: string) => call<null>('secret_set', { name, value }),
  secretDelete: (name: SecretName) => call<null>('secret_delete', { name }),
  sources: () => call<Source[]>('sources_list'),
  backups: () => call<BackupInfo[]>('backups_list'),
  backupNow: () => call<string>('backup_now'),
  openDataDir: () => call<null>('open_data_dir'),
  recoveryKey: () => call<string | null>('recovery_key'),
  activities: (f: ListFilter, limit: number, offset: number) =>
    call<ActivityPage>('activities_list', { sport: f.sport ?? null, days: f.days ?? null, query: f.query || null, sort: f.sort ?? null, limit, offset }),
  activityGet: (id: string) => call<ActivityDetail>('activity_get', { id }),
  activityUpdate: (id: string, rpe: number | null, notes: string | null) => call<null>('activity_update', { id, rpe, notes }),
  strengthSetsSave: (id: string, sets: SetRow[]) => call<null>('strength_sets_save', { id, sets }),
  ofpSave: (id: string, blocks: OfpBlock[]) => call<null>('ofp_save', { id, blocks }),
  ofpLast: (id: string) => call<OfpLast>('ofp_last', { id }),
  manualState: () => call<ManualState>('manual_state'),
  manualSave: (date: string, values: { hrv?: number | null; resp?: number | null; spo2?: number | null; kind?: string }) => call<null>('manual_save', { date, values }),
  deviceChangeDecide: (changes: DeviceChange[], reset: boolean) => call<null>('device_change_decide', { changes, reset }),
  complexesGet: () => call<{ complexes: Complex[]; custom: ExerciseItem[] }>('complexes_get'),
  complexesSave: (complexes: Complex[], custom: CustomExercise[]) => call<null>('complexes_save', { complexes, custom }),
  complexesImport: (text: string) => call<{ complexes: number; exercises: number }>('complexes_import', { text }),
  exercises: () => call<string[]>('exercises_list'),
  exerciseLibrary: () => call<ExerciseLibrary>('exercise_library'),
  bandsGet: () => call<ResBand[]>('bands_get'),
  bandsSet: (bands: ResBand[]) => call<null>('bands_set', { bands }),
  shoesGet: () => call<Shoe[]>('shoes_get'),
  shoesSet: (shoes: Shoe[]) => call<null>('shoes_set', { shoes }),
  shoeRuns: (id: string) => call<ShoeRun[]>('shoe_runs', { id }),
  shoeAdvice: () => call<ShoeAdvice>('shoe_advice'),
  openLink: (url: string): Promise<null> => { if (inTauri) return call<null>('open_link', { url }); window.open(url, '_blank'); return Promise.resolve(null); },
  shoeConditionsSet: (temp: string, surface: string) => call<null>('shoe_conditions_set', { temp, surface }),
  planCheckChoice: (date: string, items: PlanItem[]) => call<ChoiceCheck>('plan_check_choice', { date, items }),
  planChoose: (date: string, items: PlanItem[]) => call<ChoiceResult>('plan_choose', { date, items }),
  planConfirm: (date: string) => call<null>('plan_confirm', { date }),
  activitySetShoe: (id: string, shoe: string | null) => call<null>('activity_set_shoe', { id, shoe }),
  trainingOverview: () => call<TrainingOverview>('training_overview'),
  recordsExtra: () => call<RecordsExtra>('records_extra'),
  settingListSet: (key: 'records_hidden' | 'strength_manual', value: unknown[]) => call<null>('setting_list_set', { key, value }),
  healthSummary: () => call<HealthSummary>('health_summary'),
  healthDay: (date: string) => call<HealthDay>('health_day', { date }),
  nutritionSummary: () => call<NutritionSummary>('nutrition_summary'),
  nutritionDay: (date: string) => call<NutritionDay>('nutrition_day', { date }),
  habitMark: (date: string, key: string, val: number | null) => call<null>('habit_mark', { date, key, val }),
  psychSummary: () => call<PsychSummary>('psych_summary'),
  medsSummary: () => call<MedsSummary>('meds_summary'),
  sourcesState: () => call<SourcesState>('sources_state'),
  sourcesPreview: (group: string, provider: string) => call<SourcePreview>('sources_preview', { group, provider }),
  sourcesSet: (group: string, provider: string, mode: 'keep' | 'replace') => call<null>('sources_set', { group, provider, mode }),
  sourcesMapSet: (provider: string, canon: string, metric: string | null) => call<null>('sources_map_set', { provider, canon, metric }),
  sourcesSetAll: (provider: string, mode: 'keep' | 'replace') => call<null>('sources_set_all', { provider, mode }),
  haeState: () => call<HaeState>('hae_state'),
  haeSet: (patch: { enabled?: boolean; port?: number; folder?: string; newToken?: boolean }) => call<HaeState>('hae_set', { patch }),
  haeImportFile: (path: string) => call<HaeImport>('hae_import_file', { path }),
  haeScan: () => call<number>('hae_scan'),
  workoutsState: () => call<WorkoutSources>('workouts_state'),
  workoutsSave: (data: Partial<Pick<WorkoutSources, 'mode' | 'providers' | 'rules'>>) => call<null>('workouts_save', { data }),
  dupsPending: () => call<DupGroup[]>('dups_pending'),
  stravaGone: () => call<StravaGone[]>('strava_gone'),
  stravaGoneDecide: (id: string, del: boolean) => call<null>('strava_gone_decide', { id, delete: del }),
  dupDecide: (group: string, keep: string | null, remember: boolean) => call<null>('dup_decide', { group, keep, remember }),
  workoutChoose: (id: string) => call<null>('workout_choose', { id }),
  workoutsDelete: (ids: string[]) => call<null>('workouts_delete', { ids }),
  workoutRestore: (id: string) => call<null>('workout_restore', { id }),
  workoutPurge: (id: string) => call<null>('workout_purge', { id }),
  workoutsDeleted: () => call<DeletedWorkout[]>('workouts_deleted'),
  bodyState: () => call<BodyState>('body_state'),
  checkinSave: (id: string, data: CheckinData) => call<null>('checkin_save', { id, data }),
  painMark: (data: { area: string; side?: string; level: number; context?: 'manual' | 'morning'; note?: string; date?: string }) => call<null>('pain_mark', { data }),
  painDelete: (id: number) => call<null>('pain_delete', { id }),
  returnStart: (area: string, stage: number) => call<null>('return_start', { area, stage }),
  returnStop: () => call<null>('return_stop'),
  planState: () => call<PlanState>('plan_state'),
  planDecide: (id: number, accept: boolean) => call<null>('plan_decide', { id, accept }),
  planSetDay: (date: string, items: PlanItem[]) => call<null>('plan_set_day', { date, items }),
  planResetDay: (date: string) => call<null>('plan_reset_day', { date }),
  planTemplateSave: (days: PlanItem[][], rebuild: boolean) => call<null>('plan_template_save', { days, rebuild }),
  coachState: () => call<CoachState>('coach_state'),
  coachProfileSave: (profile: CoachProfile) => call<null>('coach_profile_save', { profile }),
  coachNotesSave: (text: string) => call<null>('coach_notes_save', { text }),
  coachDay: () => call<CoachReport>('coach_day'),
  coachWeek: () => call<CoachReport>('coach_week'),
  coachPackage: (reportId: number | null, question?: string) => call<string>('coach_package', { reportId, question: question ?? null }),
  coachSaveAnswer: (parent: number | null, text: string) => call<CoachReport>('coach_save_answer', { parent, text }),
  coachDelete: (id: number) => call<null>('coach_delete', { id }),
  medSave: (med: Partial<Med>) => call<number>('med_save', { med }),
  medDelete: (id: number) => call<null>('med_delete', { id }),
  medTake: (id: number, date: string, servings: number, time?: string) => call<null>('med_take', { id, date, servings, time: time ?? null }),
  catalogBrands: () => call<CatalogBrands>('catalog_brands'),
  catalogSearch: (brand: string | null, kind: string | null, q: string) => call<CatalogItem[]>('catalog_search', { brand, kind, q }),
  catalogProduct: (id: string) => call<Partial<Med> | null>('catalog_product', { id }),
  catalogOnline: (q: string) => call<OnlineItem[]>('catalog_online', { q }),
  catalogLabel: (text: string) => call<Partial<Med>>('catalog_label', { text }),
  drugSearch: (q: string) => call<DrugHit[]>('drug_search', { q }),
  drugOnline: (inn: string) => call<{ classes: string[]; group: MedGroup | null }>('drug_online', { inn }),
  drugRegistryInfo: () => call<DrugRegistry>('drug_registry_info'),
  drugRegistryUpdate: () => call<number>('drug_registry_update'),
  suppOnlineSet: (on: boolean) => call<null>('supp_online_set', { on }),
  medsEnabled: () => call<boolean>('meds_enabled'),
  labsParse: (path: string) => call<LabPreview>('labs_parse', { path }),
  labsParseText: (text: string) => call<LabPreview>('labs_parse_text', { text }),
  labsSave: (report: LabPreview) => call<number>('labs_save', { report }),
  labsSummary: () => call<LabsSummary>('labs_summary'),
  labsDelete: (id: number) => call<null>('labs_delete', { id }),
  labsOpen: (id: number) => call<null>('labs_open', { id }),
  deficitSnooze: (key: string) => call<null>('deficit_snooze', { key }),
  medsEnabledSet: (on: boolean) => call<null>('meds_enabled_set', { on }),
  surveySave: (kind: SurveyKind, answers: number[], date?: string) => call<{ score: number; extra: unknown }>('survey_save', { kind, answers, date: date ?? null }),
  surveyDelete: (kind: SurveyKind, date: string) => call<null>('survey_delete', { kind, date }),
  healthSettingSet: (key: 'sleep_schedule' | 'habits_custom' | 'lab_vo2' | 'sleep_tonight' | 'nutrient_norms', value: unknown) => call<null>('health_setting_set', { key, value }),
  mapCacheInfo: () => call<{ enabled: boolean; tiles: number; bytes: number }>('map_cache_info'),
  mapSetEnabled: (enabled: boolean) => call<null>('map_set_enabled', { enabled }),
  mapCacheClear: () => call<null>('map_cache_clear'),
  activityStats: () => call<ActivityStats>('activities_stats'),
  importPreview: (paths: string[]) => call<Preview>('import_preview', { paths }),
  importCommit: (merge: boolean) => call<ImportResult>('import_commit', { merge }),
  importCancel: () => call<null>('import_cancel'),
  importHistory: () => call<ImportLog[]>('import_history'),
  exportActivities: (days: number | null, format: 'csv' | 'gpx' | 'original') =>
    call<ExportResult>('export_activities', { days, format }),
  stravaStatus: () => call<SourceStatus>('strava_status'),
  stravaConnect: () => call<string>('strava_connect'),
  stravaSync: (full: boolean) => call<StravaSync>('strava_sync', { full }),
  stravaDisconnect: () => call<null>('strava_disconnect'),
  freddyStatus: () => call<SourceStatus>('freddy_status'),
  freddySync: (full: boolean) => call<FreddySync>('freddy_sync', { full }),
  healthOverview: () => call<HealthOverview>('health_overview'),
  engineSummary: () => call<EngineSummary>('engine_summary'),
  profileGet: () => call<AthleteProfile & { hrRestAuto: number }>('profile_get'),
  profileSet: (profile: AthleteProfile) => call<null>('profile_set', { profile }),
};

// ---------- stage 3: engine ----------

export type AthleteProfile = {
  name?: string | null; filled?: boolean;
  hrMax: number; hrRest: number | null; zones: number[]; heightCm: number; birthYear: number; male: boolean;
  proteinGPerKg: number; zonesDate: string | null; thresholdPaceS: number | null; weekCap?: number; paceZones?: number[];
  proteinManual?: number | null; goal?: 'maintain' | 'lose' | 'gain'; goalRate?: number; targetWeight?: number | null;
};
export type Split = { low: number; mid: number; high: number; hours: number };
export type TrendJ = { latest: [string, number] | null; smooth: number | null; change7d: number | null; change30d: number | null };
export type Targets = { kcal: number; protein: number; carbs: number; fat: number; fiber: number; waterMl: number; bmr?: number };
export type EngineSummary = {
  today: string;
  profile: AthleteProfile;
  hrRestUsed: number;
  readiness: {
    score: number | null; level: 'green' | 'yellow' | 'red' | 'unknown';
    components: { key: string; label: string; value: number | null; baseline: number | null; score: number | null; weight: number; note: string }[];
    penalties: { text: string; points: number }[];
    history: [string, number | null][];
    frozen?: boolean; calibrating?: { group: string; name: string; from: string; daysLeft: number; provider: string }[]; hrvNightly?: boolean; rhrNightly?: boolean; evening?: Evening | null; eveningHistory?: [string, number][]; eveningOn?: boolean; eveningFrom?: string;
  };
  stress?: { today: StressDay | null; history: [string, number | null][] };
  muscleMap?: MuscleGroupState[];
  /** 1.4: strength / ОФП workouts of the last 6 days whose load is not on the map (no exercises). */
  /** 1.4: another watch now writes these groups (by the device field) — start the norm again? */
  deviceChange?: DeviceChange[];
  /** 1.4.1: steps today vs your usual day (median of 30 days before). */
  steps?: { today: number | null; usual: number | null; avg7: number | null; last14: [string, number][] };
  /** 1.4: HRV (and breathing / SpO₂) typed in by hand — null when every group has a device. */
  manualEntry?: ManualState | null;
  muscleMissing?: { id: string; sport: string; start: number; name: string | null; free: number; total: number }[];
  recovery?: { leftH: number; factor: number; workouts: { id: string; sport: string; end: number; hours: number; heartH: number; muscleH: number; leftH: number; muscle: number | null; up: number | null; down: number | null }[] };
  load: {
    today: number; atl: number; ctl: number; tsb: number; acwr: number | null; week: number;
    state: 'unknown' | 'low' | 'optimal' | 'high' | 'very_high'; stateText: string;
    series: [string, number, number, number, number][];
    muscle?: { today: number; week: number; usualWeek: number; acwr: number | null; groups7: { key: string; name: string; value: number }[] };
    history?: [string, number, number, number | null, number | null, number | null, number][];
    acts?: { id: string; start: number; sport: string; name: string | null; heart: number | null; muscle: number; up: number; down: number; bad: boolean }[];
  };
  intensity: { d7: Split | null; d28: Split | null; zones28Min: number[] };
  weeks: { week: string; load: number; hours: number; runKm: number; count: number }[];
  records: { distanceM: number; timeS: number; date: string; activityId: string }[];
  predictions: { distanceM: number; timeS: number; fromDistanceM: number; fromTimeS: number; fromDate: string }[];
  sleep: { date: string; totalS: number; deepS: number; remS: number; coreS: number; awakeS: number; score: number }[];
  body: { weight: TrendJ; fatPct: TrendJ; lean: TrendJ };
  nutrition: {
    days: { date: string; kcal: number | null; protein: number | null; carbs: number | null; fat: number | null; fiber: number | null; waterMl: number | null; target: Targets }[];
    targetToday: Targets; weightUsed: number;
  };
  signals: { level: 'warn' | 'info' | 'good'; code: string; text: string }[];
  meta: { activities: number; byHr: number; byAvgHr: number; estimated: number; ms: number };
  day: {
    advice: { title: string; text: string; tier?: string; bodyTier?: string; reasons?: string[]; hint?: string | null; evening?: boolean };
    targetLoad: number; usualLoad: number; todayLoad: number;
    target?: { lo: number; hi: number; headroom: number | null; capped: boolean; cap: number; left: number };
    options?: { kind: string; title: string; text: string }[];
    todayActivities: DayAct[]; recent: DayAct[];
    week: { date: string; load: number; sports: string[]; minutes: number; readiness: number | null }[];
    usualReadiness: number | null;
    alcoholYesterday: number | null;
    vitals: VitalRow[];
  };
};
export type DayAct = {
  id: string; sport: string; name: string | null; startUtc: number; durationS: number; distanceM: number | null;
  load: number; relativeEffort: number | null; method: string;
};

// ---------- formatting ----------

export const SPORTS: Record<string, { name: string; letter: string; color: string }> = {
  run: { name: 'Бег', letter: 'Б', color: '#3DDC97' },
  walk: { name: 'Ходьба', letter: 'Х', color: '#7FD3FF' },
  hike: { name: 'Поход', letter: 'П', color: '#7FD3FF' },
  ride: { name: 'Велосипед', letter: 'В', color: '#F5B642' },
  ride_virtual: { name: 'Виртуальный заезд', letter: 'В', color: '#F5B642' },
  strength: { name: 'Силовая', letter: 'С', color: '#FF8A65' },
  workout: { name: 'Тренировка', letter: 'Т', color: '#C792EA' },
  yoga: { name: 'Йога', letter: 'Й', color: '#C792EA' },
  swim: { name: 'Плавание', letter: 'П', color: '#7FD3FF' },
  elliptical: { name: 'Эллипс', letter: 'Э', color: '#A78BFA' },
  stepper: { name: 'Степпер', letter: 'С', color: '#FF6B6B' },
  other: { name: 'Другое', letter: '•', color: '#9AA3AE' },
};
export const sportOf = (s: string) => SPORTS[s] ?? SPORTS.other;

export function fmtKm(m: number | null | undefined, dec = 1): string {
  if (m == null || m <= 0) return '—';
  return `${(m / 1000).toFixed(dec).replace('.', ',')} км`;
}

export function fmtDur(s: number | null | undefined): string {
  if (s == null) return '—';
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = Math.round(s % 60);
  if (h > 0) return `${h}:${String(m).padStart(2, '0')}:${String(sec).padStart(2, '0')}`;
  return `${m}:${String(sec).padStart(2, '0')}`;
}

/** Pace from seconds per km, "6:03". */
export function fmtPace(secPerKm: number | null | undefined): string {
  if (secPerKm == null || !isFinite(secPerKm) || secPerKm <= 0 || secPerKm > 3600) return '—';
  const m = Math.floor(secPerKm / 60), s = Math.round(secPerKm % 60);
  return s === 60 ? `${m + 1}:00` : `${m}:${String(s).padStart(2, '0')}`;
}

export function fmtHours(s: number): string {
  const h = Math.floor(s / 3600);
  const m = Math.round((s % 3600) / 60);
  return h > 0 ? `${h} ч ${m} мин` : `${m} мин`;
}

export function fmtWhen(unix: number): string {
  return new Date(unix * 1000).toLocaleString('ru-RU', {
    day: 'numeric', month: 'short', year: 'numeric', hour: '2-digit', minute: '2-digit',
  }).replace(' г.', '');
}

export function fmtDay(unix: number): string {
  return new Date(unix * 1000).toLocaleDateString('ru-RU', { day: 'numeric', month: 'long', year: 'numeric' });
}

export function fmtIso(iso: string | null | undefined): string {
  if (!iso) return '—';
  const d = new Date(iso.includes('T') || iso.includes(' ') ? iso.replace(' ', 'T') + (iso.endsWith('Z') ? '' : 'Z') : iso);
  if (isNaN(d.getTime())) return iso;
  return d.toLocaleString('ru-RU', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
}

export function plural(n: number, one: string, few: string, many: string): string {
  const m10 = n % 10, m100 = n % 100;
  const w = m10 === 1 && m100 !== 11 ? one : m10 >= 2 && m10 <= 4 && (m100 < 10 || m100 >= 20) ? few : many;
  return `${n.toLocaleString('ru-RU')} ${w}`;
}

export function errText(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return 'Что-то пошло не так';
}

export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} Б`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} КБ`;
  return `${(n / 1024 / 1024).toFixed(1).replace('.', ',')} МБ`;
}

export function fmtDateTime(sec: number): string {
  return new Date(sec * 1000).toLocaleString('ru-RU', {
    day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit',
  });
}
