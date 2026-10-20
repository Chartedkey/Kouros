// «Анализы» (0.20): lab reports from a PDF (read on this computer) or from Claude's [АНАЛИЗ]
// lines, every test with its history and plain-word notes for training and food.
import { useCallback, useEffect, useState } from 'react';
import { api, errText, inTauri, type LabNote, type LabPreview, type LabRow, type LabsSummary, type LabTest } from '../lib/api';
import { useDataChanged } from '../lib/events';

const COL = { bad: '#FF6B6B', warn: '#F5B642', info: '#7FD3FF', ok: '#3DDC97' };
const ST: Record<string, [string, string]> = { low: ['ниже нормы', '#F5B642'], high: ['выше нормы', '#F5B642'], ok: ['в норме', '#3DDC97'], '': ['', 'var(--text-2)'] };
const ru = (d: string) => new Date(d + 'T12:00:00').toLocaleDateString('ru-RU', { day: 'numeric', month: 'short', year: 'numeric' });
const num = (v: number | null | undefined) => (v == null ? '—' : (Math.abs(v) >= 100 ? Math.round(v) : Math.round(v * 100) / 100).toString().replace('.', ','));

async function pickPdf(): Promise<string | null> {
  if (!inTauri) return 'пример.pdf';
  const { open } = await import('@tauri-apps/plugin-dialog');
  const r = await open({ multiple: false, title: 'PDF с результатами анализов', filters: [{ name: 'PDF', extensions: ['pdf'] }] });
  return typeof r === 'string' ? r : null;
}

function NoteBox({ n }: { n: LabNote }) {
  const c = COL[n.level];
  return (
    <div className="stack small" style={{ gap: 4, lineHeight: 1.5, padding: '10px 12px', borderRadius: 10, background: c + '12', border: `1px solid ${c}44` }}>
      <b style={{ color: c }}>{n.title}</b>
      {n.text && <span>{n.text}</span>}
      {n.todo && <span className="muted">{n.todo}</span>}
    </div>
  );
}

/** Values over time with the lab range as a band. */
function Spark({ t }: { t: LabTest }) {
  const s = t.series;
  if (s.length < 2) return null;
  const W = 520, H = 90, P = 8;
  const vals = s.map((x) => x[1]);
  const lo = t.latest.lo, hi = t.latest.hi;
  let min = Math.min(...vals, lo ?? Infinity), max = Math.max(...vals, hi ?? -Infinity);
  if (!Number.isFinite(min)) min = Math.min(...vals);
  if (!Number.isFinite(max)) max = Math.max(...vals);
  const pad = (max - min) * 0.15 || 1;
  min -= pad; max += pad;
  const t0 = new Date(s[0][0]).getTime(), t1 = new Date(s[s.length - 1][0]).getTime() || t0 + 1;
  const x = (d: string) => P + ((new Date(d).getTime() - t0) / Math.max(1, t1 - t0)) * (W - 2 * P);
  const y = (v: number) => H - P - ((v - min) / (max - min)) * (H - 2 * P);
  const bandTop = y(hi ?? max), bandBot = y(lo ?? min);
  return (
    <svg viewBox={`0 0 ${W} ${H + 16}`} style={{ width: '100%', height: 'auto' }} role="img" aria-label={`${t.name}: история`}>
      {(lo != null || hi != null) && <rect x={0} y={bandTop} width={W} height={Math.max(0, bandBot - bandTop)} fill="#3DDC97" opacity={0.08} />}
      <polyline fill="none" stroke="#7FD3FF" strokeWidth={2} points={s.map((p) => `${x(p[0])},${y(p[1])}`).join(' ')} />
      {s.map((p) => {
        const out = (lo != null && p[1] < lo) || (hi != null && p[1] > hi);
        return (
          <g key={p[0]}>
            <circle cx={x(p[0])} cy={y(p[1])} r={4} fill={out ? '#F5B642' : '#7FD3FF'} />
            <text x={x(p[0])} y={y(p[1]) - 8} fontSize={11} textAnchor="middle" fill="var(--text-2)">{num(p[1])}</text>
            <text x={x(p[0])} y={H + 12} fontSize={10} textAnchor="middle" fill="var(--text-3, #777)">{new Date(p[0] + 'T12:00:00').toLocaleDateString('ru-RU', { month: 'short', year: '2-digit' })}</text>
          </g>
        );
      })}
    </svg>
  );
}

function Preview({ p, onDone }: { p: LabPreview; onDone: (saved: boolean) => void }) {
  const [date, setDate] = useState(p.date ?? '');
  const [lab, setLab] = useState(p.lab);
  const [rows, setRows] = useState<LabRow[]>(p.rows);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const set = (i: number, r: Partial<LabRow>) => setRows(rows.map((x, j) => (j === i ? { ...x, ...r } : x)));
  const save = async () => {
    setBusy(true);
    try {
      const fixed = rows.map((r) => {
        const v = Number(r.valueText.replace(',', '.').replace(/^[<>]\s*/, ''));
        return { ...r, value: r.valueText.trim() && Number.isFinite(v) ? v : null, lo: null, hi: null };
      });
      await api.labsSave({ ...p, date, lab, rows: fixed });
      onDone(true);
    } catch (e) { setErr(errText(e)); } finally { setBusy(false); }
  };
  const known = rows.filter((r) => r.key && !r.skip).length;
  const sections = Array.from(new Set(rows.map((r) => r.section)));
  return (
    <section className="card" aria-label="Проверка результатов">
      <div className="between"><span className="label">Проверь, что Kouros прочитал</span><button type="button" className="xbtn" aria-label="Закрыть" onClick={() => onDone(false)}>×</button></div>
      <span className="small muted" style={{ lineHeight: 1.5 }}>
        Найдено {rows.length} показателей, из них Kouros знает {known} — по ним будут подсказки и графики. Остальные сохранятся как есть. Если цифра прочиталась неверно — поправь, лишнее — сними галочку.
      </span>
      <div className="row" style={{ gap: 12, flexWrap: 'wrap' }}>
        <label className="stack" style={{ gap: 4 }}><span className="muted tiny">Дата сдачи</span><input type="date" className="select" value={date} onChange={(e) => setDate(e.target.value)} /></label>
        <label className="stack" style={{ gap: 4, flex: '1 1 200px' }}><span className="muted tiny">Лаборатория</span><input className="input" value={lab} onChange={(e) => setLab(e.target.value)} /></label>
      </div>
      <div className="stack" style={{ gap: 2, maxHeight: 460, overflowY: 'auto' }}>
        {sections.map((sec) => (
          <div key={sec} className="stack" style={{ gap: 2 }}>
            {sec && <span className="tiny muted" style={{ marginTop: 8, fontWeight: 700 }}>{sec}</span>}
            {rows.map((r, i) => r.section !== sec ? null : (
              <div key={i} className="row small" style={{ gap: 8, alignItems: 'center', opacity: r.skip ? 0.45 : 1, flexWrap: 'wrap' }}>
                <input type="checkbox" checked={!r.skip} onChange={(e) => set(i, { skip: !e.target.checked })} aria-label="Сохранить" />
                <span style={{ flex: '2 1 220px', lineHeight: 1.35 }}>{r.name}{r.key && <span className="tiny" style={{ color: '#3DDC97' }}> · {r.keyName}</span>}</span>
                <input className="input" style={{ width: 90, flex: 'none', color: r.flag ? '#F5B642' : undefined }} value={r.valueText} onChange={(e) => set(i, { valueText: e.target.value })} aria-label="Результат" />
                <span className="muted" style={{ width: 80, flex: 'none' }}>{r.unit}</span>
                <span className="muted tiny" style={{ flex: '1 1 90px' }}>{r.refText}</span>
              </div>
            ))}
          </div>
        ))}
      </div>
      {err && <div className="note-err">{err}</div>}
      <div className="row" style={{ gap: 8 }}>
        <button type="button" className="btn" disabled={busy || !date} onClick={save}>{busy ? 'Сохраняю…' : 'Сохранить'}</button>
        <button type="button" className="btn2" onClick={() => onDone(false)}>Отмена</button>
      </div>
    </section>
  );
}

export function Labs() {
  const [s, setS] = useState<LabsSummary | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [prev, setPrev] = useState<LabPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [paste, setPaste] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [onlyOut, setOnlyOut] = useState(false);
  const load = useCallback(() => { api.labsSummary().then((v) => { setS(v); setErr(null); }).catch((e) => setErr(errText(e))); }, []);
  useEffect(load, [load]);
  useDataChanged(load);

  const fromPdf = async () => {
    const f = await pickPdf();
    if (!f) return;
    setBusy(true); setErr(null);
    try { setPrev(await api.labsParse(f)); } catch (e) { setErr(errText(e)); } finally { setBusy(false); }
  };
  const fromText = async () => {
    if (!paste?.trim()) return;
    try { setPrev(await api.labsParseText(paste)); setPaste(null); setErr(null); } catch (e) { setErr(errText(e)); }
  };
  const del = async (id: number) => {
    try { await api.labsDelete(id); load(); } catch (e) { setErr(errText(e)); }
  };

  if (prev) return <Preview p={prev} onDone={(saved) => { setPrev(null); if (saved) load(); }} />;
  const tests = (s?.tests ?? []).filter((t) => !onlyOut || t.latest.status === 'low' || t.latest.status === 'high');
  const groups = Array.from(new Set(tests.map((t) => t.group)));
  return (
    <>
      <section className="card" aria-label="Загрузить анализы">
        <div className="between"><span className="label">Добавить результаты</span><span className="muted tiny">файл остаётся на этом компьютере</span></div>
        <span className="small muted" style={{ lineHeight: 1.5 }}>
          PDF из личного кабинета лаборатории (KDL, Инвитро, Гемотест, Хеликс и другие) Kouros прочитает сам. Скан или фото бланка — отправь Claude в проект «Тренер Kouros» и вставь сюда ответ со строками [АНАЛИЗ].
        </span>
        <div className="row" style={{ gap: 8, flexWrap: 'wrap' }}>
          <button type="button" className="btn" disabled={busy} onClick={fromPdf}>{busy ? 'Читаю PDF…' : 'Загрузить PDF'}</button>
          <button type="button" className="btn2" onClick={() => setPaste(paste == null ? '' : null)}>Вставить ответ Claude</button>
        </div>
        {paste != null && (
          <div className="stack" style={{ gap: 6 }}>
            <textarea className="input" rows={6} value={paste} onChange={(e) => setPaste(e.target.value)} placeholder={'[АНАЛИЗЫ] 14.10.2026, KDL\n[АНАЛИЗ] Ферритин | 45 | мкг/л | 30-400'} />
            <button type="button" className="btn2" style={{ alignSelf: 'flex-start' }} disabled={!paste.trim()} onClick={fromText}>Разобрать</button>
          </div>
        )}
        {err && <div className="note-err">{err}</div>}
      </section>

      {s && s.tests.length === 0 && (
        <section className="card">
          <span className="label">Анализов пока нет</span>
          <span className="small muted" style={{ lineHeight: 1.5 }}>
            Для бегуна полезно раз в 3–6 месяцев: общий анализ крови, ферритин, витамин D, B12, С-реактивный белок, КФК, глюкоза и липиды. Сдавай утром натощак и после 2–3 спокойных дней без тяжёлых и длинных тренировок — иначе КФК, АСТ и креатинин будут выше обычного.
          </span>
        </section>
      )}

      {s && s.findings.length > 0 && (
        <section className="card" aria-label="На что обратить внимание">
          <span className="label">На что обратить внимание</span>
          <div className="stack" style={{ gap: 8 }}>{s.findings.map((f) => <NoteBox key={f.key} n={f} />)}</div>
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>Kouros не ставит диагнозов: он сравнивает с нормой лаборатории и подсказывает, что это может значить для тренировок и питания. Решения — вместе с врачом.</span>
        </section>
      )}

      {s && s.tests.length > 0 && (
        <section className="card" aria-label="Показатели">
          <div className="between">
            <span className="label">Показатели · последний результат</span>
            <label className="row tiny" style={{ gap: 6, alignItems: 'center' }}><input type="checkbox" checked={onlyOut} onChange={(e) => setOnlyOut(e.target.checked)} />только вне нормы</label>
          </div>
          {groups.map((g) => (
            <div key={g} className="stack" style={{ gap: 2 }}>
              <span className="tiny muted" style={{ marginTop: 8, fontWeight: 700 }}>{g}</span>
              {tests.filter((t) => t.group === g).map((t) => {
                const [st, c] = ST[t.latest.status] ?? ST[''];
                const isOpen = open === t.key;
                return (
                  <div key={t.key} className="stack" style={{ gap: 6 }}>
                    <button type="button" className="list-row small" style={{ background: 'none', border: 'none', color: 'inherit', textAlign: 'left', cursor: 'pointer', padding: '6px 0' }} onClick={() => setOpen(isOpen ? null : t.key)} aria-expanded={isOpen}>
                      <span style={{ flex: 1 }}>{t.name}{t.note && t.note.level !== 'info' && <span style={{ color: COL[t.note.level] }}> ●</span>}</span>
                      <span className="num" style={{ color: c }}>{t.latest.value != null ? num(t.latest.value) : t.latest.text} <span className="muted">{t.unit}</span></span>
                      <span className="muted tiny" style={{ width: 150, textAlign: 'right' }}>{st ? `${st} · ` : ''}{ru(t.latest.date)}</span>
                    </button>
                    {isOpen && (
                      <div className="stack" style={{ gap: 8, paddingBottom: 8 }}>
                        <span className="muted tiny">Норма лаборатории: {t.latest.ref || 'не указана'}{t.printed !== t.name ? ` · в бланке: ${t.printed}` : ''}{t.count > 1 ? ` · ${t.count} результата` : ''}</span>
                        <Spark t={t} />
                        {t.note && <NoteBox n={t.note} />}
                        {t.latest.comment && <span className="muted tiny" style={{ lineHeight: 1.5, whiteSpace: 'pre-line' }}>Комментарий лаборатории: {t.latest.comment}</span>}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          ))}
        </section>
      )}

      {s && s.reports.length > 0 && (
        <section className="card" aria-label="Сдачи">
          <span className="label">Сдачи</span>
          {s.reports.map((r) => (
            <div key={r.id} className="list-row small" style={{ gap: 8 }}>
              <span style={{ flex: 1 }}>{ru(r.date)}{r.lab ? ` · ${r.lab}` : ''} <span className="muted">· {r.count} показ.{r.source === 'claude' ? ' · из ответа Claude' : ''}</span></span>
              {r.file && <button type="button" className="linkbtn tiny" onClick={() => api.labsOpen(r.id).catch((e) => setErr(errText(e)))}>Открыть PDF</button>}
              <button type="button" className="linkbtn tiny" onClick={() => { if (window.confirm('Удалить эту сдачу со всеми результатами?')) del(r.id); }}>Удалить</button>
            </div>
          ))}
        </section>
      )}
    </>
  );
}
