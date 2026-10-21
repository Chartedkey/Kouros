// Shoes (0.23): characteristics table with Russian labels (filled by hand or pasted from a review
// table — RunRepeat style), today's pick with reasons and the two-tap weather question.
import { useState } from 'react';
import { api, errText, type ShoeAdvice, type ShoePace, type ShoeSpec } from '../lib/api';

type NumKey = 'sa' | 'er' | 'traction' | 'weightG' | 'dropMm' | 'softness' | 'coldPct' | 'outsoleMm' | 'toebox' | 'heelPad' | 'breath' | 'widthMm' | 'toeboxMm' | 'heelMm' | 'foreMm';

/** Rows of the table: key, label, unit, hint. */
const ROWS: [NumKey, string, string, string][] = [
  ['sa', 'Амортизация', 'SA', 'Сколько удара гасит подошва (Shock absorption). Около 100 — жёстко, 140+ — очень мягко.'],
  ['er', 'Возврат энергии', '%', 'Насколько подошва «пружинит» (Energy return). 70%+ — для темпа и стартов.'],
  ['traction', 'Сцепление', '0–1', 'Как держит на мокром (Traction). 0,5+ — хорошо, ниже 0,4 — скользко.'],
  ['weightG', 'Вес пары', 'г', 'Вес одного кроссовка (Weight). Для быстрых работ лучше легче 250 г.'],
  ['dropMm', 'Перепад', 'мм', 'Разница высоты пятки и носка (Drop). Низкий (<6) нагружает ахилл и икры.'],
  ['heelMm', 'Высота под пяткой', 'мм', 'Heel stack. 35+ — максимальная амортизация.'],
  ['foreMm', 'Высота под носком', 'мм', 'Forefoot stack.'],
  ['softness', 'Мягкость пены', '', 'Midsole softness (AC или HA): чем меньше число, тем мягче.'],
  ['coldPct', 'Дубеет на холоде', '%', 'На сколько пена твердеет в мороз (Difference in midsole softness in cold). 20%+ — заметно.'],
  ['outsoleMm', 'Толщина резины подошвы', 'мм', 'Outsole durability: чем толще, тем дольше прослужит.'],
  ['toebox', 'Прочность носка', '1–5', 'Toebox durability.'],
  ['heelPad', 'Прочность пятки внутри', '1–5', 'Heel padding durability.'],
  ['breath', 'Воздухопроницаемость', '1–5', 'Breathability: 4–5 — для жары.'],
  ['widthMm', 'Ширина колодки', 'мм', 'Width / fit.'],
  ['toeboxMm', 'Ширина носка', 'мм', 'Toebox width: 75+ — свободно пальцам.'],
];
const PACES: [ShoePace, string][] = [['daily', 'на каждый день'], ['recovery', 'восстановительные'], ['tempo', 'темп и интервалы'], ['race', 'старты'], ['trail', 'трейл']];

const num = (s: string) => {
  const m = s.replace(',', '.').match(/-?\d+(\.\d+)?/);
  return m ? Number(m[0]) : undefined;
};

/** Labels of the review table (English, as on RunRepeat) → how to read them. */
const LABELS: [string, (v: string, sp: ShoeSpec) => void][] = [
  ['Difference in midsole softness in cold', (v, sp) => { sp.coldPct = num(v); }],
  ['Heel padding durability', (v, sp) => { sp.heelPad = num(v); }],
  ['Heel counter stiffness', (v, sp) => { ex(sp, 'Жёсткость задника', v); }],
  ['Torsional rigidity', (v, sp) => { ex(sp, 'Жёсткость на скручивание', v); }],
  ['Outsole durability', (v, sp) => { sp.outsoleMm = num(v); }],
  ['Toebox durability', (v, sp) => { sp.toebox = num(v); }],
  ['Midsole softness', (v, sp) => { sp.softness = num(v); sp.softUnit = /HA/i.test(v) ? 'HA' : 'AC'; }],
  ['Shock absorption', (v, sp) => { sp.sa = num(v); }],
  ['Widths available', (v, sp) => { ex(sp, 'Доступные ширины', v); }],
  ['Orthotic friendly', (v, sp) => { ex(sp, 'Под ортопедические стельки', yes(v) ? 'да' : 'нет'); }],
  ['Removable insole', (v, sp) => { ex(sp, 'Съёмная стелька', yes(v) ? 'да' : 'нет'); }],
  ['Energy return', (v, sp) => { sp.er = num(v); }],
  ['Strike pattern', (v, sp) => { ex(sp, 'Постановка стопы', v); }],
  ['Arch support', (v, sp) => { sp.arch = /stabil|motion/i.test(v) ? 'stability' : 'neutral'; }],
  ['Toebox width', (v, sp) => { sp.toeboxMm = num(v); }],
  ['Breathability', (v, sp) => { sp.breath = num(v); }],
  ['Weight brand', (v, sp) => { if (sp.weightG == null) sp.weightG = grams(v); }],
  ['Weight lab', (v, sp) => { sp.weightG = grams(v) ?? sp.weightG; }],
  ['Forefoot brand', (v, sp) => { if (sp.foreMm == null) sp.foreMm = num(v); }],
  ['Forefoot lab', (v, sp) => { sp.foreMm = num(v) ?? sp.foreMm; }],
  ['Heel brand', (v, sp) => { if (sp.heelMm == null) sp.heelMm = num(v); }],
  ['Heel lab', (v, sp) => { sp.heelMm = num(v) ?? sp.heelMm; }],
  ['Drop brand', (v, sp) => { if (sp.dropMm == null) sp.dropMm = num(v); }],
  ['Drop lab', (v, sp) => { sp.dropMm = num(v) ?? sp.dropMm; }],
  ['Width / fit', (v, sp) => { sp.widthMm = num(v); }],
  ['Stiffness', (v, sp) => { ex(sp, 'Жёсткость на изгиб', v); }],
  ['Traction', (v, sp) => { sp.traction = num(v); }],
  ['Our score', (v, sp) => { ex(sp, 'Оценка обзора', v); }],
  ['Rocker', (v, sp) => { sp.rocker = yes(v); }],
  ['Waterproofing', (v, sp) => { sp.membrane = wet(v); }],
  ['Waterproof', (v, sp) => { sp.membrane = wet(v); }],
  ['Gore-Tex', (v, sp) => { sp.membrane = wet(v); }],
  ['Season', (v, sp) => { sp.season = /winter/i.test(v) ? 'winter' : /summer/i.test(v) ? 'summer' : 'all'; }],
  ['Size', (v, sp) => { ex(sp, 'Размер', v); }],
  ['Pace', (v, sp) => {
    const p: ShoePace[] = [];
    if (/daily/i.test(v)) p.push('daily');
    if (/recover/i.test(v)) p.push('recovery');
    if (/tempo/i.test(v)) p.push('tempo');
    if (/compet|race/i.test(v)) p.push('race');
    if (/trail/i.test(v)) p.push('trail');
    if (p.length) sp.pace = p;
  }],
];
function ex(sp: ShoeSpec, k: string, v: string) { const t = v.trim(); if (t) sp.extra = { ...(sp.extra ?? {}), [k]: t }; }
function yes(v: string) { return /✓|yes|да|✔/i.test(v) || v.trim() === ''; }
function wet(v: string) { return !/^\s*(no|нет|none|-|✗|✕)\b/i.test(v) && /✓|✔|yes|да|gore|gtx|membrane|waterproof/i.test(v); }
function grams(v: string) { const m = v.replace(',', '.').match(/(\d+(\.\d+)?)\s*g\b/i); return m ? Number(m[1]) : undefined; }

/** Reads a table copied from a review page: «Label value» pairs in any layout. */
export function parseTable(text: string): ShoeSpec {
  const sp: ShoeSpec = {};
  const t = text.replace(/\r/g, '');
  type Hit = { at: number; end: number; i: number };
  const hits: Hit[] = [];
  LABELS.forEach(([lab], i) => {
    // Labels start a line or a cell; case matters, so «True to size» is not the label «Size».
    const re = new RegExp('(^|[\\n\\t])(' + lab.replace(/[.*+?^${}()|[\]\\/]/g, '\\$&') + ')', 'g');
    let m: RegExpExecArray | null;
    while ((m = re.exec(t))) {
      const at = m.index + m[1].length, end = at + m[2].length;
      if (!hits.some((h) => at < h.end && end > h.at)) hits.push({ at, end, i });
    }
  });
  hits.sort((a, b) => a.at - b.at);
  // Pairs like «Weight lab / Weight brand» often come as two labels followed by two values.
  hits.forEach((h, k) => {
    let v = t.slice(h.end, k + 1 < hits.length ? hits[k + 1].at : t.length).trim();
    const nextLab = LABELS[hits[k + 1]?.i ?? -1]?.[0] ?? '';
    const lab = LABELS[h.i][0];
    if (!v && / lab$/i.test(lab) && / brand$/i.test(nextLab)) {
      const nv = t.slice(hits[k + 1].end, k + 2 < hits.length ? hits[k + 2].at : t.length).trim();
      v = nv; // the first value of the pair is the lab one
    }
    // Rocker / orthotic / insole: an empty cell after the label means «no» only if the next line is not a tick.
    if (!v && ['Rocker', 'Orthotic friendly', 'Removable insole', 'Waterproofing', 'Waterproof', 'Gore-Tex'].includes(lab)) return;
    LABELS[h.i][1](v, sp);
  });
  return sp;
}

export function SpecEditor({ spec, url, name = '', onSave, onCancel }: { spec: ShoeSpec; url: string; name?: string; onSave: (s: ShoeSpec, url: string) => void; onCancel: () => void }) {
  const [sp, setSp] = useState<ShoeSpec>(spec);
  const [u, setU] = useState(url);
  const [paste, setPaste] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const setN = (k: NumKey, v: string) => setSp({ ...sp, [k]: v.trim() === '' ? undefined : num(v) });
  const fromPaste = () => {
    if (!paste?.trim()) return;
    const p = parseTable(paste);
    const n = Object.keys(p).filter((k) => k !== 'extra').length + Object.keys(p.extra ?? {}).length;
    if (!n) { setMsg('Не нашёл знакомых строк. Скопируй таблицу целиком — от «Our score» до «Removable insole».'); return; }
    setSp({ ...sp, ...p, extra: { ...(sp.extra ?? {}), ...(p.extra ?? {}) } });
    setMsg(`Разобрал ${n} строк — проверь цифры и нажми «Сохранить».`);
    setPaste(null);
  };
  const togglePace = (p: ShoePace) => { const cur = sp.pace ?? []; setSp({ ...sp, pace: cur.includes(p) ? cur.filter((x) => x !== p) : [...cur, p] }); };
  return (
    <div className="inset" style={{ gap: 12 }}>
      <div className="between"><b className="small">Характеристики пары</b><span className="muted tiny">все поля необязательные</span></div>
      <div className="stack" style={{ gap: 6 }}>
        <span className="muted tiny" style={{ lineHeight: 1.5 }}>Быстрее всего — открыть обзор модели (например, на RunRepeat), выделить таблицу характеристик, скопировать и вставить сюда. Kouros сам разложит цифры.</span>
        {paste == null
          ? <button type="button" className="btn2" style={{ alignSelf: 'flex-start' }} onClick={() => setPaste('')}>Вставить таблицу</button>
          : (
            <>
              <textarea className="input" rows={6} value={paste} onChange={(e) => setPaste(e.target.value)} placeholder={'Pace  Daily running\nShock absorption  151 SA\n…'} />
              <div className="row" style={{ gap: 8 }}><button type="button" className="btn2" disabled={!paste.trim()} onClick={fromPaste}>Разобрать</button><button type="button" className="linkbtn small" onClick={() => setPaste(null)}>Отмена</button></div>
            </>
          )}
        {msg && <span className="small" style={{ color: '#3DDC97' }}>{msg}</span>}
      </div>
      <div className="stack" style={{ gap: 4 }}>
        <span className="small">Назначение</span>
        <div className="row" style={{ gap: 6 }}>{PACES.map(([k, n]) => <button key={k} type="button" className="pill" aria-pressed={(sp.pace ?? []).includes(k)} onClick={() => togglePace(k)}>{n}</button>)}</div>
      </div>
      <div className="row" style={{ gap: 6 }} role="group" aria-label="Поддержка свода">
        <span className="small" style={{ marginRight: 6 }}>Поддержка свода</span>
        <button type="button" className="pill" aria-pressed={sp.arch === 'neutral'} onClick={() => setSp({ ...sp, arch: 'neutral' })}>нейтральные</button>
        <button type="button" className="pill" aria-pressed={sp.arch === 'stability'} onClick={() => setSp({ ...sp, arch: 'stability' })}>стабилизация</button>
      </div>
      <div className="spec-grid">
        {ROWS.map(([k, lab, unit, hint]) => (
          <label key={k} className="spec-row" title={hint}>
            <span className="small">{lab}{unit && <span className="muted tiny"> · {k === 'softness' ? sp.softUnit ?? 'AC' : unit}</span>}</span>
            <input className="input sm" inputMode="decimal" value={sp[k] ?? ''} onChange={(e) => setN(k, e.target.value)} />
            <span className="muted tiny" style={{ lineHeight: 1.4 }}>{hint}</span>
          </label>
        ))}
      </div>
      <div className="row" style={{ gap: 16 }}>
        <label className="row small" style={{ gap: 8 }}><input type="checkbox" checked={!!sp.rocker} onChange={(e) => setSp({ ...sp, rocker: e.target.checked })} /> Рокер (подошва-«качалка»)</label>
        <span className="row" style={{ gap: 6 }}>
          <span className="small">Сезон</span>
          {([['all', 'всесезонные'], ['summer', 'лето'], ['winter', 'зима']] as const).map(([k, n]) => <button key={k} type="button" className="pill" aria-pressed={(sp.season ?? 'all') === k} onClick={() => setSp({ ...sp, season: k })}>{n}</button>)}
        </span>
      </div>
      <div className="row" style={{ gap: 6 }} role="group" aria-label="Мембрана">
        <span className="small" style={{ marginRight: 6 }}>Мембрана (Gore-Tex и т. п.)</span>
        <button type="button" className="pill" aria-pressed={sp.membrane === true} onClick={() => setSp({ ...sp, membrane: true })}>есть</button>
        <button type="button" className="pill" aria-pressed={sp.membrane === false} onClick={() => setSp({ ...sp, membrane: false })}>нет</button>
        {sp.membrane == null && <span className="muted tiny">{/gtx|gore-?tex/i.test(name) ? 'по названию (GTX) похоже, что есть' : 'не указано — считаю, что нет'}</span>}
      </div>
      <span className="muted tiny" style={{ lineHeight: 1.5, marginTop: -6 }}>Мембрана — плюс в дождь, слякоть и снег, минус в жару: нога в ней хуже дышит.</span>
      {sp.extra && Object.keys(sp.extra).length > 0 && (
        <div className="stack" style={{ gap: 2 }}>
          <span className="muted tiny">Для справки (в совете не используется)</span>
          {Object.entries(sp.extra).map(([k, v]) => <span key={k} className="tiny"><span className="muted">{k}:</span> {v}</span>)}
        </div>
      )}
      <label className="stack" style={{ gap: 4 }}><span className="small">Ссылка на обзор модели</span><input className="input" value={u} placeholder="https://…" onChange={(e) => setU(e.target.value)} /></label>
      <div className="row"><button type="button" className="btn" onClick={() => onSave(sp, u.trim())}>Сохранить</button><button type="button" className="btn2" onClick={onCancel}>Отмена</button></div>
    </div>
  );
}

const TEMPS: [string, string][] = [['lt-10', 'ниже −10'], ['-10..0', '−10…0'], ['0..10', '0…+10'], ['10..20', '+10…+20'], ['gt20', 'выше +20']];
const SURF: [string, string][] = [['dry', 'сухо'], ['wet', 'мокро'], ['snow', 'снег'], ['ice', 'лёд'], ['trail', 'грязь / трейл']];

/** «В каких кроссовках сегодня» on «Сегодня». */
export function ShoeTodayCard({ a, reload, onGear }: { a: ShoeAdvice; reload: () => void; onGear?: () => void }) {
  const [t, setT] = useState(a.conditions?.temp ?? '');
  const [s, setS] = useState(a.conditions?.surface ?? '');
  const [edit, setEdit] = useState(!a.conditions);
  const [err, setErr] = useState<string | null>(null);
  if (!a.need || a.shoes === 0) return null;
  const save = async (tt: string, ss: string) => {
    if (!tt || !ss) return;
    try { await api.shoeConditionsSet(tt, ss); setEdit(false); reload(); } catch (e) { setErr(errText(e)); }
  };
  const best = a.picks[0];
  const others = a.picks.slice(1, 3);
  const noSpec = a.picks.some((p) => !p.spec);
  return (
    <section className="card" aria-label="Кроссовки на сегодня">
      <div className="between"><span className="label">Кроссовки на сегодня · {a.need.title}</span>{onGear && <button type="button" className="linkbtn small" onClick={onGear}>Мои кроссовки →</button>}</div>
      {a.shoes > 1 && (edit ? (
        <div className="stack" style={{ gap: 8 }}>
          <span className="small">Что на улице? Два касания — и совет учтёт погоду.</span>
          <div className="row" style={{ gap: 6 }}>{TEMPS.map(([k, n]) => <button key={k} type="button" className="pill" aria-pressed={t === k} onClick={() => { setT(k); save(k, s); }}>{n}</button>)}</div>
          <div className="row" style={{ gap: 6 }}>{SURF.map(([k, n]) => <button key={k} type="button" className="pill" aria-pressed={s === k} onClick={() => { setS(k); save(t, k); }}>{n}</button>)}</div>
        </div>
      ) : (
        <span className="muted small">На улице: {TEMPS.find((x) => x[0] === t)?.[1] ?? '—'}, {SURF.find((x) => x[0] === s)?.[1] ?? '—'} · <button type="button" className="linkbtn small" onClick={() => setEdit(true)}>изменить</button></span>
      ))}
      {err && <div className="note-err">{err}</div>}
      {best && (
        <div className="inset" style={{ gap: 6 }}>
          <b>{best.name}</b>
          {best.plus.length > 0 && <span className="small" style={{ color: '#3DDC97', lineHeight: 1.5 }}>{best.plus.join(' · ')}</span>}
          {best.minus.length > 0 && <span className="small" style={{ color: '#F5B642', lineHeight: 1.5 }}>{best.minus.join(' · ')}</span>}
        </div>
      )}
      {others.map((o) => (
        <div key={o.id} className="list-row small" style={{ alignItems: 'flex-start' }}>
          <span style={{ flex: 1 }}>{o.name}<span className="muted"> · {[...o.minus, ...o.plus].slice(0, 2).join(' · ') || 'запасной вариант'}</span></span>
        </div>
      ))}
      {(a.warn ?? []).map((w) => <span key={w} className="small" style={{ color: '#FF6B6B', lineHeight: 1.5 }}>{w}</span>)}
      {a.shoes > 1 && !a.conditions && !edit && <span className="muted tiny">Погода не указана — совет без её учёта.</span>}
      {noSpec && <span className="muted tiny" style={{ lineHeight: 1.5 }}>У части пар нет характеристик — совет по ним грубее. Заполни их в «Тренировки → Инвентарь».</span>}
    </section>
  );
}
