import { useCallback, useEffect, useMemo, useState } from 'react';
import { api, errText, plural, sportOf, type PsychSummary, type SurveyKind, type SurveyRow } from '../lib/api';
import { useDataChanged } from '../lib/events';

const dayLabel = (d: string) => new Date(d + 'T12:00:00').toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' });

const HOOPER: [string, string, string][] = [
  ['Сон этой ночью', 'отлично выспался', 'очень плохо'],
  ['Стресс', 'совсем нет', 'очень сильный'],
  ['Усталость', 'бодр', 'вымотан'],
  ['Мышцы', 'ничего не болит', 'сильно болят'],
];

/** BRUMS: four words per scale, stored in scale order; shown interleaved. */
const BRUMS: [string, string, string[]][] = [
  ['tension', 'Напряжение', ['в панике', 'тревожно', 'беспокойно', 'нервничаю']],
  ['depression', 'Подавленность', ['подавлен', 'в унынии', 'несчастлив', 'тоскливо']],
  ['anger', 'Злость', ['раздражён', 'обижен', 'сердит', 'вспыльчив']],
  ['vigour', 'Бодрость', ['полон сил', 'энергичен', 'активен', 'собран']],
  ['fatigue', 'Усталость', ['вымотан', 'без сил', 'сонлив', 'устал']],
  ['confusion', 'Растерянность', ['растерян', 'сбит с толку', 'туман в голове', 'неуверен']],
];
const BRUMS_ORDER = Array.from({ length: 24 }, (_, k) => (k % 6) * 4 + Math.floor(k / 6));
const BRUMS_SCALE = ['совсем нет', 'немного', 'умеренно', 'сильно', 'очень сильно'];

const STRESSREC: string[] = [
  'Чувствовал давление — работа, дела, обязательства',
  'Был раздражён, напряжён или в конфликте',
  'Тело было уставшим и тяжёлым',
  'Было трудно расслабиться и переключиться',
  'Хорошо спал и просыпался отдохнувшим',
  'Было время на отдых, хобби, друзей и близких',
  'Был доволен, в хорошем настроении',
  'Тело ощущалось свежим и готовым к нагрузке',
];
const SR_SCALE = ['никогда', 'редко', 'иногда', 'часто', 'очень часто', 'почти всегда', 'всегда'];

const TQR: Record<number, string> = { 6: 'совсем не восстановился', 7: 'крайне плохо', 9: 'очень плохо', 11: 'плохо', 13: 'средне', 15: 'хорошо', 17: 'очень хорошо', 19: 'крайне хорошо', 20: 'полностью' };

const feelOf = (r: SurveyRow | undefined) => (r?.extra as { feel?: number } | null)?.feel ?? null;
const colorOf = (v: number | null) => (v == null ? '#9AA3AE' : v >= 67 ? '#3DDC97' : v >= 40 ? '#F5B642' : '#FF6B6B');

export function Psychology({ onOpenWorkout }: { onOpenWorkout?: (id: string) => void }) {
  const [p, setP] = useState<PsychSummary | null>(null);
  const [ready, setReady] = useState<[string, number | null][]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [open, setOpen] = useState<SurveyKind | null>(null);
  const load = useCallback(() => {
    api.psychSummary().then((v) => { setP(v); setErr(null); }).catch((e) => setErr(errText(e)));
    api.engineSummary().then((s) => setReady(s.readiness.history)).catch(() => {});
  }, []);
  useEffect(load, [load]);
  useDataChanged(load);

  async function save(kind: SurveyKind, answers: number[]) {
    try { await api.surveySave(kind, answers); setOpen(null); load(); } catch (e) { setErr(errText(e)); }
  }

  if (!p) return <>{err ? <div className="note-err">{err}</div> : <div className="muted">Загружаю…</div>}</>;
  const today = p.today;
  const hToday = p.hooper.find((x) => x.date === today);
  const feel = feelOf(hToday);
  const rToday = ready.find((x) => x[0] === today)?.[1] ?? null;
  const diff = feel != null && rToday != null ? feel - rToday : null;
  const brums = p.brums[p.brums.length - 1];
  const sr = p.stressrec[p.stressrec.length - 1];
  return (
    <>
      <div className="page-head">
        <div className="stack">
          <h1>Психология</h1>
          <p className="subtitle">Как голова справляется с нагрузкой — и совпадает ли это с тем, что показывает тело</p>
        </div>
      </div>
      {err && <div className="note-err">{err}</div>}

      <div className="rings">
        <section className="card" aria-label="Самочувствие">
          <span className="label">Самочувствие сегодня</span>
          <span className="num" style={{ fontSize: 34, fontWeight: 700, color: colorOf(feel) }}>{feel ?? '—'}<span className="muted" style={{ fontSize: 15 }}> из 100</span></span>
          <span className="muted small">{hToday ? `Утренний опрос · индекс Хупера ${hToday.score} из 28` : 'Утренний опрос ещё не пройден'}</span>
        </section>
        <section className="card" aria-label="Тело и ощущения">
          <span className="label">Тело и ощущения</span>
          <span className="num" style={{ fontSize: 34, fontWeight: 700 }}>{feel ?? '—'} <span className="muted" style={{ fontSize: 16 }}>vs</span> {rToday ?? '—'}</span>
          <span className="small" style={{ color: diff == null ? 'var(--muted)' : Math.abs(diff) < 20 ? '#3DDC97' : '#F5B642' }}>
            {diff == null ? 'Самочувствие по опросу против готовности по часам' : Math.abs(diff) < 20 ? 'Совпадают' : diff < 0 ? 'Ощущения хуже, чем показывают часы' : 'Ощущения лучше, чем показывают часы'}
          </span>
        </section>
        <section className="card" aria-label="Настроение недели">
          <span className="label">Настроение недели</span>
          {brums ? <><span style={{ fontFamily: 'var(--display)', fontSize: 22, fontWeight: 700, color: iceberg(brums).color }}>{iceberg(brums).title}</span><span className="muted small">Профиль BRUMS от {dayLabel(brums.date)}</span></> : <span className="muted small">Пройди профиль настроения — 2–3 минуты раз в неделю.</span>}
        </section>
      </div>

      <div className="grid2">
        <section className="card" aria-label="Утренний опрос">
          <div className="between"><span className="label">Утренний опрос · индекс Хупера</span><span className="muted tiny">30 секунд, каждое утро</span></div>
          {open === 'hooper' || !hToday
            ? <ScaleForm items={HOOPER} min={1} max={7} initial={hToday?.answers} onSave={(a) => save('hooper', a)} onCancel={hToday ? () => setOpen(null) : undefined} />
            : (
              <>
                <div>
                  {HOOPER.map(([n, lo, hi], i) => (
                    <div key={n} className="list-row small">
                      <span style={{ flex: 1 }}>{n}</span>
                      <span className="muted tiny">{hToday.answers[i] <= 2 ? lo : hToday.answers[i] >= 6 ? hi : ''}</span>
                      <b className="num" style={{ color: hToday.answers[i] >= 5 ? '#F5B642' : undefined }}>{hToday.answers[i]} / 7</b>
                    </div>
                  ))}
                </div>
                <button type="button" className="btn2" style={{ alignSelf: 'flex-start' }} onClick={() => setOpen('hooper')}>Изменить ответы</button>
              </>
            )}
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>1 — отлично, 7 — очень плохо. Сумма 4–28: чем меньше, тем лучше. Самочувствие = насколько ты далёк от 28, в процентах.</span>
        </section>
        <section className="card" aria-label="14 дней">
          <div className="between"><span className="label">14 дней · ощущения и тело</span>
            <span className="row tiny" style={{ gap: 12 }}><span className="row" style={{ gap: 4 }}><span className="dot" style={{ background: '#C792EA' }} />самочувствие</span><span className="row" style={{ gap: 4 }}><span className="dot" style={{ background: '#3DDC97' }} />готовность</span></span></div>
          <DualChart today={today} hooper={p.hooper} ready={ready} />
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>Жёлтая полоса — дни, когда ощущения и часы расходятся на 20+ пунктов. Если часы показывают хорошее восстановление, а самочувствие хуже — обычно это стресс вне спорта: работа, недосып, эмоции. В такие дни лучше снизить интенсивность, даже если HRV в норме.</span>
        </section>
      </div>

      <div className="grid2">
        <section className="card" aria-label="Профиль настроения">
          <div className="between"><span className="label">Профиль настроения · BRUMS</span><span className="muted tiny">шкала 0–16</span></div>
          {open === 'brums'
            ? <BrumsForm onSave={(a) => save('brums', a)} onCancel={() => setOpen(null)} />
            : (
              <>
                {brums ? <Iceberg row={brums} prev={p.brums.length > 1 ? p.brums[p.brums.length - 2] : undefined} /> : <span className="muted small">Ещё не пройден.</span>}
                <button type="button" className="btn2" style={{ alignSelf: 'flex-start' }} onClick={() => setOpen('brums')}>{brums ? 'Пройти заново' : 'Пройти · 2–3 минуты'}</button>
              </>
            )}
        </section>
        <div className="stack" style={{ gap: 20 }}>
          <section className="card" aria-label="Перед тренировкой">
            <div className="between"><span className="label">Перед тренировкой · TQR</span><span className="muted tiny">5 секунд</span></div>
            <span className="small" style={{ color: 'var(--text-2)' }}>Насколько ты восстановился к тренировке?</span>
            <div className="tqr" role="group" aria-label="Восстановление 6–20">
              {Array.from({ length: 15 }, (_, k) => k + 6).map((v) => {
                const last = p.tqr.find((x) => x.date === today);
                return <button key={v} type="button" aria-pressed={last?.score === v} title={TQR[v] ?? ''} onClick={() => save('tqr', [v])}>{v}</button>;
              })}
            </div>
            <div className="between tiny muted"><span>6 · совсем не восстановился</span><span>13 · средне</span><span>20 · полностью</span></div>
            {(() => { const last = p.tqr.find((x) => x.date === today); return last ? <span className="small" style={{ color: 'var(--green)' }}>Сегодня: {last.score} — {TQR[last.score] ?? TQR[last.score - 1] ?? ''}</span> : null; })()}
          </section>
          {p.unrated.length > 0 && (
            <section className="card" aria-label="Оценить тренировки">
              <span className="label">Оцени тяжесть тренировок</span>
              {p.unrated.map((a) => {
                const sp = sportOf(a.sport);
                return (
                  <button key={a.id} type="button" className="list-row plain small" onClick={() => onOpenWorkout?.(a.id)}>
                    <span className="badge" style={{ color: sp.color }}>{sp.letter}</span>
                    <span style={{ flex: 1 }}>{a.name || sp.name} <span className="muted">· {new Date(a.startUtc * 1000).toLocaleString('ru-RU', { weekday: 'short', hour: '2-digit', minute: '2-digit' })}</span></span>
                    <span className="linkbtn small">Оценить →</span>
                  </button>
                );
              })}
              <span className="muted tiny">Оценка 1–10 уточняет нагрузку силовых и физиотерапии и помогает тренеру.</span>
            </section>
          )}
        </div>
      </div>

      <section className="card" aria-label="Стресс и восстановление">
        <div className="between"><span className="label">Баланс стресса и восстановления</span><span className="muted tiny">раз в 2 недели · 1 минута</span></div>
        {open === 'stressrec'
          ? <ScaleForm items={STRESSREC.map((q) => [q, SR_SCALE[0], SR_SCALE[6]])} min={0} max={6} labels={SR_SCALE} hint="Как часто за последние 3 дня…" onSave={(a) => save('stressrec', a)} onCancel={() => setOpen(null)} />
          : (
            <>
              {sr ? <StressRec row={sr} /> : <span className="muted small">Короткая анкета Kouros: 4 вопроса о стрессе и 4 — о восстановлении. Показывает, хватает ли отдыха на всё, что на тебя давит, — в спорте и вне его.</span>}
              <button type="button" className="btn2" style={{ alignSelf: 'flex-start' }} onClick={() => setOpen('stressrec')}>{sr ? 'Пройти заново' : 'Пройти'}</button>
            </>
          )}
      </section>

      <section className="card" aria-label="Опросники">
        <span className="label">Опросники · Kouros напомнит, когда пора</span>
        <div>
          <Q name="Индекс Хупера" what="Сон, стресс, усталость, мышечная боль" when="Каждое утро" status={p.due.hooper ? 'ждёт' : 'пройден'} warn={p.due.hooper} />
          <Q name="TQR" what="Насколько восстановился перед тренировкой, 6–20" when="Перед тренировкой" status={p.due.tqr ? 'сегодня не пройден' : 'пройден'} />
          <Q name="Оценка тяжести" what="Тяжесть сессии × минуты — уточняет нагрузку силовых" when="После тренировки" status={p.unrated.length ? `ждут: ${p.unrated.length}` : 'всё оценено'} warn={p.unrated.length > 0} />
          <Q name="BRUMS" what="Профиль настроения по шести шкалам" when="Раз в неделю" status={p.due.brumsDays == null ? 'ни разу' : p.due.brumsDays >= 7 ? 'пора' : `через ${plural(7 - p.due.brumsDays, 'день', 'дня', 'дней')}`} warn={p.due.brumsDays == null || p.due.brumsDays >= 7} />
          <Q name="Стресс и восстановление" what="Баланс нагрузки и отдыха в жизни, анкета Kouros" when="Раз в 2 недели" status={p.due.stressrecDays == null ? 'ни разу' : p.due.stressrecDays >= 14 ? 'пора' : `через ${plural(14 - p.due.stressrecDays, 'день', 'дня', 'дней')}`} warn={p.due.stressrecDays == null || p.due.stressrecDays >= 14} />
          <Q name="Тревога перед стартом" what="Тревога и уверенность за 1–3 дня до соревнования" when="Перед стартами" status="этап 6, вместе со стартами" />
        </div>
      </section>
      <section className="card" aria-label="Тренер">
        <div className="between"><span className="label">Тренер Kouros</span><span className="chip chip-off">этап 5</span></div>
        <span className="muted small">Тренер Claude будет учитывать опросы в плане: например, перенести темповую на вечер, если стресс пикует днём, или снизить интенсивность при расхождении ощущений и часов.</span>
      </section>
      <p className="muted tiny" style={{ textAlign: 'center', margin: 0 }}>Опросники помогают замечать перегруз, но не заменяют консультацию специалиста.</p>
    </>
  );
}

function Q({ name, what, when, status, warn }: { name: string; what: string; when: string; status: string; warn?: boolean }) {
  return (
    <div className="list-row small">
      <span className="stack" style={{ gap: 0, flex: 1 }}><b>{name}</b><span className="muted tiny">{what}</span></span>
      <span className="muted tiny" style={{ width: 130 }}>{when}</span>
      <span className="chip" style={{ background: '#1B1F25', color: warn ? '#F5B642' : 'var(--muted)' }}>{status}</span>
    </div>
  );
}

function ScaleForm({ items, min, max, initial, labels, hint, onSave, onCancel }: {
  items: [string, string, string][]; min: number; max: number; initial?: number[]; labels?: string[]; hint?: string;
  onSave: (a: number[]) => void; onCancel?: () => void;
}) {
  const [a, setA] = useState<(number | null)[]>(initial ?? items.map(() => null));
  const done = a.every((x) => x != null);
  const vals = Array.from({ length: max - min + 1 }, (_, k) => k + min);
  return (
    <div className="stack" style={{ gap: 12 }}>
      {hint && <span className="small muted">{hint}</span>}
      {items.map(([q, lo, hi], i) => (
        <div key={q} className="stack" style={{ gap: 6 }}>
          <span className="small" style={{ fontWeight: 600 }}>{q}</span>
          <div className="scale" role="group" aria-label={q}>
            {vals.map((v) => <button key={v} type="button" aria-pressed={a[i] === v} title={labels?.[v - min] ?? ''} onClick={() => setA(a.map((x, k) => (k === i ? v : x)))}>{v}</button>)}
          </div>
          <div className="between tiny muted"><span>{lo}</span><span>{hi}</span></div>
        </div>
      ))}
      <div className="row">
        <button type="button" className="btn" disabled={!done} onClick={() => onSave(a as number[])}>Сохранить</button>
        {onCancel && <button type="button" className="btn2" onClick={onCancel}>Отмена</button>}
      </div>
    </div>
  );
}

function BrumsForm({ onSave, onCancel }: { onSave: (a: number[]) => void; onCancel: () => void }) {
  const [a, setA] = useState<(number | null)[]>(Array(24).fill(null));
  const words = BRUMS.flatMap(([, , w]) => w);
  const left = a.filter((x) => x == null).length;
  return (
    <div className="stack" style={{ gap: 10 }}>
      <span className="small muted">Как ты себя чувствуешь прямо сейчас, включая сегодня? 0 — совсем нет, 4 — очень сильно.</span>
      <div className="brums">
        {BRUMS_ORDER.map((idx) => (
          <div key={idx} className="brow">
            <span className="small">{words[idx]}</span>
            <div className="scale sm" role="group" aria-label={words[idx]}>
              {[0, 1, 2, 3, 4].map((v) => <button key={v} type="button" aria-pressed={a[idx] === v} title={BRUMS_SCALE[v]} onClick={() => setA(a.map((x, k) => (k === idx ? v : x)))}>{v}</button>)}
            </div>
          </div>
        ))}
      </div>
      <div className="row">
        <button type="button" className="btn" disabled={left > 0} onClick={() => onSave(a as number[])}>{left ? `Осталось ${left}` : 'Сохранить'}</button>
        <button type="button" className="btn2" onClick={onCancel}>Отмена</button>
      </div>
    </div>
  );
}

function iceberg(r: SurveyRow): { title: string; color: string } {
  const e = (r.extra ?? {}) as Record<string, number>;
  const neg = ['tension', 'depression', 'anger', 'confusion'].map((k) => e[k] ?? 0);
  const v = e.vigour ?? 0, f = e.fatigue ?? 0;
  if (v >= Math.max(...neg) + 4 && v > f) return { title: '«Айсберг» — хорошая адаптация', color: '#3DDC97' };
  if (f >= v) return { title: 'Усталость выше бодрости', color: '#F5B642' };
  if (Math.max(...neg) >= 8) return { title: 'Много напряжения', color: '#F5B642' };
  return { title: 'Ровный профиль', color: '#9AA3AE' };
}

function Iceberg({ row, prev }: { row: SurveyRow; prev?: SurveyRow }) {
  const e = (row.extra ?? {}) as Record<string, number>;
  const pe = (prev?.extra ?? {}) as Record<string, number>;
  const ib = iceberg(row);
  return (
    <div className="stack" style={{ gap: 10 }}>
      <div className="iceberg">
        {BRUMS.map(([k, name]) => {
          const v = e[k] ?? 0;
          const d = prev ? v - (pe[k] ?? 0) : 0;
          return (
            <div key={k} className="ib">
              <span className="num small" style={{ fontWeight: 700 }}>{v}</span>
              <div className="ibbar"><div style={{ height: `${(v / 16) * 100}%`, background: k === 'vigour' ? '#3DDC97' : k === 'fatigue' ? '#F5B642' : '#7FD3FF' }} /></div>
              <span className="tiny muted" style={{ textAlign: 'center' }}>{name}</span>
              {prev && d !== 0 && <span className="tiny num" style={{ color: (k === 'vigour') === d > 0 ? '#3DDC97' : '#F5B642' }}>{d > 0 ? '+' : ''}{d}</span>}
            </div>
          );
        })}
      </div>
      <span className="small" style={{ color: ib.color, fontWeight: 700 }}>{ib.title}</span>
      <span className="muted tiny" style={{ lineHeight: 1.5 }}>Когда бодрость возвышается над остальными шкалами, получается профиль «айсберг» — типичный для спортсмена в хорошей форме. Рост усталости и напряжения две недели подряд — ранний признак перегруза.{prev ? ` Изменения — относительно ${dayLabel(prev.date)}.` : ''}</span>
    </div>
  );
}

function StressRec({ row }: { row: SurveyRow }) {
  const e = (row.extra ?? {}) as { stress?: number; recovery?: number };
  const s = e.stress ?? 0, r = e.recovery ?? 0;
  const bal = r - s;
  const word = bal >= 1 ? ['Восстановления хватает', '#3DDC97'] : bal > -0.5 ? ['На грани — следи за отдыхом', '#F5B642'] : ['Стресс перевешивает отдых', '#FF6B6B'];
  return (
    <div className="stack" style={{ gap: 8 }}>
      <span style={{ fontFamily: 'var(--display)', fontSize: 20, fontWeight: 700, color: word[1] }}>{word[0]}</span>
      {[['Стресс', s, '#FF8A65'], ['Восстановление', r, '#3DDC97']].map(([n, v, c]) => (
        <div key={n as string} className="stack" style={{ gap: 4 }}>
          <div className="between small"><span>{n}</span><b className="num">{String((v as number).toFixed(1)).replace('.', ',')} из 6</b></div>
          <div style={{ height: 8, borderRadius: 999, background: '#22262D', overflow: 'hidden' }}><div style={{ width: `${((v as number) / 6) * 100}%`, height: '100%', background: c as string }} /></div>
        </div>
      ))}
      <span className="muted tiny">От {dayLabel(row.date)}. Анкета Kouros — короткий ориентир, а не клиническая методика.</span>
    </div>
  );
}

function DualChart({ today, hooper, ready }: { today: string; hooper: SurveyRow[]; ready: [string, number | null][] }) {
  const days = useMemo(() => Array.from({ length: 14 }, (_, k) => { const d = new Date(today + 'T12:00:00Z'); d.setUTCDate(d.getUTCDate() - 13 + k); return d.toISOString().slice(0, 10); }), [today]);
  const f = days.map((d) => feelOf(hooper.find((x) => x.date === d)));
  const r = days.map((d) => ready.find((x) => x[0] === d)?.[1] ?? null);
  const W = 600, H = 120;
  const x = (i: number) => (i / 13) * W;
  const y = (v: number) => H - 6 - (v / 100) * (H - 12);
  const line = (v: (number | null)[]) => v.map((p, i) => (p == null ? null : `${x(i).toFixed(1)},${y(p).toFixed(1)}`)).filter(Boolean).join(' ');
  if (!f.some((v) => v != null)) return <span className="muted small">Пройди утренний опрос несколько дней подряд — здесь появится сравнение с часами.</span>;
  return (
    <div className="stack" style={{ gap: 4 }}>
      <div className="chart" style={{ cursor: 'default' }}>
        <svg viewBox={`0 0 ${W} ${H}`} width="100%" height={H} preserveAspectRatio="none" aria-hidden="true">
          {days.map((d, i) => (f[i] != null && r[i] != null && Math.abs(f[i]! - r[i]!) >= 20 ? <rect key={d} x={x(i) - W / 26} width={W / 13} y="0" height={H} fill="#F5B642" opacity="0.12" /> : null))}
          <polyline points={line(r)} fill="none" stroke="#3DDC97" strokeWidth="2" vectorEffect="non-scaling-stroke" />
          <polyline points={line(f)} fill="none" stroke="#C792EA" strokeWidth="2" vectorEffect="non-scaling-stroke" />
        </svg>
        <span className="axis-top tiny muted num">100</span>
        <span className="axis-bot tiny muted num">0</span>
      </div>
      <div className="between tiny muted"><span>{dayLabel(days[0])}</span><span>{dayLabel(days[7])}</span><span>сегодня</span></div>
    </div>
  );
}
