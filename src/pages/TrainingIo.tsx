import { useCallback, useEffect, useState } from 'react';
import {
  api, errText, fmtIso, fmtKm, fmtWhen, inTauri, plural, sportOf,
  type ImportLog, type ImportResult, type Preview,
} from '../lib/api';

// ---------- Импорт и экспорт ----------

type Stage = { kind: 'idle' } | { kind: 'busy'; text: string } | { kind: 'preview'; p: Preview } | { kind: 'done'; r: ImportResult; title: string };

async function pickFiles(): Promise<string[]> {
  if (!inTauri) return ['export_83112406.zip'];
  const { open } = await import('@tauri-apps/plugin-dialog');
  const r = await open({
    multiple: true,
    title: 'Файлы тренировок или архив Strava',
    filters: [{ name: 'Тренировки', extensions: ['zip', 'fit', 'gpx', 'tcx', 'gz'] }],
  });
  if (r === null) return [];
  return Array.isArray(r) ? r : [r];
}

export function IoTab({ onDone }: { onDone: () => void }) {
  const [stage, setStage] = useState<Stage>({ kind: 'idle' });
  const [merge, setMerge] = useState(true);
  const [over, setOver] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [hist, setHist] = useState<ImportLog[]>([]);

  const loadHist = () => api.importHistory().then(setHist).catch(() => {});
  useEffect(() => { loadHist(); }, []);

  const preview = useCallback(async (paths: string[]) => {
    if (!paths.length) return;
    setErr(null);
    setStage({ kind: 'busy', text: paths.length === 1 ? 'Читаю файл…' : `Читаю ${paths.length} файлов…` });
    try {
      setStage({ kind: 'preview', p: await api.importPreview(paths) });
    } catch (e) {
      setErr(errText(e));
      setStage({ kind: 'idle' });
    }
  }, []);

  // Files dragged onto the window.
  useEffect(() => {
    if (!inTauri) return;
    let un: (() => void) | undefined;
    let alive = true;
    import('@tauri-apps/api/webview').then(({ getCurrentWebview }) =>
      getCurrentWebview().onDragDropEvent((e) => {
        if (e.payload.type === 'drop') { setOver(false); preview(e.payload.paths); }
        else if (e.payload.type === 'leave') setOver(false);
        else setOver(true);
      }).then((f) => { if (alive) un = f; else f(); }),
    );
    return () => { alive = false; un?.(); };
  }, [preview]);

  async function commit() {
    if (stage.kind !== 'preview') return;
    const title = stage.p.title;
    setStage({ kind: 'busy', text: 'Сохраняю тренировки…' });
    try {
      const r = await api.importCommit(merge);
      setStage({ kind: 'done', r, title });
      loadHist();
    } catch (e) {
      setErr(errText(e));
      setStage({ kind: 'idle' });
    }
  }

  function cancel() {
    api.importCancel().catch(() => {});
    setStage({ kind: 'idle' });
  }

  return (
    <div className="grid">
      <section className="card" aria-label="Импорт тренировок">
        <div className="between"><span className="label">Импорт тренировок</span><span className="muted tiny">FIT · GPX · TCX · GZ · ZIP</span></div>
        {err && <div className="note-err">{err}</div>}

        {stage.kind === 'idle' && (
          <button type="button" className={`drop${over ? ' over' : ''}`} onClick={async () => preview(await pickFiles())}>
            <span className="drop-ic">⇪</span>
            <span style={{ fontSize: 17, fontWeight: 700 }}>Перетащи файлы или архив сюда</span>
            <span className="muted small" style={{ lineHeight: 1.5 }}>
              или нажми, чтобы выбрать · один файл, несколько файлов или .zip целиком, включая архив данных Strava
            </span>
          </button>
        )}

        {stage.kind === 'busy' && <div className="drop" aria-busy="true"><span className="muted">{stage.text}</span></div>}

        {stage.kind === 'preview' && <PreviewView p={stage.p} merge={merge} setMerge={setMerge} onGo={commit} onCancel={cancel} />}

        {stage.kind === 'done' && (
          <div className="note-ok stack" style={{ gap: 10 }}>
            <span style={{ fontSize: 16, fontWeight: 700, color: 'var(--green)' }}>
              ✓ {stage.r.added ? `Импортировано: ${plural(stage.r.added, 'тренировка', 'тренировки', 'тренировок')}` : 'Новых тренировок не было'}
            </span>
            <span>
              {stage.r.merged ? `Дополнено: ${stage.r.merged}. ` : ''}
              {stage.r.skipped ? `Пропущено как уже существующие: ${stage.r.skipped}. ` : ''}
              {stage.r.errors ? `Не прочитано файлов: ${stage.r.errors}.` : ''}
            </span>
            <div className="row">
              <button type="button" className="btn2" onClick={onDone}>Открыть обзор</button>
              <button type="button" className="btn2" onClick={() => setStage({ kind: 'idle' })}>Импортировать ещё</button>
            </div>
          </div>
        )}

        <div className="stack" style={{ gap: 0 }}>
          <span className="small" style={{ fontWeight: 700, paddingBottom: 6 }}>История импорта</span>
          {hist.length === 0 && <div className="list-row muted small">Импортов ещё не было</div>}
          {hist.map((h, i) => (
            <div key={i} className="list-row small">
              <span className="muted" style={{ whiteSpace: 'nowrap' }}>{fmtIso(h.at)}</span>
              <span className="ellipsis" style={{ flex: 1 }}>{h.title}</span>
              <span style={{ fontWeight: 700, color: 'var(--green)' }}>+{h.added}{h.merged ? ` · ↻${h.merged}` : ''}</span>
            </div>
          ))}
        </div>
      </section>

      <div className="stack" style={{ gap: 20 }}>
        <ExportCard />
        <section className="card" aria-label="Архив Strava">
          <span className="label">Архив данных Strava</span>
          <span className="small" style={{ lineHeight: 1.55, color: 'var(--text-2)' }}>
            Kouros читает архив целиком: тренировки из папки activities, названия, описания и подходы — из activities.csv.
            Распаковывать не нужно.
          </span>
          <ol className="small" style={{ margin: 0, paddingLeft: 20, lineHeight: 1.6, color: 'var(--text-2)' }}>
            <li>strava.com → Настройки → Мой аккаунт → «Скачать или удалить аккаунт» → «Запросить архив»</li>
            <li>Strava пришлёт ссылку на почту</li>
            <li>Перетащи скачанный .zip сюда — дубликаты Kouros пропустит сам</li>
          </ol>
        </section>
      </div>
    </div>
  );
}

function PreviewView({ p, merge, setMerge, onGo, onCancel }: {
  p: Preview; merge: boolean; setMerge: (v: boolean) => void; onGo: () => void; onCancel: () => void;
}) {
  const canGo = p.new > 0 || (merge && p.mergeable > 0);
  const goLbl = p.new > 0
    ? `Импортировать ${p.new}${merge && p.mergeable ? ` и дополнить ${p.mergeable}` : ''}`
    : merge && p.mergeable ? `Дополнить ${p.mergeable}` : 'Нечего импортировать';
  return (
    <div className="stack" style={{ gap: 16 }}>
      <div className="stack" style={{ gap: 4 }}>
        <span style={{ fontSize: 16, fontWeight: 700, wordBreak: 'break-all' }}>{p.title}</span>
        <span className="muted small">
          {p.kind === 'strava_archive' ? 'Архив данных Strava' : 'Файлы тренировок'}
          {p.firstUtc && p.lastUtc ? ` · ${new Date(p.firstUtc * 1000).toLocaleDateString('ru-RU')} — ${new Date(p.lastUtc * 1000).toLocaleDateString('ru-RU')}` : ''}
        </span>
      </div>
      <div className="tiles4">
        <div className="tile"><span className="num tile-v">{p.found}</span><span className="muted tiny">найдено</span></div>
        <div className="tile"><span className="num tile-v" style={{ color: 'var(--muted)' }}>{p.existing}</span><span className="muted tiny">уже есть</span></div>
        <div className="tile"><span className="num tile-v" style={{ color: 'var(--green)' }}>{p.new}</span><span className="muted tiny">новых</span></div>
        <div className="tile"><span className="num tile-v" style={{ color: p.errors.length ? 'var(--red)' : 'var(--muted)' }}>{p.errors.length}</span><span className="muted tiny">с ошибкой</span></div>
      </div>
      {p.newRows.length > 0 && (
        <div className="stack" style={{ gap: 0 }}>
          <span className="small" style={{ fontWeight: 700, paddingBottom: 6 }}>Новые тренировки</span>
          {p.newRows.map((r, i) => (
            <div key={i} className="list-row small">
              <span className="muted" style={{ whiteSpace: 'nowrap', width: 140 }}>{fmtWhen(r.startUtc)}</span>
              <span className="ellipsis" style={{ flex: 1 }}><b>{r.name || sportOf(r.sport).name}</b> <span className="muted">· {sportOf(r.sport).name}</span></span>
              <span className="num">{fmtKm(r.distanceM)}</span>
            </div>
          ))}
          {p.new > p.newRows.length && <span className="muted tiny" style={{ paddingTop: 6 }}>и ещё {p.new - p.newRows.length}</span>}
        </div>
      )}
      {p.existing > 0 && (
        <div className="inset">
          <span className="small" style={{ fontWeight: 700 }}>{plural(p.existing, 'тренировка уже есть', 'тренировки уже есть', 'тренировок уже есть')} в Kouros</span>
          <div className="row" role="group" aria-label="Что делать с совпадениями">
            <button type="button" className="pill" aria-pressed={merge} onClick={() => setMerge(true)}>Объединить — дописать недостающее</button>
            <button type="button" className="pill" aria-pressed={!merge} onClick={() => setMerge(false)}>Пропустить</button>
          </div>
          <span className="muted tiny" style={{ lineHeight: 1.5 }}>
            {merge
              ? `Совпадение — по номеру Strava или по старту ±2 мин и длительности. ${p.mergeable ? `У ${p.mergeable} из них в файле есть пульс и темп, которых нет в Kouros — допишу.` : 'Дописывать нечего — данные уже полные.'}`
              : 'Совпавшие тренировки не трогаю.'}
          </span>
        </div>
      )}
      {p.errors.length > 0 && (
        <div className="note-err stack" style={{ gap: 6 }}>
          <b>Не удалось прочитать</b>
          {p.errors.slice(0, 6).map((e, i) => (
            <span key={i} className="tiny"><b style={{ wordBreak: 'break-all' }}>{e.name}</b> — {e.why}</span>
          ))}
          {p.errors.length > 6 && <span className="tiny">и ещё {p.errors.length - 6}</span>}
          {p.kind === 'strava_archive' && <span className="tiny">Сами тренировки из архива всё равно сохранятся — без графиков пульса.</span>}
        </div>
      )}
      <div className="row">
        <button type="button" className="btn btn-green" disabled={!canGo} onClick={onGo}>{goLbl}</button>
        <button type="button" className="btn2" onClick={onCancel}>Отмена</button>
      </div>
    </div>
  );
}

function ExportCard() {
  const [days, setDays] = useState<number | null>(30);
  const [fmt, setFmt] = useState<'csv' | 'gpx' | 'original'>('csv');
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const notes = {
    csv: 'Таблица для Excel: дата, тип, дистанция, время, набор высоты, пульс, калории, подходы.',
    gpx: 'Трек, высота, пульс и каденс. Тренировки без GPS (силовые, физиотерапия) в GPX не попадают.',
    original: 'Файлы ровно такими, какими их записали часы (.fit, .tcx, .gpx) — для Garmin Connect, intervals.icu, TrainingPeaks.',
  };
  async function go() {
    setBusy(true); setMsg(null);
    try {
      const r = await api.exportActivities(days, fmt);
      setMsg({ ok: true, text: `Готово: ${plural(r.files, 'файл', 'файла', 'файлов')}${r.skipped ? `, пропущено ${r.skipped}` : ''}. Папка открыта: ${r.folder}` });
    } catch (e) { setMsg({ ok: false, text: errText(e) }); } finally { setBusy(false); }
  }
  return (
    <section className="card" aria-label="Экспорт">
      <span className="label">Экспорт</span>
      <div className="stack" style={{ gap: 6 }}>
        <span className="small" style={{ fontWeight: 600 }}>Период</span>
        <div className="row" role="group" aria-label="Период">
          {([[30, '30 дней'], [90, '3 месяца'], [365, 'Год'], [null, 'Всё']] as [number | null, string][]).map(([d, n]) => (
            <button key={n} type="button" className="pill" aria-pressed={days === d} onClick={() => setDays(d)}>{n}</button>
          ))}
        </div>
      </div>
      <div className="stack" style={{ gap: 6 }}>
        <span className="small" style={{ fontWeight: 600 }}>Формат</span>
        <div className="row" role="group" aria-label="Формат">
          {([['csv', '.csv сводка'], ['gpx', '.gpx треки'], ['original', 'Оригиналы с часов']] as ['csv' | 'gpx' | 'original', string][]).map(([f, n]) => (
            <button key={f} type="button" className="pill" aria-pressed={fmt === f} onClick={() => setFmt(f)}>{n}</button>
          ))}
        </div>
        <span className="muted tiny" style={{ lineHeight: 1.5 }}>{notes[fmt]}</span>
      </div>
      <div className="between divider" style={{ paddingTop: 14 }}>
        <span className="muted small">Сохраню в папку Kouros\exports и открою её</span>
        <button type="button" className="btn" disabled={busy} onClick={go}>{busy ? 'Сохраняю…' : 'Скачать'}</button>
      </div>
      {msg && <div className={msg.ok ? 'note-ok' : 'note-err'} style={{ wordBreak: 'break-all' }}>{msg.text}</div>}
    </section>
  );
}

