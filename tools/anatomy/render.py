# Grey render of Z-Anatomy muscles (+ bones where no muscle covers them), front and back.
# Z-Anatomy — The libre 3D atlas of anatomy — CC BY-SA 4.0; BodyParts3D, DBCLS — CC BY-SA 2.1 JP.
import bpy, sys, math, os
from mathutils import Vector
bpy.ops.wm.open_mainfile(filepath=sys.argv[-1])
sc = bpy.context.scene
OUT = os.path.join(os.path.dirname(__file__), 'out')
SKIP = ('.ol', '.or', '.el', '.er', '.ot', '.et')

def coll(name):
    for c in bpy.data.collections:
        if c.name == name:
            return c
musc = coll('4: Muscular system'); skel = coll('1: Skeletal system')
want = {}
for c, kind in ((musc, 'm'), (skel, 'b')):
    for o in c.all_objects:
        if o.type == 'MESH' and not o.name.endswith(SKIP) and 'bursa' not in o.name.lower():
            want[o.name] = kind
print('selected', sum(1 for v in want.values() if v == 'm'), 'muscles', sum(1 for v in want.values() if v == 'b'), 'bones')

def unexclude(lc):
    lc.exclude = False
    lc.hide_viewport = False
    for ch in lc.children:
        unexclude(ch)
for lc in bpy.context.view_layer.layer_collection.children:
    if lc.name in ('4: Muscular system', '1: Skeletal system'):
        unexclude(lc)
    else:
        lc.exclude = True
for n in want:
    o = bpy.data.objects[n]
    mw = o.matrix_world.copy()
    o.parent = None
    o.matrix_world = mw
drop = [o for o in bpy.data.objects if o.name not in want]
for o in drop:
    bpy.data.objects.remove(o, do_unlink=True)
for _ in range(3):
    bpy.ops.outliner.orphans_purge(do_local_ids=True, do_linked_ids=True, do_recursive=True)
print('left objects', len(bpy.data.objects), flush=True)
for o in bpy.data.objects:
    o.hide_render = False
    o.hide_viewport = False
    for m in list(o.modifiers):
        o.modifiers.remove(m)

def mat(name, rgb, rough):
    m = bpy.data.materials.new(name); m.use_nodes = True
    p = m.node_tree.nodes.get('Principled BSDF')
    p.inputs['Base Color'].default_value = (*rgb, 1)
    p.inputs['Roughness'].default_value = rough
    return m
MM = mat('kmus', (0.42, 0.43, 0.45), 0.5)
MB = mat('kbone', (0.78, 0.78, 0.76), 0.6)
for n, k in want.items():
    o = bpy.data.objects[n]
    o.data.materials.clear() if o.data.users == 1 else None
    if o.data.users > 1:
        o.data = o.data.copy(); o.data.materials.clear()
    o.data.materials.append(MM if k == 'm' else MB)

# bounds
lo = Vector((1e9, 1e9, 1e9)); hi = Vector((-1e9, -1e9, -1e9))
for n in want:
    o = bpy.data.objects[n]
    for v in o.bound_box:
        w = o.matrix_world @ Vector(v)
        lo = Vector(map(min, lo, w)); hi = Vector(map(max, hi, w))
print('bounds', tuple(round(x, 3) for x in lo), tuple(round(x, 3) for x in hi))
c = (lo + hi) / 2; size = hi - lo

sc.render.engine = 'CYCLES'
sc.cycles.device = 'CPU'
sc.cycles.samples = 24
sc.cycles.use_denoising = True
sc.render.film_transparent = True
H = 1200
sc.render.resolution_y = H
sc.render.resolution_x = int(H * (size.x * 1.12) / (size.z * 1.06))
sc.render.resolution_percentage = 100
sc.render.image_settings.file_format = 'PNG'
sc.render.image_settings.color_mode = 'RGBA'
w = bpy.data.worlds.new('kw'); sc.world = w; w.use_nodes = True
w.node_tree.nodes['Background'].inputs['Color'].default_value = (0.05, 0.05, 0.06, 1)
w.node_tree.nodes['Background'].inputs['Strength'].default_value = 0.6

cam_d = bpy.data.cameras.new('kcam'); cam_d.type = 'ORTHO'; cam_d.ortho_scale = size.z * 1.06
cam = bpy.data.objects.new('kcam', cam_d); sc.collection.objects.link(cam); sc.camera = cam
def light(name, loc, energy, size_):
    d = bpy.data.lights.new(name, 'AREA'); d.energy = energy; d.size = size_
    o = bpy.data.objects.new(name, d); sc.collection.objects.link(o); o.location = loc
    o.rotation_euler = (c - Vector(loc)).to_track_quat('-Z', 'Y').to_euler()
    return o
dist = max(size) * 3
for side, sgn in (('front', -1), ('back', 1)):
    cam.location = (c.x, c.y + sgn * dist, c.z)
    cam.rotation_euler = (math.pi / 2, 0, 0 if sgn < 0 else math.pi)
    for o in [o for o in sc.collection.objects if o.name.startswith('kl')]:
        bpy.data.objects.remove(o)
    s = max(size)
    light('kl_key', (c.x - s * 0.6, c.y + sgn * s * 1.2, c.z + s * 0.6), 2500 * s * s, s * 0.8)
    light('kl_fill', (c.x + s * 0.8, c.y + sgn * s * 1.0, c.z), 900 * s * s, s)
    light('kl_rim', (c.x, c.y - sgn * s * 1.2, c.z + s * 0.5), 1500 * s * s, s * 0.6)
    sc.render.filepath = os.path.join(OUT, f'{side}.png')
    bpy.ops.render.render(write_still=True)
    print('rendered', side, flush=True)
