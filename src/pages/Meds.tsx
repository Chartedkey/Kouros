// «Витамины и лекарства»: your preparations with composition from the label, intake marked by hand.
// 0.18: catalog by brand (Maxler, NOW, Solgar), online search (NIH), label text from Claude,
// medicines from the Russian registry with sport-relevant groups, timing, personal effect.
import { useCallback, useEffect, useState } from 'react';
import {
  api, errText, type CatalogBrands, type CatalogItem, type CatalogKind, type DrugHit, type DrugRegistry, type Med, type MedComponent,
  type MedEffect, type MedExtra, type MedGroup, type MedsSummary, type OnlineItem,
} from '../lib/api';
import { useDataChanged } from '../lib/events';

/** Nutrients that count towards your daily norms (metric, name, unit Kouros stores). */
export const NUTRIENTS: [string, string, string][] = [
  ['dietary_vitamin_a_total', 'Витамин A', 'мкг'], ['dietary_vitamin_c_total', 'Витамин C', 'мг'], ['dietary_vitamin_d_total', 'Витамин D', 'мкг'],
  ['dietary_vitamin_e_total', 'Витамин E', 'мг'], ['dietary_vitamin_k_total', 'Витамин K', 'мкг'], ['dietary_thiamin_total', 'B1 (тиамин)', 'мг'],
  ['dietary_riboflavin_total', 'B2 (рибофлавин)', 'мг'], ['dietary_niacin_total', 'B3 (ниацин)', 'мг'], ['dietary_vitamin_b6_total', 'B6', 'мг'],
  ['dietary_folate_total', 'B9 (фолат)', 'мкг'], ['dietary_vitamin_b12_total', 'B12', 'мкг'], ['dietary_calcium_total', 'Кальций', 'мг'],
  ['dietary_iron_total', 'Железо', 'мг'], ['dietary_magnesium_total', 'Магний', 'мг'], ['dietary_zinc_total', 'Цинк', 'мг'],
  ['dietary_potassium_total', 'Калий', 'мг'], ['dietary_selenium_total', 'Селен', 'мкг'], ['dietary_phosphorus_total', 'Фосфор', 'мг'],
  ['dietary_copper_total', 'Медь', 'мг'], ['dietary_manganese_total', 'Марганец', 'мг'], ['dietary_sodium_total', 'Натрий', 'мг'],
];
/** Also understood (0.18): no daily norm in «Питание», but used for limits, sleep, caffeine, food and the personal effect. */
export const EXTRA_NUTRIENTS: [string, string, string][] = [
  ['dietary_biotin_total', 'Биотин (B7)', 'мкг'], ['dietary_pantothenic_acid_total', 'B5 (пантотеновая к-та)', 'мг'], ['dietary_iodine_total', 'Йод', 'мкг'],
  ['dietary_chromium_total', 'Хром', 'мкг'], ['choline', 'Холин', 'мг'], ['epa', 'Омега-3 EPA', 'мг'], ['dha', 'Омега-3 DHA', 'мг'],
  ['creatine', 'Креатин', 'мг'], ['melatonin', 'Мелатонин', 'мг'], ['dietary_caffeine_total', 'Кофеин', 'мг'],
  ['dietary_energy_total', 'Энергия', 'ккал'], ['dietary_protein_total', 'Белок', 'г'], ['dietary_carbohydrates_total', 'Углеводы', 'г'],
  ['dietary_fat_total_total', 'Жиры', 'г'], ['dietary_fiber_total', 'Клетчатка', 'г'], ['water_ml', 'Вода', 'мл'],
];
const ALL_NUT = [...NUTRIENTS, ...EXTRA_NUTRIENTS];
const nutName = (k: string) => ALL_NUT.find((n) => n[0] === k)?.[1] ?? k;
const nutUnit = (k: string) => ALL_NUT.find((n) => n[0] === k)?.[2] ?? '';
/** International units → the stored unit (vitamin D 40 МЕ = 1 мкг, A 1 МЕ = 0,3 мкг ретинола, E 1 МЕ ≈ 0,67 мг). */
const IU: Record<string, number> = { dietary_vitamin_d_total: 1 / 40, dietary_vitamin_a_total: 0.3, dietary_vitamin_e_total: 0.67 };
const FORMS = ['капсула', 'таблетка', 'порция порошка', 'капли', 'саше', 'мармеладка', 'порция', 'другое'];
const SCHED: [Med['schedule'], string][] = [['daily', 'каждый день'], ['asneeded', 'по необходимости'], ['course', 'курсом']];
const KINDS: [CatalogKind | '', string][] = [['', 'все'], ['vitamin', 'витамины и минералы'], ['protein', 'протеин'], ['isotonic', 'изотоники'], ['amino', 'аминокислоты, креатин'], ['other', 'другое']];
const num = (x: number) => (Math.round(x * 100) / 100).toString().replace('.', ',');
const today = () => new Date().toLocaleDateString('sv-SE');

/** Names on labels → our key (the backend recognises the same on save; here it is shown as a hint). */
const ALIASES: [RegExp, string][] = [
  [/холекальциферол|cholecalciferol|^d3\b|^д3\b|витамин d|витамин д/i, 'dietary_vitamin_d_total'],
  [/ретинол|retinol|бета-?каротин|витамин a|витамин а/i, 'dietary_vitamin_a_total'],
  [/аскорбин|ascorb|витамин c|витамин с/i, 'dietary_vitamin_c_total'],
  [/токоферол|tocopher|витамин e|витамин е/i, 'dietary_vitamin_e_total'],
  [/менахинон|филлохинон|^k2|^к2|витамин k|витамин к/i, 'dietary_vitamin_k_total'],
  [/кобаламин|cobalamin|b12|в12/i, 'dietary_vitamin_b12_total'],
  [/тиамин|thiamin|\bb1\b|\bв1\b/i, 'dietary_thiamin_total'],
  [/рибофлавин|riboflavin/i, 'dietary_riboflavin_total'],
  [/ниацин|никотинамид|niacin/i, 'dietary_niacin_total'],
  [/пиридокс|pyridox|\bb6\b|\bв6\b/i, 'dietary_vitamin_b6_total'],
  [/фол(ат|иев)|folate|folic/i, 'dietary_folate_total'],
  [/биотин|biotin/i, 'dietary_biotin_total'], [/пантотен|pantothen/i, 'dietary_pantothenic_acid_total'],
  [/^кальци|calcium/i, 'dietary_calcium_total'], [/желез|iron/i, 'dietary_iron_total'], [/магни|magnesium/i, 'dietary_magnesium_total'],
  [/цинк|zinc/i, 'dietary_zinc_total'], [/^кали[йя]|potassium/i, 'dietary_potassium_total'], [/селен|selen/i, 'dietary_selenium_total'],
  [/йод|iodine/i, 'dietary_iodine_total'], [/хром|chromium/i, 'dietary_chromium_total'], [/холин|choline/i, 'choline'],
  [/\bepa\b|эйкозапентаен|\bэпк\b/i, 'epa'], [/\bdha\b|докозагексаен|\bдгк\b/i, 'dha'], [/креатин|creatine/i, 'creatine'],
  [/мелатонин|melatonin/i, 'melatonin'], [/кофеин|caffeine/i, 'dietary_caffeine_total'],
];
const guessKey = (name: string) => ALIASES.find(([re]) => re.test(name.trim()))?.[1] ?? null;
const SALT = /цитрат|оксид|глицинат|пиколинат|карбонат|глюконат|сульфат|фумарат|лактат|хелат|аспарагинат|citrate|oxide|glycinate|picolinate|carbonate|gluconate|sulfate|fumarate|chelate/i;

type Row = { key: string; name: string; amount: string; unit: string };

function NutSelect({ value, onChange }: { value: string; onChange: (k: string) => void }) {
  return (
    <select className="select" value={value || 'other'} onChange={(e) => onChange(e.target.value)} style={{ flex: '1 1 180px' }}>
      <optgroup label="С нормой в «Питании»">{NUTRIENTS.map(([k, n]) => <option key={k} value={k}>{n}</option>)}</optgroup>
      <optgroup label="Ещё понимает Kouros">{EXTRA_NUTRIENTS.map(([k, n]) => <option key={k} value={k}>{n}</option>)}</optgroup>
      <option value="other">другое…</option>
    </select>
  );
}

function Editor({ med, check, onDone }: { med: Partial<Med> | null; check?: boolean; onDone: (saved: boolean) => void }) {
  const [kind, setKind] = useState<Med['kind']>(med?.kind ?? 'supplement');
  const [name, setName] = useState(med?.name ?? '');
  const [form, setForm] = useState(med?.form ?? 'капсула');
  const [dose, setDose] = useState(String(med?.dose ?? 1));
  const [sched, setSched] = useState<Med['schedule']>(med?.schedule ?? 'daily');
  const [from, setFrom] = useState(med?.courseFrom ?? '');
  const [to, setTo] = useState(med?.courseTo ?? '');
  const [notes, setNotes] = useState(med?.notes ?? '');
  const [extra, setExtra] = useState<MedExtra>(med?.extra ?? {});
  const [group, setGroup] = useState<MedGroup | null>(med?.groupInfo ?? null);
  const [rows, setRows] = useState<Row[]>(
    (med?.components ?? []).map((c) => ({ key: c.key, name: c.key ? nutName(c.key) : c.name, amount: String(c.amount), unit: c.unit })),
  );
  const [err, setErr] = useState<string | null>(null);
  const [gBusy, setGBusy] = useState(false);
  const setRow = (i: number, r: Partial<Row>) => setRows(rows.map((x, j) => (j === i ? { ...x, ...r } : x)));
  const foodish = extra.catalogKind === 'protein' || extra.catalogKind === 'isotonic' || rows.some((r) => ['dietary_energy_total', 'dietary_carbohydrates_total', 'dietary_protein_total'].includes(r.key));
  const findGroup = async () => {
    if (!extra.inn?.trim()) return;
    setGBusy(true);
    try {
      const r = await api.drugOnline(extra.inn.trim());
      if (r.group) { setGroup(r.group); setExtra({ ...extra, group: r.group.key }); } else setErr('В международной базе для этого вещества нет группы, важной для тренировок. Если сомневаешься — спроси Claude или врача.');
    } catch (e) { setErr(errText(e)); } finally { setGBusy(false); }
  };
  const save = async () => {
    const comps: MedComponent[] = [];
    for (const r of rows) {
      const a = Number(r.amount.replace(',', '.'));
      if (!r.amount.trim()) continue;
      if (!Number.isFinite(a) || a < 0) { setErr(`Количество для «${r.name || nutName(r.key)}» — число`); return; }
      if (r.key) {
        const base = nutUnit(r.key);
        const v = r.unit === 'МЕ' && IU[r.key] ? a * IU[r.key] : r.unit === 'мг' && base === 'мкг' ? a * 1000 : r.unit === 'мкг' && base === 'мг' ? a / 1000
          : r.unit === 'г' && base === 'мг' ? a * 1000 : r.unit === 'мг' && base === 'г' ? a / 1000 : a;
        comps.push({ key: r.key, name: nutName(r.key), amount: Math.round(v * 1000) / 1000, unit: base });
      } else if (r.name.trim()) {
        comps.push({ key: '', name: r.name.trim(), amount: a, unit: r.unit || 'мг' });
      }
    }
    const d = Number(dose.replace(',', '.'));
    try {
      await api.medSave({ id: med?.id, kind, name, form, dose: Number.isFinite(d) && d > 0 ? d : 1, schedule: sched, components: comps,
        courseFrom: sched === 'course' ? from || null : null, courseTo: sched === 'course' ? to || null : null, notes: notes || null, archived: med?.archived ?? false,
        extra: kind === 'medicine' ? extra : { ...extra, inn: undefined, group: undefined } });
      onDone(true);
    } catch (e) { setErr(errText(e)); }
  };
  return (
    <section className="card" aria-label="Препарат">
      <div className="between"><span className="label">{med?.id ? 'Изменить' : 'Добавить'}</span><button type="button" className="xbtn" aria-label="Закрыть" onClick={() => onDone(false)}>×</button></div>
      {check && (
        <div className="small" style={{ lineHeight: 1.5, padding: '10px 12px', borderRadius: 10, background: '#3DDC9714', border: '1px solid #3DDC9744' }}>
          <b>Всё совпадает с твоей банкой?</b> Сверь состав и порцию с этикеткой — у производителей бывают разные версии для разных стран. Если что-то отличается — поправь здесь, это и будет твоя версия.
          {extra.serving && <span className="muted"> Порция по данным каталога: {extra.serving}.</span>}
        </div>
      )}
      <div className="row" role="group" aria-label="Тип">
        <button type="button" className="pill" aria-pressed={kind === 'supplement'} onClick={() => setKind('supplement')}>Витамины и добавки</button>
        <button type="button" className="pill" aria-pressed={kind === 'medicine'} onClick={() => setKind('medicine')}>Лекарство</button>
      </div>
      <div className="row" style={{ gap: 12, flexWrap: 'wrap' }}>
        <label className="stack" style={{ gap: 4, flex: '2 1 240px' }}><span className="muted tiny">Название</span><input className="input" value={name} onChange={(e) => setName(e.target.value)} placeholder="Например, Omega-3" /></label>
        <label className="stack" style={{ gap: 4, flex: '1 1 150px' }}><span className="muted tiny">Форма</span>
          <select className="select" value={form} onChange={(e) => setForm(e.target.value)}>{(FORMS.includes(form) ? FORMS : [form, ...FORMS]).map((f) => <option key={f}>{f}</option>)}</select></label>
        <label className="stack" style={{ gap: 4, flex: '0 1 120px' }}><span className="muted tiny">Сколько за приём</span><input className="input" value={dose} onChange={(e) => setDose(e.target.value)} /></label>
      </div>
      {kind === 'medicine' && (
        <div className="stack" style={{ gap: 6 }}>
          <div className="row" style={{ gap: 8, flexWrap: 'wrap', alignItems: 'flex-end' }}>
            <label className="stack" style={{ gap: 4, flex: '2 1 240px' }}><span className="muted tiny">Действующее вещество (МНН) — есть на коробке под названием</span>
              <input className="input" value={extra.inn ?? ''} onChange={(e) => { setExtra({ ...extra, inn: e.target.value, group: null }); setGroup(null); }} placeholder="например, ибупрофен" /></label>
            <button type="button" className="btn2" disabled={gBusy || !extra.inn?.trim()} onClick={findGroup} title="По латинскому названию вещества, например ibuprofen">{gBusy ? 'Ищу…' : 'Найти группу в интернете'}</button>
          </div>
          {group ? (
            <span className="small" style={{ lineHeight: 1.5 }}><b>{group.name}.</b> <span className="muted">{group.note}</span></span>
          ) : (
            <span className="muted tiny" style={{ lineHeight: 1.5 }}>Группу Kouros определит сам при сохранении, если знает вещество. Онлайн-поиск понимает латинское название (ibuprofen, ciprofloxacin). Не нашлось — спроси Claude, к какой группе относится лекарство.</span>
          )}
        </div>
      )}
      <div className="row" role="group" aria-label="Как принимаешь" style={{ flexWrap: 'wrap' }}>
        {SCHED.map(([k, n]) => <button key={k} type="button" className="pill" aria-pressed={sched === k} onClick={() => setSched(k)}>{n}</button>)}
        {sched === 'course' && (
          <>
            <input type="date" className="select" value={from} onChange={(e) => setFrom(e.target.value)} aria-label="Курс с" />
            <span className="muted">—</span>
            <input type="date" className="select" value={to} onChange={(e) => setTo(e.target.value)} aria-label="Курс по" />
          </>
        )}
      </div>
      <span className="small" style={{ fontWeight: 700 }}>Состав в одной {form === 'другое' || form === 'порция' ? 'единице' : form === 'капли' ? 'порции капель' : form === 'порция порошка' ? 'порции' : form === 'саше' ? 'саше' : form === 'таблетка' ? 'таблетке' : form === 'мармеладка' ? 'мармеладке' : 'капсуле'}</span>
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>Перепиши с банки. Витамины D, A и E можно вводить в МЕ — Kouros пересчитает (D: 40 МЕ = 1 мкг; A: 1 МЕ = 0,3 мкг; E: 1 МЕ ≈ 0,67 мг). Чего нет в списке — через «другое»: Kouros попробует узнать название («холекальциферол» → витамин D), а остальное передаст Claude.</span>
      {rows.map((r, i) => {
        const g = !r.key && r.name.trim() ? guessKey(r.name) : null;
        const salt = r.name && SALT.test(r.name);
        return (
          <div key={i} className="stack" style={{ gap: 4 }}>
            <div className="row" style={{ gap: 8, flexWrap: 'wrap' }}>
              <NutSelect value={r.key} onChange={(k) => setRow(i, k === 'other' ? { key: '', unit: 'мг' } : { key: k, name: nutName(k), unit: nutUnit(k) })} />
              {!r.key && <input className="input" style={{ flex: '1 1 140px' }} placeholder="например, пиколинат цинка" value={r.name} onChange={(e) => setRow(i, { name: e.target.value })} />}
              <input className="input" style={{ width: 100, flex: 'none' }} placeholder="0" value={r.amount} onChange={(e) => setRow(i, { amount: e.target.value })} />
              <select className="select" style={{ width: 90, flex: 'none' }} value={r.unit} onChange={(e) => setRow(i, { unit: e.target.value })}>
                {(r.key ? [nutUnit(r.key), ...(IU[r.key] ? ['МЕ'] : []), ...(nutUnit(r.key) === 'мкг' ? ['мг'] : nutUnit(r.key) === 'мг' ? ['мкг', 'г'] : nutUnit(r.key) === 'г' ? ['мг'] : [])] : ['мг', 'мкг', 'г', 'МЕ', 'мл']).map((u) => <option key={u}>{u}</option>)}
              </select>
              <button type="button" className="xbtn" aria-label="Убрать" onClick={() => setRows(rows.filter((_, j) => j !== i))}>×</button>
            </div>
            {g && (
              <span className="tiny" style={{ color: '#3DDC97' }}>
                Похоже на «{nutName(g)}». <button type="button" className="linkbtn tiny" onClick={() => setRow(i, { key: g, unit: r.unit === 'МЕ' && IU[g] ? 'МЕ' : nutUnit(g) === 'мкг' && r.unit === 'мг' ? 'мг' : nutUnit(g) })}>Так и записать</button>
              </span>
            )}
            {salt && (
              <span className="muted tiny" style={{ lineHeight: 1.5 }}>На банке часто стоит масса соли («цитрат магния 500 мг»), а самого элемента в ней меньше — для магния в цитрате около 16%. Вводи элементарное количество: обычно оно указано отдельно или в скобках.</span>
            )}
          </div>
        );
      })}
      <button type="button" className="btn2" style={{ alignSelf: 'flex-start' }} onClick={() => setRows([...rows, { key: 'dietary_vitamin_d_total', name: 'Витамин D', amount: '', unit: 'мкг' }])}>+ Компонент</button>
      {kind === 'supplement' && foodish && (
        <div className="stack" style={{ gap: 6 }}>
          <label className="row small" style={{ gap: 8, alignItems: 'center' }}>
            <input type="checkbox" checked={extra.countFood ?? extra.catalogKind === 'isotonic'} onChange={(e) => setExtra({ ...extra, countFood: e.target.checked })} />
            Учитывать в питании (калории, БЖУ, вода)
          </label>
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>Выключи, если записываешь это в YAZIO — иначе посчитается дважды. Изотоники обычно не записывают, поэтому у них включено; протеин — выключено.</span>
          {(extra.countFood ?? extra.catalogKind === 'isotonic') && (
            <label className="row small" style={{ gap: 8, alignItems: 'center' }}>
              Разводишь в <input className="input" style={{ width: 90 }} value={extra.waterMl ?? ''} onChange={(e) => setExtra({ ...extra, waterMl: Number(e.target.value.replace(',', '.')) || null })} /> мл воды — это тоже вода дня
            </label>
          )}
        </div>
      )}
      <label className="stack" style={{ gap: 4 }}><span className="muted tiny">Заметка (зачем, кто назначил)</span><input className="input" value={notes} onChange={(e) => setNotes(e.target.value)} /></label>
      {err && <div className="note-err">{err}</div>}
      <div className="row"><button type="button" className="btn" onClick={save} disabled={!name.trim()}>Сохранить</button><button type="button" className="btn2" onClick={() => onDone(false)}>Отмена</button></div>
    </section>
  );
}

/** How to add: catalog by brand, online, medicine registry, Claude's label line, or by hand. */
function Picker({ onPick, onClose }: { onPick: (m: Partial<Med> | null, check: boolean) => void; onClose: () => void }) {
  const [tab, setTab] = useState<'catalog' | 'online' | 'medicine' | 'claude'>('catalog');
  const [brands, setBrands] = useState<CatalogBrands | null>(null);
  const [brand, setBrand] = useState('');
  const [kind, setKind] = useState<CatalogKind | ''>('');
  const [q, setQ] = useState('');
  const [list, setList] = useState<CatalogItem[]>([]);
  const [online, setOnline] = useState<OnlineItem[] | null>(null);
  const [drugs, setDrugs] = useState<DrugHit[] | null>(null);
  const [paste, setPaste] = useState('');
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => { api.catalogBrands().then(setBrands).catch(() => {}); }, []);
  useEffect(() => {
    if (tab !== 'catalog') return;
    const t = setTimeout(() => { api.catalogSearch(brand || null, kind || null, q).then(setList).catch((e) => setErr(errText(e))); }, 150);
    return () => clearTimeout(t);
  }, [tab, brand, kind, q]);
  useEffect(() => {
    if (tab !== 'medicine' || q.trim().length < 2) { setDrugs(null); return; }
    const t = setTimeout(() => { api.drugSearch(q).then(setDrugs).catch((e) => setErr(errText(e))); }, 200);
    return () => clearTimeout(t);
  }, [tab, q]);
  const pick = async (id: string) => {
    const p = await api.catalogProduct(id).catch(() => null);
    if (p) onPick(p, true);
  };
  const searchOnline = async () => {
    setBusy(true); setErr(null);
    try { setOnline(await api.catalogOnline(q)); } catch (e) { setErr(errText(e)); } finally { setBusy(false); }
  };
  const fromLabel = async () => {
    setErr(null);
    try { onPick(await api.catalogLabel(paste), true); } catch (e) { setErr(errText(e)); }
  };
  const pickDrug = (d: DrugHit) => onPick({
    kind: 'medicine', name: d.trade, form: /капс/i.test(d.form) ? 'капсула' : /табл/i.test(d.form) ? 'таблетка' : 'другое', dose: 1, schedule: 'asneeded', components: [],
    notes: [d.form, d.dose].filter(Boolean).join(', ') || null, extra: { inn: d.inn, group: d.group?.key ?? null, source: d.source }, groupInfo: d.group,
  }, false);
  const brandName = (k: string) => brands?.brands.find((b) => b.key === k)?.name ?? k;
  return (
    <section className="card" aria-label="Добавить препарат">
      <div className="between"><span className="label">Добавить</span><button type="button" className="xbtn" aria-label="Закрыть" onClick={onClose}>×</button></div>
      <div className="row" role="tablist" style={{ flexWrap: 'wrap' }}>
        {([['catalog', 'Каталог'], ['online', 'Найти в интернете'], ['medicine', 'Лекарство'], ['claude', 'Фото этикетки → Claude']] as const).map(([k, n]) => (
          <button key={k} type="button" role="tab" className="pill" aria-pressed={tab === k} onClick={() => { setTab(k); setErr(null); }}>{n}</button>
        ))}
        <button type="button" className="linkbtn small" onClick={() => onPick(null, false)}>Вручную →</button>
      </div>
      {tab !== 'claude' && (
        <input className="input" autoFocus value={q} onChange={(e) => setQ(e.target.value)}
          placeholder={tab === 'medicine' ? 'Название с коробки или действующее вещество' : tab === 'online' ? 'Бренд и название, например Thorne Magnesium' : 'Поиск по названию'}
          onKeyDown={(e) => { if (e.key === 'Enter' && tab === 'online') searchOnline(); }} />
      )}
      {tab === 'catalog' && (
        <>
          <div className="row" role="group" aria-label="Бренд" style={{ flexWrap: 'wrap' }}>
            <button type="button" className="pill" aria-pressed={!brand} onClick={() => setBrand('')}>Все бренды</button>
            {brands?.brands.map((b) => <button key={b.key} type="button" className="pill" aria-pressed={brand === b.key} onClick={() => setBrand(b.key)}>Добавки {b.name} <span className="muted">{b.count}</span></button>)}
          </div>
          <div className="row" role="group" aria-label="Вид" style={{ flexWrap: 'wrap', gap: 6 }}>
            {KINDS.map(([k, n]) => <button key={k || 'all'} type="button" className="pill small" aria-pressed={kind === k} onClick={() => setKind(k)}>{n}</button>)}
          </div>
          <div className="stack" style={{ gap: 0, maxHeight: 420, overflowY: 'auto' }}>
            {list.map((p) => (
              <button key={p.id} type="button" className="list-row" style={{ textAlign: 'left', background: 'none', border: 'none', color: 'inherit', cursor: 'pointer', width: '100%' }} onClick={() => pick(p.id)}>
                <span className="stack" style={{ gap: 2, flex: 1, minWidth: 0 }}>
                  <b className="small">{brandName(p.brand)} · {p.name}</b>
                  <span className="muted tiny">{p.serving ? `порция: ${p.serving} · ` : ''}{p.preview}{p.count > 4 ? ` и ещё ${p.count - 4}` : ''}</span>
                </span>
                <span className="muted small">→</span>
              </button>
            ))}
            {!list.length && <span className="muted small" style={{ lineHeight: 1.5 }}>Ничего не нашлось. Попробуй вкладку «Найти в интернете» или «Фото этикетки → Claude».</span>}
          </div>
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>Maxler — с официального сайта (регион Европа). NOW и Solgar — из базы этикеток NIH (американские версии, у нас состав может немного отличаться). {brands?.version ? `Каталог от ${brands.version}; обновляется с новыми версиями Kouros.` : ''}</span>
        </>
      )}
      {tab === 'online' && (
        <>
          <div className="row"><button type="button" className="btn" disabled={busy || q.trim().length < 3} onClick={searchOnline}>{busy ? 'Ищу…' : 'Искать'}</button></div>
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>Поиск в базе этикеток NIH (США, больше 200 тысяч добавок). В интернет уходит только текст запроса — ничего о тебе.</span>
          {online && !online.length && <span className="muted small">Ничего не нашлось. Остаётся «Фото этикетки → Claude» или вручную.</span>}
          {online?.map((o) => (
            <button key={o.id} type="button" className="list-row" style={{ textAlign: 'left', background: 'none', border: 'none', color: 'inherit', cursor: 'pointer', width: '100%' }} onClick={() => onPick(o.prep, true)}>
              <span className="stack" style={{ gap: 2, flex: 1 }}>
                <b className="small">{o.brand} · {o.name}{o.offMarket ? <span className="muted" style={{ fontWeight: 400 }}> · снят с продажи</span> : null}</b>
                <span className="muted tiny">порция: {o.serving} · {(o.prep.components ?? []).slice(0, 4).map((c) => c.name).join(', ')}</span>
              </span>
              <span className="muted small">→</span>
            </button>
          ))}
        </>
      )}
      {tab === 'medicine' && (
        <>
          {drugs?.map((d, i) => (
            <button key={i} type="button" className="list-row" style={{ textAlign: 'left', background: 'none', border: 'none', color: 'inherit', cursor: 'pointer', width: '100%' }} onClick={() => pickDrug(d)}>
              <span className="stack" style={{ gap: 2, flex: 1 }}>
                <b className="small">{d.trade} <span className="muted" style={{ fontWeight: 400 }}>· {d.inn}</span></b>
                <span className="muted tiny">{[d.form, d.dose].filter(Boolean).join(' · ')}{d.group ? ` · ${d.group.name}` : ''}</span>
              </span>
              <span className="muted small">→</span>
            </button>
          ))}
          {drugs && !drugs.length && (
            <span className="muted small" style={{ lineHeight: 1.5 }}>
              В реестре не нашлось. Посмотри на коробке действующее вещество (МНН, мелко под названием) и поищи по нему — или <button type="button" className="linkbtn small" onClick={() => onPick({ kind: 'medicine', name: q, schedule: 'asneeded', components: [], extra: {} }, false)}>добавь «{q}» вручную</button> и впиши МНН.
            </span>
          )}
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>Реестр лекарств РФ (grls.rosminzdrav.ru) встроен и обновляется кнопкой внизу раздела. Kouros отмечает группы, важные для тренировок, — без диагнозов и без советов по дозам.</span>
        </>
      )}
      {tab === 'claude' && (
        <>
          <span className="small" style={{ lineHeight: 1.6 }}>
            1. Сфотографируй этикетку с составом и отправь фото в свой проект Claude «Тренер Kouros» с просьбой: <i>«Перепиши состав добавки строкой [ДОБАВКА]»</i>.<br />
            2. Claude ответит строкой вида <code>[ДОБАВКА] Название | порция: 2 таблетки | Витамин D3 25 мкг; Цинк 15 мг</code>.<br />
            3. Вставь ответ сюда — Kouros разберёт состав и покажет для проверки.
          </span>
          <textarea className="input" rows={4} value={paste} onChange={(e) => setPaste(e.target.value)} placeholder="[ДОБАВКА] …" />
          <div className="row"><button type="button" className="btn" disabled={!paste.trim()} onClick={fromLabel}>Разобрать</button></div>
        </>
      )}
      {err && <div className="note-err">{err}</div>}
    </section>
  );
}

const effLine = (e: MedEffect, what: string) => {
  const parts: string[] = [];
  if (e.hrvPct != null) parts.push(`HRV ${e.hrvPct > 0 ? '+' : ''}${num(e.hrvPct)}%`);
  if (e.rhr != null) parts.push(`пульс покоя ${e.rhr > 0 ? '+' : ''}${num(e.rhr)}`);
  if (e.sleepMin != null) parts.push(`сон ${e.sleepMin > 0 ? '+' : ''}${e.sleepMin} мин`);
  if (e.ready != null) parts.push(`готовность ${e.ready > 0 ? '+' : ''}${e.ready}`);
  return parts.length ? `${what}: ${parts.join(', ')} (${e.with} и ${e.without} дн.)` : null;
};

/** One preparation with the «принял» button and 14 days of marks. */
export function MedRow({ m, onChanged, onEdit, compact }: { m: Med; onChanged: () => void; onEdit?: () => void; compact?: boolean }) {
  const taken = (m.today ?? 0) > 0;
  const [busy, setBusy] = useState(false);
  const [open, setOpen] = useState(false);
  const toggle = async () => {
    setBusy(true);
    try { await api.medTake(m.id, today(), taken ? 0 : m.dose); onChanged(); } finally { setBusy(false); }
  };
  const setTime = async (t: string) => { if (t) { await api.medTake(m.id, today(), m.today ?? m.dose, t); onChanged(); } };
  const comp = m.components.slice(0, 4).map((c) => `${c.name} ${num(c.amount * m.dose)} ${c.unit}`).join(' · ');
  const eff = [m.effect?.nextMorning && effLine(m.effect.nextMorning, 'Утро после приёма vs без'), m.effect?.course && effLine(m.effect.course, '2 недели курса vs 2 недели до')].filter(Boolean) as string[];
  return (
    <div className="stack" style={{ gap: 4 }}>
      <div className="list-row" style={{ alignItems: 'center', gap: 12 }}>
        <button type="button" className={taken ? 'btn' : 'btn2'} style={{ minWidth: 112 }} disabled={busy} onClick={toggle} aria-pressed={taken}>{taken ? '✓ Принял' : 'Принять'}</button>
        <span className="stack" style={{ gap: 2, flex: 1, minWidth: 0 }}>
          <b className="small">{m.name} <span className="muted" style={{ fontWeight: 400 }}>· {num(m.dose)} {m.form ?? ''}{m.kind === 'medicine' ? ' · лекарство' : ''}</span></b>
          {!compact && m.groupInfo && <span className="tiny" style={{ color: '#F5B642' }}>{m.groupInfo.name}{m.extra?.inn ? ` · ${m.extra.inn}` : ''}</span>}
          {!compact && comp && <span className="muted tiny">{comp}{m.components.length > 4 ? ' …' : ''}</span>}
          {!compact && m.schedule === 'course' && <span className="muted tiny">курс {m.courseFrom ?? '…'} — {m.courseTo ?? '…'}</span>}
        </span>
        {taken && (
          <label className="row tiny muted" style={{ gap: 4, alignItems: 'center' }} title="Когда принял — для подсказок о сочетаниях">
            в <input type="time" className="select" style={{ width: 92, padding: '2px 6px' }} defaultValue={m.todayTime ?? ''} onBlur={(e) => { if (e.target.value !== (m.todayTime ?? '')) setTime(e.target.value); }} />
          </label>
        )}
        {!compact && m.days && m.days.length > 0 && (
          <span className="row" style={{ gap: 3 }} aria-label="14 дней">
            {m.days.map(([d, v]) => <span key={d} title={d} style={{ width: 7, height: 14, borderRadius: 2, background: v > 0 ? '#3DDC97' : '#22262D' }} />)}
          </span>
        )}
        {!compact && ((m.tips?.length ?? 0) > 0 || eff.length > 0) && <button type="button" className="linkbtn small" aria-expanded={open} onClick={() => setOpen(!open)}>{open ? 'Скрыть' : 'Как принимать'}</button>}
        {onEdit && <button type="button" className="linkbtn small" onClick={onEdit}>Изменить</button>}
      </div>
      {open && !compact && (
        <div className="stack tiny" style={{ gap: 4, padding: '0 0 8px 124px', lineHeight: 1.5 }}>
          {m.tips?.map((t, i) => <span key={i}>• {t}</span>)}
          {eff.map((t, i) => <span key={`e${i}`} style={{ color: '#7FB7FF' }}>{t}. Это совпадение во времени, а не доказательство.</span>)}
          {!eff.length && <span className="muted">Личный эффект появится, когда наберётся хотя бы 5 утр с приёмом и 5 без (или 2 недели курса).</span>}
        </div>
      )}
    </div>
  );
}

/** Medicines in use and timing of intake. */
export function MedNotes({ s, todayOnly }: { s: MedsSummary; todayOnly?: boolean }) {
  const notes = s.notes ?? [];
  const timing = (s.timing ?? []).filter((t) => !todayOnly || t.today);
  if (!notes.length && !timing.length) return null;
  return (
    <>
      {notes.map((n, i) => (
        <span key={i} className="small" style={{ lineHeight: 1.5, color: n.level === 'warn' ? '#F5B642' : undefined }}>{n.level === 'warn' ? '⚠ ' : ''}{n.text}</span>
      ))}
      {timing.map((t) => (
        <span key={t.code} className="small" style={{ lineHeight: 1.5 }}><b>{t.title}.</b> {t.text}{!todayOnly && t.days > 1 ? <span className="muted"> ({t.days} дн. из 7)</span> : null}</span>
      ))}
    </>
  );
}

function Registry() {
  const [r, setR] = useState<DrugRegistry | null>(null);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const load = () => { api.drugRegistryInfo().then(setR).catch(() => {}); };
  useEffect(load, []);
  if (!r) return null;
  const update = async () => {
    setBusy(true); setMsg(null);
    try { const n = await api.drugRegistryUpdate(); setMsg(`Готово: ${n.toLocaleString('ru-RU')} записей.`); load(); } catch (e) { setMsg(errText(e)); } finally { setBusy(false); }
  };
  return (
    <section className="card" aria-label="Справочники">
      <span className="label">Справочники</span>
      <span className="small" style={{ lineHeight: 1.5 }}>
        Реестр лекарств РФ: {r.local ? `${r.local.toLocaleString('ru-RU')} записей, ${r.localVersion ?? ''}` : `встроенный, ${r.builtin.toLocaleString('ru-RU')} записей${r.builtinVersion ? ` (${r.builtinVersion})` : ''}`}.
      </span>
      <div className="row" style={{ gap: 12, flexWrap: 'wrap' }}>
        <button type="button" className="btn2" disabled={busy} onClick={update}>{busy ? 'Загружаю с grls.rosminzdrav.ru…' : 'Обновить реестр'}</button>
        <label className="row small" style={{ gap: 8, alignItems: 'center' }}>
          <input type="checkbox" checked={r.online} onChange={async (e) => { await api.suppOnlineSet(e.target.checked); load(); }} />
          Искать добавки и лекарства в интернете (NIH)
        </label>
      </div>
      {msg && <span className="small">{msg}</span>}
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>В интернет уходит только то, что ты ищешь, или запрос на скачивание реестра. Твои данные остаются на компьютере.</span>
    </section>
  );
}

export function Meds() {
  const [s, setS] = useState<MedsSummary | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [edit, setEdit] = useState<{ med: Partial<Med> | null; check: boolean } | undefined>(undefined);
  const [picking, setPicking] = useState(false);
  const [showArch, setShowArch] = useState(false);
  const load = useCallback(() => { api.medsSummary().then((v) => { setS(v); setErr(null); }).catch((e) => setErr(errText(e))); }, []);
  useEffect(load, [load]);
  useDataChanged(load);
  if (err) return <div className="note-err">{err}</div>;
  if (!s) return <div className="muted">Загружаю…</div>;
  const active = s.items.filter((m) => m.active);
  const rest = s.items.filter((m) => !m.active);
  const archive = async (m: Med, v: boolean) => { await api.medSave({ ...m, archived: v }); load(); };
  const del = async (m: Med) => { if (confirm(`Удалить «${m.name}» вместе с отметками приёма?`)) { await api.medDelete(m.id); setEdit(undefined); load(); } };
  const hasNotes = (s.notes?.length ?? 0) + (s.timing?.length ?? 0) > 0;
  return (
    <>
      {s.warnings.length > 0 && (
        <section className="card" aria-label="Верхние пределы" style={{ borderColor: '#F5B64255' }}>
          <span className="label" style={{ color: '#F5B642' }}>Выше безопасного предела</span>
          {s.warnings.map((w) => (
            <span key={w.key} className="small" style={{ lineHeight: 1.5 }}>
              <b>{nutName(w.key)}</b>: в среднем {num(w.avg)} {nutUnit(w.key)} в день за неделю (из добавок {num(w.supp)}) при верхнем пределе {num(w.limit)}{w.note ? ` (${w.note})` : ''}.
            </span>
          ))}
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>Верхний предел — количество, которое большинству людей безопасно получать каждый день долго. Превышение не значит «опасно сразу», но это повод проверить дозировку и обсудить её с врачом.</span>
        </section>
      )}
      {hasNotes && (
        <section className="card" aria-label="Обрати внимание">
          <span className="label">Обрати внимание</span>
          <MedNotes s={s} />
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>Подсказки по времени приёма — по моментам, когда ты нажал «Принять» (время можно поправить), и по дневнику еды. Про лекарства — только «обрати внимание» и «обсуди с врачом»: дозу и схему меняет врач.</span>
        </section>
      )}
      {picking ? (
        <Picker onClose={() => setPicking(false)} onPick={(m, check) => { setPicking(false); setEdit({ med: m, check }); }} />
      ) : edit !== undefined ? (
        <Editor key={JSON.stringify(edit.med?.extra?.source ?? edit.med?.id ?? 'new')} med={edit.med} check={edit.check} onDone={(saved) => { setEdit(undefined); if (saved) load(); }} />
      ) : (
        <section className="card" aria-label="Сегодня">
          <div className="between">
            <span className="label">Сегодня</span>
            <button type="button" className="btn2" onClick={() => setPicking(true)}>+ Добавить</button>
          </div>
          {active.length ? active.map((m) => <MedRow key={m.id} m={m} onChanged={load} onEdit={() => setEdit({ med: m, check: false })} />) : (
            <span className="muted small" style={{ lineHeight: 1.5 }}>Пока пусто. Добавь витамины и лекарства, которые принимаешь: найди банку в каталоге (Maxler, NOW, Solgar), в интернете или перепиши состав — дальше нужно будет только отмечать приём.</span>
          )}
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>Kouros учитывает только то, что ты отметил здесь. Добавки, записанные в YAZIO, не считаются — чтобы не было путаницы с единицами и пропущенными приёмами. Витамины и минералы из добавок попадают в «Питание» отдельной частью, кофеин — в кофеин дня, изотоник (если включено) — в калории, углеводы и воду.</span>
        </section>
      )}
      {(s.unknown?.length ?? 0) > 0 && edit === undefined && !picking && (
        <span className="muted tiny" style={{ lineHeight: 1.5, padding: '0 4px' }}>Kouros не знает норм для: {s.unknown!.join(', ')}. Они передаются Claude в «Спросить Claude» и участвуют в личном эффекте.</span>
      )}
      {rest.length > 0 && (
        <section className="card" aria-label="Не сейчас">
          <button type="button" className="linkbtn small" style={{ alignSelf: 'flex-start' }} onClick={() => setShowArch(!showArch)}>{showArch ? 'Скрыть' : `Курс закончен или в архиве (${rest.length})`}</button>
          {showArch && rest.map((m) => (
            <div key={m.id} className="list-row small" style={{ alignItems: 'center' }}>
              <span style={{ flex: 1 }}>{m.name}{m.archived ? ' · в архиве' : ' · курс не сейчас'}</span>
              {m.archived ? <button type="button" className="linkbtn small" onClick={() => archive(m, false)}>Вернуть</button> : <button type="button" className="linkbtn small" onClick={() => setEdit({ med: m, check: false })}>Изменить</button>}
              <button type="button" className="linkbtn small" onClick={() => del(m)}>Удалить</button>
            </div>
          ))}
        </section>
      )}
      {active.length > 0 && edit === undefined && !picking && (
        <section className="card" aria-label="Управление">
          <span className="label">Убрать из списка</span>
          {active.map((m) => (
            <div key={m.id} className="list-row small" style={{ alignItems: 'center' }}>
              <span style={{ flex: 1 }}>{m.name}</span>
              <button type="button" className="linkbtn small" onClick={() => archive(m, true)}>В архив</button>
              <button type="button" className="linkbtn small" onClick={() => del(m)}>Удалить</button>
            </div>
          ))}
          <span className="muted tiny">Архив сохраняет историю приёма; удаление стирает и препарат, и все отметки.</span>
        </section>
      )}
      <Registry />
    </>
  );
}
