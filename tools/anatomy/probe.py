# Lists what is inside Z-Anatomy's Startup.blend (collections, object counts, muscle-ish names).
import bpy, sys
bpy.ops.wm.open_mainfile(filepath=sys.argv[-1])
def walk(c, d=0):
    objs = [o for o in c.objects]
    print("  " * d + f"[{c.name}] objs={len(objs)} meshes={sum(1 for o in objs if o.type=='MESH')}")
    for ch in c.children:
        walk(ch, d + 1)
walk(bpy.context.scene.collection)
names = [o.name for o in bpy.data.objects if o.type == 'MESH']
print("TOTAL MESH", len(names))
for k in ["Quadriceps", "Rectus femoris", "Vastus", "Gastrocnemius", "Gluteus", "Pectoralis", "Deltoid", "Biceps", "Latissimus", "Trapezius", "Rectus abdominis", "Skin", "skin"]:
    print(k, [n for n in names if k.lower() in n.lower()][:8])
print("SCENES", [s.name for s in bpy.data.scenes], "ENGINE", bpy.context.scene.render.engine)
