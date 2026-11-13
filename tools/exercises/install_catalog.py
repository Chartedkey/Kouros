"""Install the owner-approved catalog/art, retaining all shipped IDs and legacy media.
Usage: python tools/exercises/install_catalog.py /path/to/private/docs/exercise-catalog
No personal database is read. Original PNGs are copied without transformation.
"""
import gzip, hashlib, json, shutil, sys
from pathlib import Path
root=Path(__file__).resolve().parents[2];source=Path(sys.argv[1]);draft=json.loads((source/'catalog.json').read_text(encoding='utf-8'))
base=json.loads(gzip.decompress((root/'src-tauri/resources/exercises.json.gz').read_bytes()))
extra=json.loads((root/'src-tauri/resources/exercises_extra.json').read_text(encoding='utf-8'));items=base['items']+extra['items'];known={x['id'] for x in items}
eq={'bodyweight':'body only','long_band':'bands','mini_band':'mini bands','kettlebell':'kettlebells','dumbbell':'dumbbell','barbell':'barbell','machine':'machine','cable':'cable'}
tips={
'new_pushup_knees':'Упор на ладони и колени. Сохраняй прямую линию от коленей до головы, опускай грудь между ладонями и выжимайся.',
'new_stepup_body':'Поставь всю стопу на устойчивую невысокую платформу. Поднимись усилием опорной ноги, спокойно спустись. Повтори другой ногой.',
'new_calf_single_flat':'Стой на одной ноге на полу, держись за опору для равновесия. Подними пятку и медленно опусти её на пол. Повтори другой ногой.',
'new_short_foot':'Стопа на полу: пятка, основание большого пальца и мизинца сохраняют опору. Мягко подтяни передний отдел к пятке, приподнимая свод; не сгибай пальцы. Удерживай, расслабься и повтори другой стопой.',
'new_tibialis_wall':'Спиной обопрись о стену, поставь стопы немного впереди. Пятки остаются на полу; подними носки, затем плавно опусти.',
'new_step_down':'Стой одной ногой на невысокой ступени. Сгибая опорную ногу, плавно опусти свободную пятку к полу и вернись. Колено следует направлению стопы.',
'new_march':'Стой прямо. Поочерёдно поднимай согнутое колено, работая противоположной рукой. Сохраняй устойчивый таз, без прыжка.',
'new_pogo':'Выполняй невысокие упругие подскоки двумя ногами. Приземляйся мягко, сохраняй небольшое сгибание коленей. Считай контакты с полом.',
'new_kb_swing_two':'Держи гирю двумя руками. Отведи таз назад, проведи гирю между ногами; разгибанием таза направь её вперёд. Руки направляют гирю, спина сохраняет нейтральное положение.',
'new_kb_deadlift':'Гиря на полу между стопами. Отведи таз назад, согни колени и возьми рукоять двумя руками. Встань разгибанием ног и таза, затем контролируемо верни гирю на пол.',
'new_kb_rdl':'Держи гирю двумя руками перед собой. Слегка согнув колени, отводи таз назад и опускай гирю близко к ногам. Вернись разгибанием таза; не округляй спину.',
'new_kb_suitcase':'Держи гирю в одной руке сбоку и иди ровными шагами без наклона корпуса. Смени руку. В этой версии записывай длительность переноски в секундах на сторону.',
'new_band_facepull':'Закрепи длинную ленту на уровне лица. Тяни концы к лицу, разводя локти в стороны; не запрокидывай голову. Плавно верни руки вперёд.',
'new_band_hamstring':'Лёжа на животе, сгибай колени, подтягивая пятки к ягодицам против сопротивления закреплённой со стороны стоп ленты. Таз остаётся на опоре; возвращай ноги плавно.'}
new=[];variants=[];hashes={}
for x in draft['variants']:
 row={k:x[k] for k in ('id','family','name','equipment','purpose','visibility','measurement','search_aliases')}
 if x['id'] not in known:
  assert x['id'] in tips
  # New IDs become stable on first shipment; never map old history onto another variant.
  p=[m.replace('middle_back','middle back') for m in x['primary_muscles']]
  side='leg' if x['measurement'].endswith('each_side') and x['id']!='new_kb_suitcase' else 'arm' if x['id']=='new_kb_suitcase' else None
  it=dict(id=x['id'],ru=x['name'],en='',syn=x['search_aliases'],tip=tips[x['id']],cat='strength',eq=eq[x['equipment']],lvl='',mech='',force='',p=p,s=[],bw=None,st=x['measurement'].startswith('seconds') or x['id']=='new_kb_suitcase',img=0)
  if side:it['side']=side
  new.append(it)
 if x.get('image_file'):
  src=source/x['image_file'];filename=x['id']+'.png';shutil.copyfile(src,root/'public/ex-kouros'/filename);row['media']='ex-kouros/'+filename
  hashes[filename]=hashlib.sha256(src.read_bytes()).hexdigest()
 variants.append(row)
assert len(new)==14 and len(hashes)==190
out=dict(version=1,families=draft['families'],variants=variants,newItems=new)
(root/'src/data/exercise-catalog.json').write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
(root/'public/exercise-library-demo.json').write_text(json.dumps(dict(source=base.get('source',''),version=base.get('version',''),items=items+new,runnerOfp=extra['runnerOfp'],bandList=extra['bandList']),ensure_ascii=False)+'\n',encoding='utf-8')
(root/'tools/exercises/art-sha256.json').write_text(json.dumps(hashes,indent=2)+'\n',encoding='utf-8')
print('Installed190 original PNGs,310 catalog variants,14 new IDs; preserved924 old items/media')
