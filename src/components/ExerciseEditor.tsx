import { useEffect, useRef, useState } from 'react';
import { api, errText, type ExerciseItem } from '../lib/api';
import { EQUIP, MUSCLE, refreshExerciseLib } from './Exercises';

const equipment = ['body only', 'bands', 'mini bands', 'kettlebells', 'dumbbell', 'barbell', 'machine', 'cable', 'other'];
const purposes = [['strength','Силовое'],['ofp_running','ОФП для бега'],['mobility','Подвижность / растяжка']];
export function ExerciseEditor({ initial, name = '', onSaved, onClose }: { initial?: ExerciseItem; name?: string; onSaved: (x: ExerciseItem) => void; onClose: () => void }) {
  const [x, setX] = useState<ExerciseItem>(() => initial ? { ...initial, purpose: initial.purpose ?? [initial.cat === 'stretching' ? 'mobility' : 'strength'], measurement: initial.measurement ?? (initial.st ? 'seconds' : 'reps') } : ({ id: `c_${crypto.randomUUID()}`, ru: name, en: '', syn: [], tip: '', cat: 'strength', eq: 'body only', lvl: '', mech: '', force: '', p: [], s: [], bw: null, st: false, img: 0, own: true, purpose: ['strength'], measurement: 'reps', weightMeaning: 'total' }));
  const [error, setError] = useState(''); const [busy,setBusy] = useState(false); const [reading,setReading]=useState(false);
  const file = useRef<HTMLInputElement>(null); const reader = useRef<FileReader | null>(null);
  useEffect(() => () => { reader.current?.abort(); }, []);
  useEffect(() => { const key=(e:KeyboardEvent)=>{if(e.key==='Escape'){e.stopImmediatePropagation();if(!busy)onClose();}};window.addEventListener('keydown',key,true);return()=>window.removeEventListener('keydown',key,true); },[onClose,busy]);
  const patch = (p: Partial<ExerciseItem>) => setX((v) => ({ ...v, ...p }));
  const toggleMuscle=(key:'p'|'s', m:string)=>patch({[key]:x[key].includes(m)?x[key].filter(v=>v!==m):[...x[key],m]});
  async function readFile(f:File) {
    setError('');
    if (!['image/png','image/jpeg','image/webp','image/gif'].includes(f.type) || f.size>2*1024*1024) {setError('Выбери PNG, JPEG, WebP или GIF до 2 МБ.');return;}
    setReading(true);const r=new FileReader();reader.current=r;
    r.onload=async()=>{const media=String(r.result);const img=new Image();img.src=media;try{await img.decode();patch({media,mediaType:f.type==='image/gif'?'animation':'illustration'});}catch{setError('Изображение повреждено или не поддерживается. Выбери другой файл.');}finally{setReading(false);}};
    r.onerror=()=>{setError('Не удалось прочитать файл.');setReading(false);};r.readAsDataURL(f);
  }
  async function save() {
    if(!x.ru.trim()){setError('Укажи название.');return;}
    if(!x.purpose?.length){setError('Выбери хотя бы одну категорию.');return;}
    setBusy(true);setError('');
    try{const saved=await api.exerciseCustomSave({...x,weightMeaning:x.side?'per_side':'total',cat:x.purpose.includes('mobility')&&!x.purpose.includes('strength')?'stretching':'strength',st:x.measurement==='seconds'});refreshExerciseLib();onSaved(saved);}catch(e){setError(errText(e));}finally{setBusy(false);}
  }
  return <div className="ex-modal" role="dialog" aria-modal="true" aria-label={initial?'Изменить упражнение':'Добавить упражнение'} onClick={()=>{if(!busy)onClose();}}>
    <form className="card ex-editor" onClick={e=>e.stopPropagation()} onSubmit={e=>{e.preventDefault();if(!reading&&!busy)void save();}}>
      <div className="between"><b>{initial?'Изменить упражнение':'Добавить упражнение'}</b><button type="button" className="xbtn" aria-label="Закрыть редактор" disabled={busy} onClick={onClose}>×</button></div>
      <label className="stack small">Название<input autoFocus className="input" maxLength={120} value={x.ru} onChange={e=>patch({ru:e.target.value})}/></label>
      <label className="stack small">Инвентарь<select aria-label="Инвентарь" className="input" value={x.eq??'body only'} onChange={e=>patch({eq:e.target.value})}>{equipment.map(k=><option key={k} value={k}>{EQUIP[k]??k}</option>)}</select></label>
      <span className="small">Категории</span><div className="row">{purposes.map(([k,n])=><button type="button" key={k} className="pill sm" aria-pressed={x.purpose?.includes(k)??false} onClick={()=>patch({purpose:x.purpose?.includes(k)?x.purpose.filter(v=>v!==k):[...(x.purpose??[]),k]})}>{n}</button>)}</div>
      <label className="stack small">Способ учёта<select aria-label="Способ учёта" className="input" value={x.measurement??(x.st?'seconds':'reps')} onChange={e=>patch({measurement:e.target.value})}><option value="reps">Повторения</option><option value="seconds">Время, секунды</option><option value="contacts">Контакты с полом</option></select></label>
      <label className="stack small">Выполнение по сторонам<select aria-label="Выполнение по сторонам" className="input" value={x.side??''} onChange={e=>patch({side:(e.target.value||null) as ExerciseItem['side'],leg:!!e.target.value})}><option value="">Обе стороны вместе</option><option value="arm">На каждую руку</option><option value="leg">На каждую ногу</option><option value="side">На каждую сторону</option></select></label>
      <span className="muted tiny">При выполнении на каждую руку или ногу указывай вес снарядов на одну сторону; при одновременной работе обеих сторон — общий вес. Ленту выбирай в инвентаре: её нагрузка приблизительна.</span>
      {(['p','s'] as const).map(k=><fieldset key={k} className="ex-muscle-field"><legend className="small">{k==='p'?'Основные мышцы':'Дополнительные мышцы'}</legend><div className="row">{Object.entries(MUSCLE).map(([m,n])=><button key={m} type="button" className="pill sm" aria-pressed={x[k].includes(m)} onClick={()=>toggleMuscle(k,m)}>{n}</button>)}</div></fieldset>)}
      <span className="muted tiny">Мышцы можно указать позже. Без них оценка мышечной нагрузки будет неполной.</span>
      <label className="stack small">Описание техники<textarea className="input" rows={3} maxLength={6000} value={x.tip} onChange={e=>patch({tip:e.target.value})}/></label>
      <div className="row"><button type="button" className="btn2" onClick={()=>file.current?.click()} disabled={reading}>Картинка или GIF</button><span className="muted tiny">PNG, JPEG, WebP, GIF · до 2 МБ</span><input ref={file} type="file" accept="image/png,image/jpeg,image/webp,image/gif" hidden onChange={e=>{const f=e.target.files?.[0];if(f)void readFile(f);e.target.value='';}}/>{x.media&&<button type="button" className="linkbtn small" onClick={()=>patch({media:null,mediaType:null})}>Убрать картинку</button>}</div>
      {x.media&&<img className="ex-custom-preview" src={x.media} alt="Предпросмотр упражнения"/>}
      {error&&<span className="small note-err" role="alert">{error}</span>}
      <div className="row"><button className="btn" disabled={busy||reading}>{busy?'Сохраняю…':'Сохранить упражнение'}</button><button type="button" className="btn2" disabled={busy} onClick={onClose}>Отмена</button></div>
    </form>
  </div>;
}
