import { useEffect, useState } from 'react';
import { call, errText, inTauri, fmtDateTime } from '../lib/api';
import { isPhone } from '../lib/platform';
type State = { hostRunning: boolean; address: string | null; paired: boolean; ready: boolean; remote: string | null; lastSync: string | null; error: string | null; busy: boolean; conflicts: number; counts: Record<string, number> };
type Conflict = { id: number; at: number; table: string; key: string; local: string; remote: string };
const TABLE_NAMES: Record<string,string> = { settings:'Настройки',exercises:'Упражнение',activities:'Тренировка',activity_streams:'Измерения тренировки',strength_sets:'Подходы',health_metrics:'Показатель здоровья',habit_log:'Привычка',surveys:'Опрос',readiness_log:'Утренняя готовность',evening_log:'Вечерняя готовность',meds:'Препарат',med_log:'Приём препарата',coach_reports:'Отчёт тренера',plan_days:'План дня',plan_changes:'Изменение плана',checkins:'Самочувствие после тренировки',pain_log:'Запись о боли',lab_reports:'Документ анализа',lab_results:'Результат анализа' };
const FIELDS: Record<string,string> = { name:'Название',date:'Дата',kind:'Вид',notes:'Заметки',note:'Заметка',description:'Описание',answers:'Ответы',score:'Оценка',level:'Уровень',body:'Текст',title:'Заголовок',value:'Значение',key:'Показатель',num:'Значение',txt:'Измерения',provider:'Источник',device:'Устройство',unit:'Единица',dose:'Доза',servings:'Количество порций',schedule:'Расписание',components:'Состав',archived:'В архиве',rpe:'Ощущаемая нагрузка',feel:'Самочувствие',pain:'Боль',reps:'Повторения',weight_kg:'Вес',exercise:'Упражнение',area:'Область',side:'Сторона',context:'Обстоятельства',items:'План',form:'Форма',course_from:'Начало курса',course_to:'Конец курса',value_text:'Результат',flag:'Отметка',lo:'Нижняя граница',hi:'Верхняя граница',comment:'Комментарий' };
function variant(text:string,remote=false):string {
  try { const parsed=JSON.parse(text); const row=remote ? parsed?.row : parsed;
    if (!row) return 'Запись удалена';
    const lines=Object.entries(row).filter(([key,value])=>FIELDS[key] && value!==null && value!=='').map(([key,value])=>`${FIELDS[key]}: ${typeof value==='string' ? value : JSON.stringify(value)}`);
    return (lines.length ? lines.join('\n') : 'Содержимое записи изменено.').slice(0,4000);
  } catch { return 'Не удалось показать содержимое записи.'; }
}
export function DeviceSync() {
  const phone = isPhone();
  const [status, setStatus] = useState<State | null>(null);
  const [address, setAddress] = useState(''); const [code, setCode] = useState('');
  const [offer, setOffer] = useState<{ code: string; address: string | null; expires: number } | null>(null);
  const [confirm, setConfirm] = useState(false); const [forget, setForget] = useState(false);
  const [busy, setBusy] = useState(false); const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState(''); const [progress, setProgress] = useState('');
  const [conflicts, setConflicts] = useState<Conflict[] | null>(null);
  const load = async () => { if (inTauri) setStatus(await call<State>('peer_status')); };
  useEffect(() => { if (status?.remote) setAddress(status.remote); }, [status?.remote]);
  useEffect(() => { load().catch(e => setError(errText(e))); const timer = setInterval(() => load().catch(() => {}), 5000);
    let active = true; let unlisten: (() => void) | undefined;
    if (inTauri) import('@tauri-apps/api/event').then(({ listen }) => listen<string>('peer-progress', e => { if (active) setProgress(e.payload); })).then(fn => { if (active) unlisten = fn; else fn(); });
    return () => { active = false; clearInterval(timer); unlisten?.(); };
  }, []);
  const run = async (fn: () => Promise<void>) => { setBusy(true); setError(null); setMessage(''); try { await fn(); await load(); } catch (e) { setError(errText(e)); } finally { setBusy(false); } };
  return <section className="card" aria-label="Windows и Android">
    <div className="between" style={{ flexWrap: 'wrap', gap: 8 }}><span className="label">Windows и Android</span><span className="chip">{status?.ready ? 'Устройства сопряжены' : 'Общая история по Wi-Fi'}</span></div>
    <p className="muted small">История, профиль, заметки, планы и настройки источников хранятся на обоих устройствах. Обмен защищён отдельным ключом и работает в одной домашней сети, когда приложения открыты. Пароли сервисов и ключи баз не передаются.</p>
    {!inTauri ? <div className="note-warn">Сопряжение доступно в установленном Kouros. Для переноса нужна новая версия на Windows и Android.</div> : phone ? <>
      {!status?.ready && <>
        <ol className="small" style={{ paddingLeft: 20, lineHeight: 1.7 }}><li>На Windows открой Настройки → Синхронизация → Windows и Android.</li><li>Нажми «Показать код сопряжения». Введи адрес и код ниже.</li></ol>
        <label className="stack small">Адрес Windows<input className="input" aria-label="Адрес Windows" placeholder="192.168.1.10:8766" value={address} onChange={e => setAddress(e.target.value)} autoCapitalize="none" autoCorrect="off" /></label>
        <label className="stack small">Код сопряжения<input className="input mono" aria-label="Код сопряжения" placeholder="32 знака с экрана Windows" value={code} onChange={e => setCode(e.target.value)} autoCapitalize="none" autoCorrect="off" /></label>
        <label className="row small" style={{ alignItems: 'flex-start', gap: 8 }}><input type="checkbox" checked={confirm} onChange={e => setConfirm(e.target.checked)} /><span>Взять историю Windows за основу. Текущая история Android будет заменена; перед переносом Kouros сохранит её зашифрованную резервную копию.</span></label>
        <button className="btn" type="button" disabled={!confirm || !address.trim() || !code.trim() || busy} onClick={() => run(async () => { const r = await call<{ applied: number; backup: string }>('peer_pair', { address, code, replace: true }); setCode(''); setMessage(`История перенесена: ${r.applied} записей. Резервная копия прежней базы: ${r.backup}`); })}>Перенести историю с Windows</button>
      </>}
      {status?.ready && <><span className="small">Компьютер: {status.remote}</span><details><summary className="small">Изменить адрес компьютера</summary><div className="stack" style={{ marginTop: 8 }}><label className="stack small">Адрес Windows<input className="input" value={address} onChange={e => setAddress(e.target.value)} /></label><span className="muted tiny">Если роутер выдал Windows новый адрес, измени его здесь. История и сопряжение сохранятся.</span><button type="button" className="btn2" disabled={busy || !address.trim()} onClick={() => run(async () => { await call('peer_address', { address }); setMessage('Адрес компьютера сохранён.'); })}>Сохранить адрес</button></div></details><span className="muted tiny">Автоматический обмен — примерно раз в минуту, пока оба приложения доступны.</span><button className="btn" type="button" disabled={busy || status.busy} onClick={() => run(async () => { const r = await call<{ sent: number; received: number }>('peer_sync'); setMessage(`Готово. Передано изменений: ${r.sent}, получено: ${r.received}.`); })}>Синхронизировать сейчас</button></>}
    </> : <>
      <span className="small">{status?.hostRunning ? `Приём открыт: ${status.address ?? 'адрес не найден'}` : 'Нажми кнопку, чтобы разрешить сопряжение по домашнему Wi-Fi.'}</span>
      <button className="btn" type="button" disabled={busy} onClick={() => run(async () => setOffer(await call('peer_enable')))}>Показать код сопряжения</button>
      {offer && <div className="inset"><b>Адрес: {offer.address ?? 'Не найден. Проверь подключение к домашней сети.'}</b><code className="mono" style={{ overflowWrap: 'anywhere', fontSize: 18 }}>{offer.code.match(/.{1,4}/g)?.join(' ')}</code><span className="muted tiny">Введи код на Android в течение 5 минут. Разреши Kouros доступ к частной сети в брандмауэре Windows. Код даёт доступ к истории — показывай его только своему устройству.</span></div>}
    </>}
    {status?.lastSync && <span className="small">Последний обмен: {fmtDateTime(Number(status.lastSync))}</span>}
    {(busy || status?.busy) && <div role="status" className="small">{progress || 'Соединяюсь…'}</div>}
    {message && <div role="status" className="note-ok" style={{ overflowWrap: 'anywhere' }}>{message}</div>}
    {(error || status?.error) && <div role="alert" className="note-err" style={{ overflowWrap: 'anywhere' }}>{error || status?.error}</div>}
    {status?.ready && <button className="btn2" type="button" disabled={busy} onClick={() => run(async () => setConflicts(await call('peer_conflicts')))}>Проверить конфликты изменений{status.conflicts ? ` (${status.conflicts})` : ''}</button>}
    {conflicts && <div className="stack">{!conflicts.length && <span className="small">Неразобранных конфликтов нет.</span>}{conflicts.map(item => <div key={item.id} className="inset"><b className="small">Разные изменения: {TABLE_NAMES[item.table] ?? 'Запись'}</b><span className="muted tiny">{fmtDateTime(item.at)}. Один вариант уже выбран по порядку изменений; оба сохранены. «Локальный» ниже означает вариант на Windows.</span><details><summary className="small">Сравнить варианты</summary><pre style={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere', maxHeight: 240, overflowY: 'auto', fontSize: 12 }}>{`Windows:\n${variant(item.local)}\n\nAndroid:\n${variant(item.remote,true)}`}</pre></details><div className="row" style={{ flexWrap: 'wrap' }}>{[['current', 'Оставить текущий'], ['local', 'Вариант Windows'], ['remote', 'Вариант Android']].map(([choice, text]) => <button key={choice} type="button" className="btn2" disabled={busy} onClick={() => run(async () => { await call('peer_resolve', { id: item.id, choice }); setConflicts(await call('peer_conflicts')); })}>{text}</button>)}</div></div>)}</div>}
    {(status?.paired || status?.hostRunning) && <><button type="button" className="linkbtn small" onClick={() => setForget(!forget)}>Отключить сопряжение</button>{forget && <div className="inset"><span className="small">История останется на обоих устройствах. Для нового сопряжения потребуется снова перенести основу с Windows.</span><button type="button" className="btn2" disabled={busy} onClick={() => run(async () => { await call('peer_forget'); setOffer(null); setForget(false); setMessage('Сопряжение отключено. История сохранена.'); })}>Подтвердить отключение</button></div>}</>}
  </section>;
}
