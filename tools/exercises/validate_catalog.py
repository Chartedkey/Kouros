"""Validate installed art and built-in catalog without accessing personal data."""
import gzip, hashlib, json, struct
from pathlib import Path
r=Path(__file__).resolve().parents[2]
c=json.loads((r/'src/data/exercise-catalog.json').read_text(encoding='utf-8'))
base=json.loads(gzip.decompress((r/'src-tauri/resources/exercises.json.gz').read_bytes()))
extra=json.loads((r/'src-tauri/resources/exercises_extra.json').read_text(encoding='utf-8'))
old=base['items']+extra['items'];new=c['newItems'];all_items={x['id']:x for x in old+new}
assert len(old)==924 and len(new)==14 and len(all_items)==938
assert len(c['variants'])==310 and len({v['id'] for v in c['variants']})==310
primary=[v for v in c['variants'] if v['visibility']=='primary']
assert len(primary)==190 and all(v['id'] in all_items for v in c['variants'])
hashes=json.loads((r/'tools/exercises/art-sha256.json').read_text(encoding='utf-8'))
assert len(hashes)==190
assert len({name.casefold() for name in hashes})==190 # Windows file-name collisions
for v in primary:
 p=r/'public'/v['media'];assert p.is_file();data=p.read_bytes();assert hashlib.sha256(data).hexdigest()==hashes[p.name];assert struct.unpack('>II',data[16:24])==(1536,1024)
assert len({v['family'] for v in primary})==78
assert set(extra['runnerOfp']) <= {v['id'] for v in c['variants']}
assert set(extra['bandList']) <= {v['id'] for v in c['variants']}
versions=[json.loads((r/p).read_text(encoding='utf-8'))['version'] for p in ['package.json','src-tauri/tauri.conf.json']]
assert versions==['1.7.0','1.7.0']
print('PASS: 190 illustrations,78 primary families,310 variants,924 preserved IDs+14 new,old OFP/band references,desktop1.7.0')
