#!/usr/bin/env python3
"""Exercise library for Kouros (0.22).

Source: Free Exercise DB (https://github.com/yuhonas/free-exercise-db), public domain (Unlicense).
Russian names, synonyms and short how-to lines: tools/exercises/ru.tsv (written for Kouros).

Writes
  src-tauri/resources/exercises.json.gz  — the library the app reads (names, muscles, equipment,
                                            body-weight share, static or not, how-to)
  public/ex/<id>.webp                    — both photos of the exercise side by side (start | end),
                                            shown as a two-frame animation

Usage: python3 tools/exercises/build.py /path/to/free-exercise-db
"""
import gzip, io, json, os, re, sys
from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SRC = sys.argv[1] if len(sys.argv) > 1 else '/tmp/fedb'
W = 360  # width of one frame

def ru_rows():
    out = {}
    for line in open(os.path.join(ROOT, 'tools/exercises/ru.tsv'), encoding='utf-8'):
        if line.startswith('#') or not line.strip():
            continue
        id_, ru, syn, tip = line.rstrip('\n').split('\t')
        out[id_] = (ru, [s.strip() for s in syn.split(';') if s.strip()], tip)
    return out

# Share of body weight that is moved, for exercises done with your own weight. Approximate,
# from force-plate studies (push-up ≈ 64%, knee push-up ≈ 49%, feet elevated ≈ 70–74%, …).
# (pattern on the English name, share). First match wins.
BW = [
    (r'band assisted pull', 0.95),
    (r'handstand push', 0.9),
    (r'knee.*push|push.*knee', 0.49),
    (r'incline push', 0.55),
    (r'decline push|feet elevated|feet on an exercise ball|push-ups with feet', 0.72),
    (r'push-?up|pushup|drop push|clock push', 0.64),
    (r'muscle up|chin|pull-?up|pullup|rope climb|london bridges', 0.95),
    (r'bench dip', 0.6),
    (r'dip', 0.9),
    (r'inverted row|suspended row|bodyweight mid row', 0.55),
    (r'pistol|single-leg high box squat|one leg barbell', 0.85),
    (r'squat|lunge|step-?up|split squat|split jump', 0.8),
    (r'hanging leg raise|hanging pike|knee/hip raise|gorilla', 0.3),
    (r'glute ham|natural glute|floor glute', 0.6),
    (r'hyperextension|superman|back extension', 0.5),
    (r'bridge|hip raise|butt lift|hip lift|hip thrust|butt-ups', 0.5),
    (r'sit-?up|crunch|jackknife|v-up|toe touch|heel touch|elbow to knee|cocoons|russian twist|air bike|tuck', 0.3),
    (r'leg raise|leg lift|flutter|scissor|leg pull-in|bottoms up|dead bug', 0.2),
    (r'body-?up|body tricep', 0.6),
]
STATIC = re.compile(r'plank|side bridge|hold|handed hang|wall sit|isometric|vacuum|crucifix|wipers|pinch', re.I)
BODY_EQ = {None, 'body only', 'other', 'exercise ball'}

def bw_share(x):
    if x.get('category') not in ('strength', 'plyometrics', 'powerlifting', 'strongman'):
        return None
    if x.get('equipment') not in BODY_EQ and 'Band Assisted' not in x['name']:
        return None
    n = x['name'].lower()
    if ('jump' in n or 'hop' in n) and 'squat' not in n:
        return None
    for pat, k in BW:
        if re.search(pat, n):
            return k
    return None

def frames(x):
    ims = []
    for p in x['images'][:2]:
        im = Image.open(os.path.join(SRC, 'exercises', p)).convert('RGB')
        im.thumbnail((W, W))
        ims.append(im)
    if not ims:
        return None
    h = max(i.height for i in ims)
    out = Image.new('RGB', (W * len(ims), h), (255, 255, 255))
    for k, im in enumerate(ims):
        out.paste(im, (k * W + (W - im.width) // 2, (h - im.height) // 2))
    return out

def main():
    data = json.load(open(os.path.join(SRC, 'dist/exercises.json')))
    data.sort(key=lambda x: x['id'])
    ru = ru_rows()
    os.makedirs(os.path.join(ROOT, 'public/ex'), exist_ok=True)
    items, size = [], 0
    for x in data:
        r = ru.get(x['id'])
        if not r:
            print('no Russian row for', x['id'])
            continue
        img = frames(x)
        nf = 0
        if img is not None:
            b = io.BytesIO()
            img.save(b, 'WEBP', quality=55, method=6)
            open(os.path.join(ROOT, 'public/ex', x['id'] + '.webp'), 'wb').write(b.getvalue())
            size += b.tell()
            nf = len(x['images'][:2])
        bw = bw_share(x)
        st = bool(STATIC.search(x['name']))
        items.append({
            'id': x['id'], 'ru': r[0], 'en': x['name'], 'syn': r[1], 'tip': r[2],
            'cat': x.get('category'), 'eq': x.get('equipment'), 'lvl': x.get('level'), 'mech': x.get('mechanic'), 'force': x.get('force'),
            'p': x.get('primaryMuscles', []), 's': x.get('secondaryMuscles', []), 'bw': bw, 'st': st, 'img': nf,
        })
    out = {'source': 'Free Exercise DB (public domain) · русские названия Kouros', 'version': '2026-10', 'items': items}
    with gzip.open(os.path.join(ROOT, 'src-tauri/resources/exercises.json.gz'), 'wt', encoding='utf-8') as f:
        json.dump(out, f, ensure_ascii=False, separators=(',', ':'))
    print(len(items), 'exercises,', round(size / 1e6, 1), 'MB of images,', sum(1 for i in items if i['bw']), 'with body weight,', sum(1 for i in items if i['st']), 'static')

if __name__ == '__main__':
    main()
