// «Тренер», part 1: onboarding, «Как мой день?», week analysis, «Спросить Claude» (copy / paste back),
// history and the coach's notes. Reports are written by Kouros from computed numbers.
import { useCallback, useEffect, useState } from 'react';
import { CoachPlan } from './CoachPlan';
import { CoachBody, CoachGoal } from './CoachBody';
import { api, errText, type CoachProfile, type CoachReport, type CoachSection, type CoachState, type EngineSummary } from '../lib/api';

type Sub = 'today' | 'plan' | 'body' | 'goal' | 'week' | 'profile' | 'history';
const SUBS: [Sub, string][] = [['today', 'Сегодня у тренера'], ['plan', 'План'], ['body', 'Боль и восстановление'], ['goal', 'Цель'], ['week', 'Анализ недели'], ['profile', 'Профиль'], ['history', 'История']];

const GOALS: [string, string][] = [['hm', 'Полумарафон быстрее'], ['mar', 'Первый марафон'], ['k10', 'Улучшить 5 и 10 км'], ['body', 'Снизить вес и улучшить состав тела'], ['health', 'Здоровье и общая форма'], ['str', 'Стать сильнее'], ['inj', 'Бегать без травм']];
const MODES: [string, string, string][] = [['build', 'Набор объёма', 'Постепенно растим километраж и добавляем качество'], ['maintain', 'Поддержание формы', 'Держим текущий объём, без роста'], ['winter', 'Зимовка', 'Меньше бега, больше силовой и ОФП до весны'], ['return', 'Возвращение', 'После перерыва, болезни или травмы']];
const LIKES: [string, string, string][] = [['run', 'Бег', '#3DDC97'], ['walk', 'Ходьба', '#9DB8FF'], ['gym', 'Силовые в зале', '#F5B642'], ['ofp', 'Укрепляющие упражнения', '#A78BFA'], ['bike', 'Велосипед', '#5AC8FA']];
const WD = ['Пн', 'Вт', 'Ср', 'Чт', 'Пт', 'Сб', 'Вс'];
const DUR = ['30 мин', '45 мин', '60 мин', '90 мин и больше'];
const WHEN = ['Утро', 'День', 'Вечер', 'По-разному'];
const PLACE = ['Улица', 'Беговая дорожка', 'Зал рядом', 'Дома'];
const INJ = ['Колени', 'Ахилл', 'Голень', 'Стопа', 'Спина', 'Бедро', 'Нет травм'];
const TOOLS: [string, string][] = [['Сауна', 'прогрев, расслабление'], ['Горячая ванна', 'сон и расслабление'], ['Перкуссионный массажёр', 'точечно по мышцам'], ['Роллер', 'миофасциальный релиз'], ['Лимфодренажный массажёр для ног', 'отёки и тяжесть в ногах'], ['Мази', 'локальная боль'], ['Растяжка', 'мобильность'], ['Компрессионная одежда', 'после бега и в дорогу'], ['Холодный душ или ванна', 'бодрость, отёки']];
const LEVEL: Record<string, string> = { ok: '#3DDC97', info: '#9DB8FF', warn: '#F5B642', bad: '#FF6B6B' };
const PHASE: Record<string, string> = { morning: 'утро', day: 'день', evening: 'вечер', night: 'ночь' };

const tog = (a: string[] | undefined, v: string) => (a ?? []).includes(v) ? (a ?? []).filter((x) => x !== v) : [...(a ?? []), v];
const ruDate = (d: string, y = false) => new Date(`${d}T12:00:00`).toLocaleDateString('ru-RU', { day: 'numeric', month: 'short', ...(y ? { year: 'numeric' } : {}) });
const ruTs = (t: number) => new Date(t * 1000).toLocaleString('ru-RU', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
function period(from: string, to: string) {
  const days = Math.round((Date.parse(to) - Date.parse(from)) / 86400000) + 1;
  const same = from.slice(0, 7) === to.slice(0, 7);
  const a = same ? String(Number(from.slice(8))) : ruDate(from);
  return `с ${a} по ${ruDate(to)}, ${days} ${days % 10 === 1 && days % 100 !== 11 ? 'день' : days % 10 >= 2 && days % 10 <= 4 && (days % 100 < 10 || days % 100 >= 20) ? 'дня' : 'дней'}`;
}

function profileSummary(p: CoachProfile): string {
  const goals = (p.goals ?? []).map((g) => GOALS.find((x) => x[0] === g)?.[1].toLowerCase()).filter(Boolean);
  const mode = MODES.find((m) => m[0] === p.mode);
  const week = Object.entries(p.week ?? {}).filter(([, v]) => v > 0).map(([k, v]) => `${LIKES.find((l) => l[0] === k)?.[1].toLowerCase() ?? k} ${v}`);
  const parts: string[] = [];
  if (goals.length) parts.push(`Главное для тебя — ${goals.join(', ')}.`);
  if (mode) parts.push(`Сейчас режим «${mode[1].toLowerCase()}».`);
  if (week.length) parts.push(`В обычную неделю: ${week.join(', ')} (дней).`);
  if (p.days?.length) parts.push(`Тренироваться можешь: ${p.days.join(', ')}${p.time ? `, по ${p.time}` : ''}${p.when ? `, удобнее — ${p.when.toLowerCase()}` : ''}.`);
  if (p.where?.length) parts.push(`Где: ${p.where.join(', ').toLowerCase()}.`);
  const inj = (p.injuries ?? []).filter((x) => x !== 'Нет травм');
  parts.push(inj.length ? `Буду бережнее с: ${inj.join(', ').toLowerCase()}.` : 'Травм сейчас нет.');
  if (p.tools?.length) parts.push(`Для восстановления есть: ${p.tools.join(', ').toLowerCase()} — буду предлагать их по ситуации.`);
  return parts.join(' ');
}

function knownFacts(s: EngineSummary | null): { k: string; v: string; src: string }[] {
  if (!s) return [];
  const out: { k: string; v: string; src: string }[] = [];
  const w = s.weeks.slice(-8);
  if (w.length) {
    const km = w.reduce((a, x) => a + x.runKm, 0) / w.length;
    out.push({ k: 'Бег в неделю', v: `${Math.round(km)} км`, src: 'Strava, 8 недель' });
    out.push({ k: 'Тренировок в неделю', v: String(Math.round(w.reduce((a, x) => a + x.count, 0) / w.length)), src: 'Strava, 8 недель' });
  }
  out.push({ k: 'Пульс макс / покоя', v: `${s.profile.hrMax} / ${Math.round(s.hrRestUsed)}`, src: 'профиль и Apple Health' });
  if (s.sleep.length) out.push({ k: 'Сон в среднем', v: (() => { const m = Math.round(s.sleep.reduce((a, x) => a + x.totalS, 0) / s.sleep.length / 60); return `${Math.floor(m / 60)} ч ${String(m % 60).padStart(2, '0')} мин`; })(), src: 'Apple Health, 2 недели' });
  if (s.body.weight.smooth != null) out.push({ k: 'Вес', v: `${s.body.weight.smooth.toFixed(1).replace('.', ',')} кг`, src: 'Apple Health' });
  if (s.day.usualReadiness != null) out.push({ k: 'Обычная готовность', v: String(Math.round(s.day.usualReadiness)), src: 'Kouros, 30 дней' });
  return out;
}

// ---------- onboarding ----------

function Chips({ items, sel, onPick }: { items: string[]; sel: string[]; onPick: (v: string) => void }) {
  return <div className="row" style={{ gap: 8, flexWrap: 'wrap' }}>{items.map((x) => <button key={x} type="button" className="pill" aria-pressed={sel.includes(x)} onClick={() => onPick(x)}>{x}</button>)}</div>;
}

function Onboarding({ init, facts, onDone, onSkip }: { init: CoachProfile; facts: { k: string; v: string; src: string }[]; onDone: (p: CoachProfile) => void; onSkip: () => void }) {
  const [p, setP] = useState<CoachProfile>(init);
  const [step, setStep] = useState(0);
  const set = (x: Partial<CoachProfile>) => setP((o) => ({ ...o, ...x }));
  const titles = ['Что для тебя главное?', 'Что ты любишь делать?', 'Сколько времени есть?', 'Травмы и восстановление', 'Что я понял'];
  const wk = p.week ?? {};
  return (
    <section className="card" aria-label="Знакомство с тренером" style={{ gap: 20 }}>
      <div className="stack" style={{ gap: 6 }}>
        <span className="label">Знакомство · шаг {step + 1} из 5</span>
        <h2 className="coach-title">{titles[step]}</h2>
        {step === 0 && <span className="muted small">Пять коротких шагов: цели, что любишь, сколько времени есть, чем восстанавливаешься. Остальное я уже вижу в Strava и Apple Health.</span>}
      </div>
      {step === 0 && (
        <div className="stack" style={{ gap: 16 }}>
          <div className="stack" style={{ gap: 8 }}><b className="small">Цели — можно несколько</b>
            <div className="row" style={{ gap: 8, flexWrap: 'wrap' }}>{GOALS.map(([k, n]) => <button key={k} type="button" className="pill" aria-pressed={(p.goals ?? []).includes(k)} onClick={() => set({ goals: tog(p.goals, k) })}>{n}</button>)}</div>
          </div>
          <div className="stack" style={{ gap: 8 }}><b className="small">Режим на ближайшие месяцы</b>
            <div className="opts">{MODES.map(([k, n, d]) => (
              <button key={k} type="button" className="opt-card" aria-pressed={p.mode === k} onClick={() => set({ mode: k })}>
                <b className="small">{p.mode === k ? '✓ ' : ''}{n}</b><span className="muted tiny">{d}</span>
              </button>
            ))}</div>
          </div>
        </div>
      )}
      {step === 1 && (
        <div className="stack" style={{ gap: 10 }}>
          {LIKES.map(([k, n, c]) => (
            <div key={k} className="between inset" style={{ flexDirection: 'row', padding: '10px 14px' }}>
              <span className="row" style={{ gap: 8 }}><span className="dot" style={{ background: c }} /><b className="small">{n}</b><span className="muted tiny">дней в неделю</span></span>
              <span className="row" style={{ gap: 10 }}>
                <button type="button" className="btn2" style={{ width: 36, padding: 0 }} aria-label="Меньше" onClick={() => set({ week: { ...wk, [k]: Math.max(0, (wk[k] ?? 0) - 1) } })}>−</button>
                <span className="num" style={{ minWidth: 20, textAlign: 'center', fontSize: 18, fontWeight: 700 }}>{wk[k] ?? 0}</span>
                <button type="button" className="btn2" style={{ width: 36, padding: 0 }} aria-label="Больше" onClick={() => set({ week: { ...wk, [k]: Math.min(7, (wk[k] ?? 0) + 1) } })}>+</button>
              </span>
            </div>
          ))}
          <span className="muted small" style={{ lineHeight: 1.5 }}>Это шаблон обычной недели — позже его можно будет менять на конкретную неделю. Ходьба идёт каждый день и всегда считается в нагрузку.</span>
        </div>
      )}
      {step === 2 && (
        <div className="stack" style={{ gap: 16 }}>
          <div className="stack" style={{ gap: 8 }}><b className="small">В какие дни можешь тренироваться</b><Chips items={WD} sel={p.days ?? []} onPick={(v) => set({ days: WD.filter((d) => tog(p.days, v).includes(d)) })} /></div>
          <div className="stack" style={{ gap: 8 }}><b className="small">Сколько времени на тренировку</b><Chips items={DUR} sel={p.time ? [p.time] : []} onPick={(v) => set({ time: v })} /></div>
          <div className="stack" style={{ gap: 8 }}><b className="small">Когда удобнее</b><Chips items={WHEN} sel={p.when ? [p.when] : []} onPick={(v) => set({ when: v })} /></div>
          <div className="stack" style={{ gap: 8 }}><b className="small">Где можешь тренироваться</b><Chips items={PLACE} sel={p.where ?? []} onPick={(v) => set({ where: tog(p.where, v) })} /></div>
        </div>
      )}
      {step === 3 && (
        <div className="stack" style={{ gap: 16 }}>
          <div className="stack" style={{ gap: 8 }}><b className="small">Травмы и слабые места — сейчас или часто бывают</b>
            <Chips items={INJ} sel={p.injuries ?? []} onPick={(v) => set({ injuries: v === 'Нет травм' ? ['Нет травм'] : tog((p.injuries ?? []).filter((x) => x !== 'Нет травм'), v) })} />
          </div>
          <div className="stack" style={{ gap: 8 }}><b className="small">Чем можешь восстанавливаться</b>
            <div className="opts">{TOOLS.map(([n, d]) => (
              <button key={n} type="button" className="opt-card" aria-pressed={(p.tools ?? []).includes(n)} onClick={() => set({ tools: tog(p.tools, n) })}>
                <b className="small">{(p.tools ?? []).includes(n) ? '✓ ' : ''}{n}</b><span className="muted tiny">{d}</span>
              </button>
            ))}</div>
          </div>
          <div className="stack" style={{ gap: 8 }}><b className="small">Что ещё стоит знать тренеру (необязательно)</b>
            <textarea className="textarea" rows={2} value={p.other ?? ''} onChange={(e) => set({ other: e.target.value })} placeholder="Например: по средам поздно с работы, летом на даче без зала" />
          </div>
        </div>
      )}
      {step === 4 && (
        <div className="stack" style={{ gap: 12 }}>
          {facts.length > 0 && <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))', gap: 10 }}>
            {facts.map((f) => <div key={f.k} className="inset" style={{ gap: 4 }}><span className="muted tiny" style={{ fontWeight: 600 }}>{f.k}</span><span style={{ fontSize: 16, fontWeight: 700 }}>{f.v}</span><span className="muted tiny">{f.src}</span></div>)}
          </div>}
          <div className="inset small" style={{ lineHeight: 1.55, display: 'block' }}><b>Что я понял: </b>{profileSummary(p)}</div>
        </div>
      )}
      <div className="between" style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 12 }}>
        <button type="button" className="linkbtn small" onClick={onSkip}>Пропустить, заполню потом</button>
        <span className="row" style={{ gap: 10 }}>
          {step > 0 && <button type="button" className="btn2" onClick={() => setStep(step - 1)}>Назад</button>}
          <button type="button" className="btn" onClick={() => step < 4 ? setStep(step + 1) : onDone({ ...p, done: true })}>{step < 4 ? 'Дальше' : 'Всё верно'}</button>
        </span>
      </div>
    </section>
  );
}

// ---------- reports ----------

function Section({ x, short }: { x: CoachSection; short: boolean }) {
  const [open, setOpen] = useState(false);
  const deviation = x.level === 'warn' || x.level === 'bad';
  return (
    <div className="stack" style={{ gap: 4, padding: '10px 0', borderTop: '1px solid #1c2026' }}>
      <div className="row" style={{ gap: 10, alignItems: 'baseline' }}>
        <span className="dot" style={{ background: LEVEL[x.level], flex: '0 0 8px', transform: 'translateY(-1px)' }} />
        <span className="small" style={{ lineHeight: 1.5 }}><b>{x.title}.</b> {x.text}</span>
      </div>
      {x.why && (short && !open
        ? <button type="button" className="linkbtn tiny" style={{ alignSelf: 'flex-start', marginLeft: 18 }} onClick={() => setOpen(true)}>почему это важно</button>
        : <span className="muted small" style={{ marginLeft: 18, lineHeight: 1.5 }}>{x.why}</span>)}
      {x.todo && <span className="small" style={{ marginLeft: 18, lineHeight: 1.5, color: deviation ? '#F1D9A8' : '#9DB8FF' }}>→ {x.todo}</span>}
    </div>
  );
}

function AskClaude({ reportId, onSaved, label }: { reportId: number | null; onSaved: (r: CoachReport) => void; label?: string }) {
  const [state, setState] = useState<'idle' | 'copied' | 'manual'>('idle');
  const [text, setText] = useState('');
  const [answer, setAnswer] = useState('');
  const [q, setQ] = useState('');
  const [err, setErr] = useState<string | null>(null);
  const copy = async () => {
    setErr(null);
    try {
      const t = await api.coachPackage(reportId, reportId == null ? q : undefined);
      setText(t);
      try { await navigator.clipboard.writeText(t); setState('copied'); } catch { setState('manual'); }
    } catch (e) { setErr(errText(e)); }
  };
  const save = async () => {
    setErr(null);
    try { const r = await api.coachSaveAnswer(reportId, answer); setAnswer(''); setState('idle'); onSaved(r); } catch (e) { setErr(errText(e)); }
  };
  return (
    <div className="stack" style={{ gap: 10 }}>
      {reportId == null && <textarea className="textarea" rows={3} value={q} onChange={(e) => setQ(e.target.value)} placeholder="Например: можно ли завтра сделать длительную, если сегодня болят икры?" />}
      <div className="row" style={{ gap: 10, flexWrap: 'wrap' }}>
        <button type="button" className="btn2" onClick={copy} disabled={reportId == null && !q.trim()}>{label ?? 'Спросить Claude'}</button>
        {state === 'copied' && <span className="small" style={{ color: '#3DDC97' }}>Скопировано. Вставь в чат проекта «Тренер Kouros» в Claude.</span>}
      </div>
      {state === 'manual' && <textarea className="textarea mono tiny" rows={6} readOnly value={text} onFocus={(e) => e.currentTarget.select()} />}
      {state !== 'idle' && (
        <div className="stack" style={{ gap: 8 }}>
          <span className="muted small">Когда Claude ответит — скопируй ответ и вставь сюда, он сохранится в истории.</span>
          <textarea className="textarea" rows={5} value={answer} onChange={(e) => setAnswer(e.target.value)} placeholder="Ответ Claude" />
          <div className="row" style={{ gap: 10 }}>
            <button type="button" className="btn" onClick={save} disabled={!answer.trim()}>Сохранить ответ</button>
            <button type="button" className="linkbtn small" onClick={() => { setState('idle'); setAnswer(''); }}>Отмена</button>
          </div>
        </div>
      )}
      {err && <div className="note-err">{err}</div>}
    </div>
  );
}

function ClaudeAnswer({ r, onDelete }: { r: CoachReport; onDelete?: () => void }) {
  return (
    <div className="inset" style={{ gap: 6, borderColor: '#2f3a52' }}>
      <div className="between"><span className="label" style={{ color: '#9DB8FF' }}>Ответ Claude · {ruTs(r.ts)}</span>{onDelete && <button type="button" className="linkbtn tiny" onClick={onDelete}>удалить</button>}</div>
      <div className="small" style={{ whiteSpace: 'pre-wrap', lineHeight: 1.55 }}>{r.body.text}</div>
      {(r.body.plan ?? 0) > 0 && <span className="small" style={{ color: '#F5B642' }}>Из ответа взято предложений по плану: {r.body.plan} — они ждут решения во вкладке «План».</span>}
    </div>
  );
}

function DayReport({ r, short, answers, onSaved, onDelete }: { r: CoachReport; short: boolean; answers: CoachReport[]; onSaved: (r: CoachReport) => void; onDelete: (id: number) => void }) {
  const secs = r.body.sections ?? [];
  const order = (x: CoachSection) => ({ bad: 0, warn: 1, info: 2, ok: 3 }[x.level]);
  const sorted = [...secs].sort((a, b) => order(a) - order(b));
  return (
    <section className="card" aria-label={r.title} style={{ gap: 12 }}>
      <div className="between"><span className="label">{r.title} · {PHASE[r.body.phase ?? 'day']}</span><button type="button" className="linkbtn tiny" onClick={() => onDelete(r.id)}>удалить</button></div>
      <h3 style={{ margin: 0, fontFamily: 'var(--display)', fontWeight: 500, fontSize: 20, lineHeight: 1.35 }}>{r.body.headline}</h3>
      <div className="stack" style={{ gap: 0 }}>{sorted.map((x) => <Section key={x.key + x.text} x={x} short={short} />)}</div>
      {answers.map((a) => <ClaudeAnswer key={a.id} r={a} onDelete={() => onDelete(a.id)} />)}
      <AskClaude reportId={r.id} onSaved={onSaved} />
    </section>
  );
}

function WeekReport({ r, answers, onSaved, onDelete }: { r: CoachReport; answers: CoachReport[]; onSaved: (r: CoachReport) => void; onDelete: (id: number) => void }) {
  const b = r.body;
  return (
    <section className="card" aria-label={r.title} style={{ gap: 14 }}>
      <div className="between"><span className="label">Анализ · {b.from && b.to ? period(b.from, b.to) : ''}</span><button type="button" className="linkbtn tiny" onClick={() => onDelete(r.id)}>удалить</button></div>
      <h3 style={{ margin: 0, fontFamily: 'var(--display)', fontWeight: 500, fontSize: 20, lineHeight: 1.35 }}>{b.headline}</h3>
      <span className="muted tiny">Средние за день и нагрузка, приведённая к 7 дням — чтобы честно сравнивать с обычной неделей.</span>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))', gap: 10 }}>
        {(b.lines ?? []).map((l) => <div key={l.k} className="inset" style={{ gap: 2 }}><span className="muted tiny" style={{ fontWeight: 600 }}>{l.k}</span><b>{l.v}</b>{l.note && <span className="muted tiny">{l.note}</span>}</div>)}
      </div>
      {(b.follow ?? []).length > 0 && (
        <div className="stack" style={{ gap: 6 }}>
          <b className="small">Что было в прошлый раз</b>
          {b.follow!.map((f) => <span key={f.text} className="small" style={{ lineHeight: 1.5 }}><span style={{ color: f.done ? '#3DDC97' : '#F5B642', fontWeight: 700 }}>{f.done ? '✓ получилось' : '• ещё в работе'}</span> — {f.text}</span>)}
        </div>
      )}
      <div className="stack" style={{ gap: 6 }}>
        <b className="small">{(b.recs ?? []).length ? 'Что поменять' : 'Что поменять — ничего, так держать'}</b>
        {(b.recs ?? []).map((x, i) => <span key={x.code} className="small" style={{ lineHeight: 1.5 }}><b style={{ color: '#9DB8FF' }}>{i + 1}.</b> {x.text}</span>)}
      </div>
      {(b.notes ?? []).map((n) => <span key={n} className="muted tiny">{n}</span>)}
      {answers.map((a) => <ClaudeAnswer key={a.id} r={a} onDelete={() => onDelete(a.id)} />)}
      <AskClaude reportId={r.id} onSaved={onSaved} label="Обсудить с Claude" />
    </section>
  );
}

function ReportView(props: { r: CoachReport; all: CoachReport[]; short: boolean; onSaved: (r: CoachReport) => void; onDelete: (id: number) => void }) {
  const answers = props.all.filter((a) => a.kind === 'claude' && a.parent === props.r.id).sort((a, b) => a.ts - b.ts);
  if (props.r.kind === 'week') return <WeekReport r={props.r} answers={answers} onSaved={props.onSaved} onDelete={props.onDelete} />;
  if (props.r.kind === 'claude') return <section className="card"><ClaudeAnswer r={props.r} onDelete={() => props.onDelete(props.r.id)} /></section>;
  return <DayReport r={props.r} short={props.short} answers={answers} onSaved={props.onSaved} onDelete={props.onDelete} />;
}

// ---------- page ----------

export function Coach() {
  const [st, setSt] = useState<CoachState | null>(null);
  const [eng, setEng] = useState<EngineSummary | null>(null);
  const [sub, setSub] = useState<Sub>('today');
  const [onb, setOnb] = useState(false);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [open, setOpen] = useState<number | null>(null);
  const [notes, setNotes] = useState('');
  const [notesSaved, setNotesSaved] = useState(false);
  const [rulesShown, setRulesShown] = useState(false);
  const [rulesCopied, setRulesCopied] = useState(false);

  const load = useCallback(() => {
    api.coachState().then((v) => { setSt(v); setNotes(v.notes); if (!v.profile) setOnb(true); }).catch((e) => setErr(errText(e)));
  }, []);
  useEffect(load, [load]);
  useEffect(() => { api.engineSummary().then(setEng).catch(() => {}); }, []);

  if (err && !st) return <div className="note-err">{err}</div>;
  if (!st) return <p className="muted">Загружаю…</p>;
  const facts = knownFacts(eng);
  const profile = st.profile ?? {};
  const short = profile.style === 'short';
  const saveProfile = async (p: CoachProfile) => { await api.coachProfileSave(p).catch((e) => setErr(errText(e))); setOnb(false); load(); };

  if (onb) return <Onboarding init={profile} facts={facts} onDone={saveProfile} onSkip={() => { if (!st.profile) saveProfile({ done: false }); else setOnb(false); }} />;

  const run = async (f: () => Promise<CoachReport>) => {
    setBusy(true); setErr(null);
    try { const r = await f(); setOpen(r.id); load(); } catch (e) { setErr(errText(e)); } finally { setBusy(false); }
  };
  const del = async (id: number) => { await api.coachDelete(id).catch(() => {}); if (open === id) setOpen(null); load(); };
  const saved = () => load();
  const lastDay = st.reports.find((r) => r.kind === 'day');
  const lastWeek = st.reports.find((r) => r.kind === 'week');
  const openR = open != null ? st.reports.find((r) => r.id === open) : null;
  const nextDate = st.week.nextTs ? new Date(st.week.nextTs * 1000).toLocaleDateString('ru-RU', { weekday: 'long', day: 'numeric', month: 'long' }) : null;
  const freeAnswers = st.reports.filter((r) => r.kind === 'claude' && r.parent == null);

  return (
    <div className="stack" style={{ gap: 16 }}>
      <nav className="subtabs" aria-label="Разделы тренера">
        {SUBS.map(([id, n]) => <button key={id} type="button" aria-pressed={sub === id} onClick={() => { setSub(id); setOpen(null); }}>{n}</button>)}
      </nav>
      {err && <div className="note-err">{err}</div>}

      {sub === 'today' && (
        <>
          <section className="card coach" aria-label="Как мой день">
            <div className="stack" style={{ gap: 10, flex: '999 1 420px' }}>
              <span className="label">Тренер Kouros</span>
              <h2 className="coach-title">Как мой день?</h2>
              <span className="muted small" style={{ lineHeight: 1.5 }}>Коротко разберу день на эту минуту: сон, готовность, тренировку, стресс, еду и воду, добавки. Что в норме — одной строкой, подробно — только о том, что выбивается.</span>
            </div>
            <div className="stack" style={{ gap: 8, justifyContent: 'center' }}>
              <button type="button" className="btn btn-green" onClick={() => run(api.coachDay)} disabled={busy}>{busy ? 'Считаю…' : 'Как мой день?'}</button>
              {lastDay && <span className="muted tiny">прошлый — {ruTs(lastDay.ts)}</span>}
            </div>
          </section>
          {(openR && openR.kind === 'day' ? openR : lastDay) && <ReportView r={(openR && openR.kind === 'day' ? openR : lastDay)!} all={st.reports} short={short} onSaved={saved} onDelete={del} />}
          <section className="card" aria-label="Свой вопрос">
            <span className="label">Свой вопрос тренеру Claude</span>
            <span className="muted small" style={{ lineHeight: 1.5 }}>Kouros соберёт твой профиль, заметки и цифры дня в один текст и скопирует — останется вставить его в чат Claude.</span>
            <AskClaude reportId={null} onSaved={saved} label="Скопировать для Claude" />
            {freeAnswers.slice(0, 2).map((a) => <ClaudeAnswer key={a.id} r={a} onDelete={() => del(a.id)} />)}
          </section>
        </>
      )}

      {sub === 'plan' && <CoachPlan />}
      {sub === 'body' && <CoachBody />}
      {sub === 'goal' && <CoachGoal profile={profile} onSave={saveProfile} />}

      {sub === 'week' && (
        <>
          <section className="card coach" aria-label="Анализ недели">
            <div className="stack" style={{ gap: 10, flex: '999 1 420px' }}>
              <span className="label">{st.week.ready ? `Период: ${period(st.week.from, st.week.to)}` : 'Анализ недели'}</span>
              <h2 className="coach-title">Анализ недели</h2>
              <span className="muted small" style={{ lineHeight: 1.5 }}>Сравню средние за период с твоей обычной неделей, проверю прошлые советы и скажу, что поменять. Период — от прошлого анализа до сегодня, но не больше 4 недель.</span>
            </div>
            <div className="stack" style={{ gap: 8, justifyContent: 'center' }}>
              <button type="button" className="btn btn-green" onClick={() => run(api.coachWeek)} disabled={busy || !st.week.ready}>{busy ? 'Считаю…' : 'Сделать анализ'}</button>
              {!st.week.ready && nextDate && <span className="muted tiny">Следующий анализ — с {nextDate}</span>}
            </div>
          </section>
          {(openR && openR.kind === 'week' ? openR : lastWeek)
            ? <ReportView r={(openR && openR.kind === 'week' ? openR : lastWeek)!} all={st.reports} short={short} onSaved={saved} onDelete={del} />
            : <p className="muted small">Анализов пока не было. Первый охватит последние 7 дней.</p>}
        </>
      )}

      {sub === 'profile' && (
        <div className="grid2">
          <div className="stack" style={{ gap: 16 }}>
            <section className="card" aria-label="Знакомство">
              <div className="between"><span className="label">Что тренер знает обо мне</span><button type="button" className="linkbtn small" onClick={() => setOnb(true)}>{profile.done ? 'Пройти знакомство заново' : 'Пройти знакомство'}</button></div>
              <div className="small" style={{ lineHeight: 1.6 }}>{profile.done ? profileSummary(profile) : 'Знакомство ещё не пройдено — отчёты будут без учёта целей и травм.'}</div>
              {profile.other && <span className="small muted">Ещё: {profile.other}</span>}
            </section>
            <section className="card" aria-label="Заметки">
              <span className="label">Заметки для тренера</span>
              <span className="muted small" style={{ lineHeight: 1.5 }}>Всё, что важно помнить: самочувствие, планы, старты, работа, что сказал врач. Уходит вместе с вопросом к Claude.</span>
              <textarea className="textarea" rows={6} value={notes} onChange={(e) => { setNotes(e.target.value); setNotesSaved(false); }} placeholder="Например: 9 ноября забег 10 км; правое колено ноет на спусках" />
              <div className="row" style={{ gap: 10 }}>
                <button type="button" className="btn" onClick={() => api.coachNotesSave(notes).then(() => { setNotesSaved(true); load(); }).catch((e) => setErr(errText(e)))}>Сохранить</button>
                {notesSaved && <span className="small" style={{ color: '#3DDC97' }}>Сохранено</span>}
              </div>
            </section>
          </div>
          <div className="stack" style={{ gap: 16 }}>
            <section className="card" aria-label="Из данных">
              <span className="label">Что тренер знает из данных</span>
              {facts.map((f) => <div key={f.k} className="list-row small"><span>{f.k}<span className="muted tiny"> · {f.src}</span></span><b className="num">{f.v}</b></div>)}
            </section>
            <section className="card" aria-label="Как общается">
              <span className="label">Как тренер со мной общается</span>
              <div className="row" style={{ gap: 8 }}>
                <span className="small" style={{ minWidth: 100 }}>Объяснения</span>
                <button type="button" className="pill" aria-pressed={short} onClick={() => saveProfile({ ...profile, style: 'short' })}>Коротко</button>
                <button type="button" className="pill" aria-pressed={!short} onClick={() => saveProfile({ ...profile, style: 'long' })}>Подробно</button>
              </div>
              <span className="muted tiny">Коротко — «почему это важно» прячется под ссылку.</span>
              <div className="row" style={{ gap: 8 }}>
                <span className="small" style={{ minWidth: 100 }}>Изменения в плане</span>
                <button type="button" className="pill" aria-pressed={profile.planChanges !== 'auto'} onClick={() => saveProfile({ ...profile, planChanges: 'ask' })}>Спросить</button>
                <button type="button" className="pill" aria-pressed={profile.planChanges === 'auto'} onClick={() => saveProfile({ ...profile, planChanges: 'auto' })}>Сразу</button>
              </div>
              <span className="muted tiny">«Спросить» — Kouros предлагает замену и ждёт твоего «Принять». «Сразу» — меняет сам и пишет в историю.</span>
            </section>
            <section className="card" aria-label="Claude">
              <span className="label">Тренер Claude · настройка один раз</span>
              <span className="small" style={{ lineHeight: 1.55 }}>
                В Claude создай проект «Тренер Kouros» и вставь в его инструкции текст ниже. Дальше кнопки «Спросить Claude» копируют сводку — её нужно вставить в новый чат этого проекта.
              </span>
              <div className="row" style={{ gap: 10 }}>
                <button type="button" className="btn2" onClick={() => navigator.clipboard.writeText(st.rules).then(() => setRulesCopied(true)).catch(() => setRulesShown(true))}>Скопировать инструкции</button>
                <button type="button" className="linkbtn small" onClick={() => setRulesShown(!rulesShown)}>{rulesShown ? 'скрыть' : 'показать текст'}</button>
                {rulesCopied && <span className="small" style={{ color: '#3DDC97' }}>Скопировано</span>}
              </div>
              {rulesShown && <textarea className="textarea tiny" rows={10} readOnly value={st.rules} onFocus={(e) => e.currentTarget.select()} />}
            </section>
          </div>
        </div>
      )}

      {sub === 'history' && (
        openR ? (
          <>
            <button type="button" className="backbtn" onClick={() => setOpen(null)}>← История</button>
            <ReportView r={openR} all={st.reports} short={short} onSaved={saved} onDelete={del} />
          </>
        ) : (
          <section className="card" aria-label="История отчётов">
            <span className="label">История отчётов</span>
            {st.reports.filter((r) => r.kind !== 'claude' || r.parent == null).length === 0 && <span className="muted small">Пока пусто — нажми «Как мой день?» или «Анализ недели».</span>}
            {st.reports.filter((r) => r.kind !== 'claude' || r.parent == null).map((r) => {
              const n = st.reports.filter((a) => a.parent === r.id).length;
              return (
                <button key={r.id} type="button" className="list-row plain small" onClick={() => setOpen(r.id)} style={{ textAlign: 'left' }}>
                  <span className="chip" style={{ background: '#1a1f26', color: r.kind === 'week' ? '#A78BFA' : r.kind === 'claude' ? '#9DB8FF' : '#3DDC97' }}>{r.kind === 'week' ? 'неделя' : r.kind === 'claude' ? 'Claude' : 'день'}</span>
                  <span style={{ flex: 1, minWidth: 0 }} className="ellipsis">{r.kind === 'week' && r.from && r.to ? period(r.from, r.to) : r.body.headline ?? r.body.text?.slice(0, 120)}</span>
                  {n > 0 && <span className="muted tiny">+ ответ Claude</span>}
                  <span className="muted tiny num">{ruTs(r.ts)}</span>
                </button>
              );
            })}
          </section>
        )
      )}
    </div>
  );
}
