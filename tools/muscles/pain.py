# Pain figure (1.3.1): the same drawing as the muscle map, cut into pain areas of body_svc::AREAS with
# left and right sides -> public/muscles/pain.png (value = index + 1 in KEYS below, 0 = not an area).
# Built from the muscle masks of make.py plus hand outlines for neck, shin, ankle, foot, hip joint and
# the back of the knee. Coordinates are in the source drawing (tools/muscles/src.png, 1272 px wide).
# Run from the repo root after make.py.
from PIL import Image, ImageDraw
import numpy as np
from scipy import ndimage

base = Image.open('public/muscles/base.webp')
W, H = base.size
S = W / 1272
body = np.asarray(base)[..., 3] > 40
yy, xx = np.mgrid[0:H, 0:W]
FRONT_MID, BACK_MID, SPLIT = 343 * S, 920 * S, 636 * S
front = xx < SPLIT


def m(k):
    return np.asarray(Image.open(f'public/muscles/m_{k}.png'))[..., 1] > 110


def poly(pts):
    im = Image.new('L', (W, H), 0)
    ImageDraw.Draw(im).polygon([(x * S, y * S) for x, y in pts], fill=255)
    return np.asarray(im) > 0


def circ(cx, cy, r):
    return (xx - cx * S) ** 2 + (yy - cy * S) ** 2 < (r * S) ** 2


A = {}
A['neck'] = (poly([(302, 165), (384, 165), (392, 214), (296, 214)]) | poly([(880, 160), (960, 160), (968, 210), (872, 210)])) & body
A['shoulders'] = m('shoulders')
A['chest'] = m('chest')
A['arms'] = m('arms')
A['core'] = m('core')
A['back'] = m('back')
A['glutes'] = m('glutes')
A['hips'] = (m('hips') | circ(258, 572, 26) | circ(428, 572, 26)) & front & body
A['quads'] = m('quads')
A['hams'] = m('hams')
A['knees'] = (m('knees') | circ(853, 842, 20) | circ(995, 842, 20)) & body
A['calves'] = m('calves') & ~front
A['shin'] = (poly([(236, 858), (300, 858), (298, 1072), (262, 1072)]) | poly([(386, 858), (458, 858), (432, 1072), (396, 1072)])) & body
A['ankles'] = ((m('ankles') & ~front) | poly([(255, 1070), (302, 1070), (304, 1112), (252, 1112)]) | poly([(392, 1070), (438, 1070), (442, 1112), (390, 1112)])) & body
A['foot'] = (poly([(218, 1110), (306, 1110), (306, 1190), (216, 1190)]) | poly([(386, 1110), (472, 1110), (474, 1190), (384, 1190)])
             | poly([(792, 1105), (882, 1105), (880, 1180), (790, 1180)]) | poly([(962, 1105), (1050, 1105), (1050, 1180), (960, 1180)])) & body

# Later areas win where they overlap (joints over muscles).
ORDER = ['back', 'core', 'chest', 'shoulders', 'arms', 'neck', 'glutes', 'hams', 'quads', 'hips', 'calves', 'shin', 'knees', 'ankles', 'foot']
MIDDLE = {'neck', 'core', 'back'}
KEYS = []
for a in ORDER:
    KEYS += [f'{a}:'] if a in MIDDLE else [f'{a}:R', f'{a}:L']
lbl = np.zeros((H, W), np.uint8)
for a in ORDER:
    mask = ndimage.binary_opening(A[a], iterations=1)
    if a in MIDDLE:
        lbl[mask] = KEYS.index(f'{a}:') + 1
        continue
    # On the front view the athlete's right is on the left of the picture; on the back view it is on the right.
    pic_left = np.where(front, xx < FRONT_MID, xx < BACK_MID)
    right = np.where(front, pic_left, ~pic_left)
    lbl[mask & right] = KEYS.index(f'{a}:R') + 1
    lbl[mask & ~right] = KEYS.index(f'{a}:L') + 1
Image.fromarray(lbl).save('public/muscles/pain.png', optimize=True)
print(len(KEYS), KEYS)
for a in ORDER:
    print(a, int(A[a].sum()))
