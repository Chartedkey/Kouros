# 1.4.1: workout icons (drawn by a friend of the owner, given for the app) → public/sport/<kind>.webp.
# The source sheets in tools/icons/src hold rounded tiles on a transparent background; each tile is
# found as a blob of the alpha channel, cropped square with a margin and saved at 256 px.
# Run from the repo root.
from PIL import Image
import numpy as np
from scipy import ndimage
import os

SHEETS = [
    ('tools/icons/src/set1_yoga_ofp_bike_treadmill_elliptical_stepper_indoorbike.png',
     ['yoga', 'workout', 'ride', 'treadmill', 'elliptical', 'stepper', 'ride_virtual']),
    ('tools/icons/src/set2_walk_run_strength.png', ['walk', 'run', 'strength']),
    ('tools/icons/src/set3_hike_swim_other.png', ['hike', 'swim', 'other']),
]
os.makedirs('public/sport', exist_ok=True)
for path, keys in SHEETS:
    im = Image.open(path).convert('RGBA')
    a = np.asarray(im)[..., 3] > 60
    lbl, n = ndimage.label(ndimage.binary_closing(a, iterations=3))
    boxes = [s for s in ndimage.find_objects(lbl)]
    boxes = [b for b in boxes if (b[0].stop - b[0].start) > im.height * 0.15]
    # Reading order: rows from top (within 15 % of the tile height), then left to right.
    h = max(b[0].stop - b[0].start for b in boxes)
    boxes.sort(key=lambda b: (round(b[0].start / (h * 0.5)), b[1].start))
    assert len(boxes) == len(keys), (path, len(boxes))
    for k, b in zip(keys, boxes):
        y0, y1, x0, x1 = b[0].start, b[0].stop, b[1].start, b[1].stop
        side = max(y1 - y0, x1 - x0)
        cx, cy = (x0 + x1) // 2, (y0 + y1) // 2
        m = int(side * 0.04)
        half = side // 2 + m
        crop = im.crop((cx - half, cy - half, cx + half, cy + half)).resize((256, 256), Image.LANCZOS)
        crop.save(f'public/sport/{k}.webp', 'WEBP', quality=88, method=6)
        print(k, side)
