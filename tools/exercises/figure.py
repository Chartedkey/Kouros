# 1.4.2: a small figure renderer for Kouros' own exercises (no photos in Free Exercise DB):
# a plain "mannequin" drawn from a pose — pelvis, spine direction and hand / foot targets solved
# by two-bone IK — with the working muscles tinted, the band in orange, optional props
# (anchor post, box, bench, mat) and arrows. Two poses → start | end frame, like the library photos.
import math

SK, SK2, OL = '#D3D7DD', '#B9BFC8', '#7E8590'
MU, MUL = '#FF6A4D', '#D9472E'
BD, BDL = '#F28C28', '#C96A12'
SH = '#3A404A'
L = dict(torso=105, neck=14, head=19, up=60, fore=54, thigh=80, shin=78, foot=24)

def vec(a, l=1.0):
    r = math.radians(a)
    return (math.sin(r) * l, math.cos(r) * l)

def ang(dx, dy):
    return math.degrees(math.atan2(dx, dy))

def add(p, q):
    return (p[0] + q[0], p[1] + q[1])

def ik(base, target, a, b, bend, hint=None):
    """Two-bone IK: returns (mid joint, end). bend = +1 / -1 picks the side of the bend;
    a hint point picks the side whose middle joint is closer to it."""
    if hint is not None:
        m1, e1 = ik(base, target, a, b, 1)
        m2, e2 = ik(base, target, a, b, -1)
        d1 = (m1[0] - hint[0]) ** 2 + (m1[1] - hint[1]) ** 2
        d2 = (m2[0] - hint[0]) ** 2 + (m2[1] - hint[1]) ** 2
        return (m1, e1) if d1 <= d2 else (m2, e2)
    dx, dy = target[0] - base[0], target[1] - base[1]
    d = max(1e-3, min(math.hypot(dx, dy), a + b - 0.5))
    base_ang = ang(dx, dy)
    cosA = (a * a + d * d - b * b) / (2 * a * d)
    A = math.degrees(math.acos(max(-1, min(1, cosA))))
    mid = add(base, vec(base_ang + bend * A, a))
    end = add(base, vec(base_ang, d))
    return mid, end

class Pose:
    """view 'front' (athlete faces the viewer) or 'side' (faces right). Coordinates: y down,
    floor at y = 0. pelvis = point; neck = point the spine points to (length is forced).
    hands / feet: dict side -> target (side 'l' / 'r' = left / right on the picture; in the side
    view 'r' is the near limb). bends: dict limb -> ±1. fs: foreshortening per segment name."""
    def __init__(self, view, pelvis, neck, hands=None, feet=None, bends=None, fs=None, flip=False, head=None, foot_ang=None, knees=None, elbows=None):
        self.view, self.flip = view, flip
        self.pelvis = pelvis
        s = ang(neck[0] - pelvis[0], neck[1] - pelvis[1])
        self.spine = s
        fs0 = fs or {}
        self.fs0 = fs0
        self.neck = add(pelvis, vec(s, L['torso'] * fs0.get('torso', 1)))
        self.head_dir = head if head is not None else s
        self.head = add(self.neck, vec(self.head_dir, L['neck'] + L['head']))
        n = vec(s - 90)  # sideways direction of the body (towards the picture right)
        self.side = n
        fs = fs or {}
        bends = bends or {}
        foot_ang = foot_ang or {}
        self.j = {}
        for k, sgn in (('l', -1), ('r', 1)):
            if view == 'front':
                sh = add(self.neck, (n[0] * 31 * sgn + vec(s, -8)[0], n[1] * 31 * sgn + vec(s, -8)[1]))
                hip = add(pelvis, (n[0] * 14 * sgn, n[1] * 14 * sgn))
            else:
                sh = add(self.neck, vec(s, -8))
                hip = pelvis
            self.j['sh_' + k], self.j['hip_' + k] = sh, hip
            ht = (hands or {}).get(k) or add(sh, vec((s + 180) + (10 * sgn if view == 'front' else 4), 113.5))
            ft = (feet or {}).get(k) or add(hip, vec(s + 180 + (3 * sgn if view == 'front' else 0), 157.6))
            ua, fa = L['up'] * fs.get('up_' + k, 1), L['fore'] * fs.get('fore_' + k, 1)
            ta, sa = L['thigh'] * fs.get('thigh_' + k, 1), L['shin'] * fs.get('shin_' + k, 1)
            self.j['el_' + k], self.j['hand_' + k] = ik(sh, ht, ua, fa, bends.get('arm_' + k, -sgn if view == 'front' else -1), (elbows or {}).get(k))
            self.j['knee_' + k], self.j['ankle_' + k] = ik(hip, ft, ta, sa, bends.get('leg_' + k, sgn if view == 'front' else 1), (knees or {}).get(k))
            if view == 'side':
                sh_ang = ang(self.j['ankle_' + k][0] - self.j['knee_' + k][0], self.j['ankle_' + k][1] - self.j['knee_' + k][1])
                self.j['toe_' + k] = add(self.j['ankle_' + k], vec(sh_ang + foot_ang.get(k, 90), L['foot']))
            else:
                self.j['toe_' + k] = add(self.j['ankle_' + k], (sgn * 6, 6))
                self.foot_rot = getattr(self, 'foot_rot', {})
                self.foot_rot[k] = foot_ang.get(k, 0)
        self.j['pelvis'], self.j['neck'], self.j['head'] = pelvis, self.neck, self.head

    def points(self):
        ps = list(self.j.values())
        ps += [(self.head[0] - L['head'], self.head[1] - L['head']), (self.head[0] + L['head'], self.head[1] + L['head'])]
        return ps

def _seg(a, b, w, fill, ol=OL):
    d = f'M{a[0]:.1f},{a[1]:.1f} L{b[0]:.1f},{b[1]:.1f}'
    return (f'<path d="{d}" stroke="{ol}" stroke-width="{w + 4}" stroke-linecap="round"/>'
            f'<path d="{d}" stroke="{fill}" stroke-width="{w}" stroke-linecap="round"/>')

def _tint(a, b, w, t0=0.12, t1=0.88):
    p = (a[0] + (b[0] - a[0]) * t0, a[1] + (b[1] - a[1]) * t0)
    q = (a[0] + (b[0] - a[0]) * t1, a[1] + (b[1] - a[1]) * t1)
    return f'<path d="M{p[0]:.1f},{p[1]:.1f} L{q[0]:.1f},{q[1]:.1f}" stroke="{MU}" stroke-width="{w - 5}" stroke-linecap="round" opacity="0.95"/>'

def _ell(c, rx, ry, rot, fill=MU, stroke=MUL):
    return f'<ellipse cx="{c[0]:.1f}" cy="{c[1]:.1f}" rx="{rx}" ry="{ry}" transform="rotate({rot:.1f} {c[0]:.1f} {c[1]:.1f})" fill="{fill}" stroke="{stroke}" stroke-width="1.3"/>'

def draw(p, muscles, tf):
    """SVG of one pose. muscles: set of Free Exercise DB muscle names to tint."""
    j = p.j
    T = lambda q: (q[0] * tf[2] + tf[0], q[1] * tf[2] + tf[1])
    J = {k: T(v) for k, v in j.items()}
    sc = tf[2]
    W = {k: v * sc for k, v in dict(up=17, fore=14, thigh=24, shin=19, torso=46).items()}
    m = set(muscles)
    front_n = vec(p.spine - 90)  # chest side in the side view (faces right when upright)
    if p.flip:
        front_n = (-front_n[0], -front_n[1])
    out = []

    def leg(k, far=False):
        fill = SK2 if far else SK
        s = _seg(J['hip_' + k], J['knee_' + k], W['thigh'], fill) + _seg(J['knee_' + k], J['ankle_' + k], W['shin'], fill)
        if p.view == 'side':
            s += _seg(J['ankle_' + k], J['toe_' + k], 9 * sc, fill)
        else:
            t = J["toe_" + k]
            sole = p.fs0.get('shin_' + k, 1) < 0.5  # legs towards the viewer: we see the sole
            if sole:
                t = J["ankle_" + k]
            s += f'<ellipse cx="{t[0]:.1f}" cy="{t[1]:.1f}" rx="{(12 if sole else 11) * sc:.1f}" ry="{(22 if sole else 6) * sc:.1f}" transform="rotate({getattr(p, "foot_rot", {}).get(k, 0)} {t[0]:.1f} {t[1]:.1f})" fill="{fill}" stroke="{OL}" stroke-width="2"/>'
        if not far or p.view == 'front':
            if m & {'quadriceps', 'hamstrings', 'adductors', 'abductors'}:
                s += _tint(J['hip_' + k], J['knee_' + k], W['thigh'])
            if 'calves' in m:
                s += _tint(J['knee_' + k], J['ankle_' + k], W['shin'], 0.15, 0.7)
        return s

    def arm(k, far=False):
        fill = SK2 if far else SK
        s = _seg(J['sh_' + k], J['el_' + k], W['up'], fill) + _seg(J['el_' + k], J['hand_' + k], W['fore'], fill)
        s += f'<circle cx="{J["hand_" + k][0]:.1f}" cy="{J["hand_" + k][1]:.1f}" r="{8 * sc:.1f}" fill="{fill}" stroke="{OL}" stroke-width="2"/>'
        if not far or p.view == 'front':
            if m & {'biceps', 'triceps'}:
                s += _tint(J['sh_' + k], J['el_' + k], W['up'], 0.2, 0.85)
            if 'forearms' in m:
                s += _tint(J['el_' + k], J['hand_' + k], W['fore'], 0.15, 0.8)
            if 'shoulders' in m:
                s += _ell(J['sh_' + k], 12 * sc, 14 * sc, 0)
        return s

    def torso():
        a, b = J['pelvis'], J['neck']
        if p.view == 'front':
            n = p.side
            pts = [add(J['sh_l'], vec(p.spine + 180, -2)), add(J['sh_r'], (0, 0)), add(J['hip_r'], (n[0] * 8 * sc, n[1] * 8 * sc)), add(J['hip_l'], (-n[0] * 8 * sc, -n[1] * 8 * sc))]
            s = '<path d="M' + ' L'.join(f'{x:.1f},{y:.1f}' for x, y in pts) + f' Z" fill="{SK}" stroke="{OL}" stroke-width="2.5" stroke-linejoin="round"/>'
            # shorts
            lerp = lambda a_, b_, t: (a_[0] + (b_[0] - a_[0]) * t, a_[1] + (b_[1] - a_[1]) * t)
            kl, kr = lerp(J['hip_l'], J['knee_l'], 0.32), lerp(J['hip_r'], J['knee_r'], 0.32)
            mid = lerp(J['hip_l'], J['hip_r'], 0.5)
            crotch = lerp(mid, lerp(kl, kr, 0.5), 0.45)
            sh_pts = [add(J['hip_l'], (-n[0] * 15 * sc + vec(p.spine, 10 * sc)[0], -n[1] * 15 * sc + vec(p.spine, 10 * sc)[1])),
                      add(J['hip_r'], (n[0] * 15 * sc + vec(p.spine, 10 * sc)[0], n[1] * 15 * sc + vec(p.spine, 10 * sc)[1])),
                      add(kr, (n[0] * 12 * sc, n[1] * 12 * sc)), add(kr, (-n[0] * 11 * sc, -n[1] * 11 * sc)), crotch,
                      add(kl, (n[0] * 11 * sc, n[1] * 11 * sc)), add(kl, (-n[0] * 12 * sc, -n[1] * 12 * sc))]
            s += '<path d="M' + ' L'.join(f'{x:.1f},{y:.1f}' for x, y in sh_pts) + f' Z" fill="{SH}" stroke="{OL}" stroke-width="2" stroke-linejoin="round"/>'
            c = lambda t: (b[0] + (a[0] - b[0]) * t, b[1] + (a[1] - b[1]) * t)
            if 'chest' in m:
                s += _ell(add(c(0.25), (-n[0] * 14 * sc, -n[1] * 14 * sc)), 13 * sc, 10 * sc, p.spine) + _ell(add(c(0.25), (n[0] * 14 * sc, n[1] * 14 * sc)), 13 * sc, 10 * sc, p.spine)
            if 'abdominals' in m:
                s += _ell(c(0.62), 13 * sc, 22 * sc, -p.spine + 180 if False else p.spine * -1 + 180)
            if m & {'lats', 'middle back'}:
                s += _ell(add(c(0.35), (-n[0] * 26 * sc, -n[1] * 26 * sc)), 7 * sc, 20 * sc, -p.spine + 180) + _ell(add(c(0.35), (n[0] * 26 * sc, n[1] * 26 * sc)), 7 * sc, 20 * sc, -p.spine + 180)
            if 'abductors' in m or 'glutes' in m:
                s += _ell(add(J['hip_l'], (-n[0] * 10 * sc, -n[1] * 10 * sc)), 8 * sc, 13 * sc, -p.spine + 180) + _ell(add(J['hip_r'], (n[0] * 10 * sc, n[1] * 10 * sc)), 8 * sc, 13 * sc, -p.spine + 180)
            return s
        s = _seg(a, b, W['torso'] * 0.92, SK)
        s += _seg(a, add(a, vec(p.spine, 6 * sc)), W['torso'] * 0.95, SH)
        c = lambda t, off: (b[0] + (a[0] - b[0]) * t + front_n[0] * off * sc, b[1] + (a[1] - b[1]) * t + front_n[1] * off * sc)
        rot = -p.spine + 180
        if 'chest' in m:
            s += _ell(c(0.22, 11), 9 * sc, 15 * sc, rot)
        if 'abdominals' in m:
            s += _ell(c(0.62, 12), 7 * sc, 20 * sc, rot)
        if m & {'lats', 'middle back', 'traps'}:
            s += _ell(c(0.32, -12), 8 * sc, 22 * sc, rot)
        if 'lower back' in m:
            s += _ell(c(0.78, -13), 6 * sc, 13 * sc, rot)
        if 'glutes' in m:
            s += _ell(c(1.0, -14), 10 * sc, 13 * sc, rot)
        return s

    def head():
        h = J['head']
        return _seg(J['neck'], add(J['neck'], vec(p.head_dir, L['neck'] * sc)), 14 * sc, SK) + \
            f'<circle cx="{h[0]:.1f}" cy="{h[1]:.1f}" r="{L["head"] * sc:.1f}" fill="{SK}" stroke="{OL}" stroke-width="2.5"/>'

    if p.view == 'front':
        out += [leg('l'), leg('r'), torso(), head(), arm('l'), arm('r')]
    else:
        out += [arm('l', True), leg('l', True), torso(), head(), leg('r'), arm('r')]
    return ''.join(out), J

def band_svg(J, spec, tf, floor):
    """spec: ('line', [points]) — a long band through joints / ('rel', x, y) points;
    ('loop', a, b) — a mini band between two joints."""
    out = []
    T = lambda q: (q[0] * tf[2] + tf[0], q[1] * tf[2] + tf[1])
    def P(x):
        if isinstance(x, str):
            return J[x]
        if x[0] == 'rel':
            return T((x[1], x[2]))
        return x
    for s in spec:
        if s[0] == 'line':
            pts = [P(x) for x in s[1]]
            d = 'M' + ' L'.join(f'{x:.1f},{y:.1f}' for x, y in pts)
            out.append(f'<path d="{d}" fill="none" stroke="{BDL}" stroke-width="{6.5 * tf[2]:.1f}" stroke-linejoin="round" stroke-linecap="round"/>'
                       f'<path d="{d}" fill="none" stroke="{BD}" stroke-width="{4.5 * tf[2]:.1f}" stroke-linejoin="round" stroke-linecap="round"/>')
        elif s[0] == 'loop':
            a, b = P(s[1]), P(s[2])
            if (a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2 < 15 ** 2:
                # seen from the side: a ring around the near limb
                k = s[2]
                up = {'knee': 'hip', 'ankle': 'knee', 'el': 'sh', 'hand': 'el'}[k.split('_')[0]] + '_' + k.split('_')[1]
                u = J[up]
                rot = math.degrees(math.atan2(b[1] - u[1], b[0] - u[0]))
                c = (b[0] + (u[0] - b[0]) * 0.18, b[1] + (u[1] - b[1]) * 0.18)
                out.append(f'<ellipse cx="{c[0]:.1f}" cy="{c[1]:.1f}" rx="{5 * tf[2]:.1f}" ry="{15 * tf[2]:.1f}" transform="rotate({rot:.1f} {c[0]:.1f} {c[1]:.1f})" fill="none" stroke="{BD}" stroke-width="{5 * tf[2]:.1f}"/>')
                continue
            out.append(f'<path d="M{a[0]:.1f},{a[1]:.1f} L{b[0]:.1f},{b[1]:.1f}" stroke="{BDL}" stroke-width="{10 * tf[2]:.1f}" stroke-linecap="round"/>'
                       f'<path d="M{a[0]:.1f},{a[1]:.1f} L{b[0]:.1f},{b[1]:.1f}" stroke="{BD}" stroke-width="{7 * tf[2]:.1f}" stroke-linecap="round"/>')
    return ''.join(out)

def props_svg(props, tf):
    T = lambda q: (q[0] * tf[2] + tf[0], q[1] * tf[2] + tf[1])
    out = []
    for pr in props:
        if pr[0] == 'post':  # anchor post at x (height optional)
            x0, y0 = T((pr[1], -(pr[2] if len(pr) > 2 else 190))); x1, y1 = T((pr[1], 0))
            out.append(f'<rect x="{x0 - 5:.1f}" y="{y0:.1f}" width="10" height="{y1 - y0:.1f}" rx="3" fill="#C9CDD3"/>')
        elif pr[0] == 'box':  # box: x, width, height
            a = T((pr[1], -pr[3])); b = T((pr[1] + pr[2], 0))
            out.append(f'<rect x="{a[0]:.1f}" y="{a[1]:.1f}" width="{b[0] - a[0]:.1f}" height="{b[1] - a[1]:.1f}" rx="4" fill="#E4E7EB" stroke="#B9BFC8" stroke-width="2"/>')
        elif pr[0] == 'arrow':  # from, to (relative)
            a, b = T(pr[1]), T(pr[2])
            ang_ = math.atan2(b[1] - a[1], b[0] - a[0])
            h1 = (b[0] - 9 * math.cos(ang_ - 0.5), b[1] - 9 * math.sin(ang_ - 0.5))
            h2 = (b[0] - 9 * math.cos(ang_ + 0.5), b[1] - 9 * math.sin(ang_ + 0.5))
            out.append(f'<path d="M{a[0]:.1f},{a[1]:.1f} L{b[0]:.1f},{b[1]:.1f} M{h1[0]:.1f},{h1[1]:.1f} L{b[0]:.1f},{b[1]:.1f} L{h2[0]:.1f},{h2[1]:.1f}" stroke="#4FA3FF" stroke-width="3" fill="none" stroke-linecap="round" stroke-linejoin="round"/>')
    return ''.join(out)

FW, FH, FLOOR = 540, 360, 330

def frames(ex):
    """Two SVG frames (start, end) for an exercise dict: poses [start, end], muscles, band, props."""
    poses = ex['poses']
    pts = [q for p in poses for q in p.points()]
    for side in ex.get('props', []):
        for pr in side:
            if pr[0] == 'post':
                pts += [(pr[1], -(pr[2] if len(pr) > 2 else 190)), (pr[1], 0)]
            elif pr[0] == 'box':
                pts += [(pr[1], -pr[3]), (pr[1] + pr[2], 0)]
    for side in ex.get('band', []):
        for b in side:
            for x in (b[1] if b[0] == 'line' else b[1:]):
                if isinstance(x, tuple) and x[0] == 'rel':
                    pts.append((x[1], x[2]))
    xs, ys = [q[0] for q in pts], [q[1] for q in pts]
    maxy = max(max(ys), 0)
    h, w = maxy - min(ys) + 24, max(xs) - min(xs) + 40
    sc = min(1.0, (FLOOR - 24) / h, (FW - 40) / w)
    cx = (max(xs) + min(xs)) / 2
    tf = (FW / 2 - cx * sc, FLOOR - 12 * (1 if maxy > -5 else 0) - maxy * sc, sc)
    tf = (tf[0], FLOOR - maxy * sc - 10 * sc, sc)
    out = []
    for i, p in enumerate(poses):
        body, J = draw(p, ex['p'] + ex.get('s', [])[:0] + ex['p'], tf)
        band = band_svg(J, ex.get('band', [[]])[i] if ex.get('band') else [], tf, FLOOR)
        props = props_svg(ex.get('props', [[], []])[i], tf)
        floor = f'<path d="M20,{FLOOR + 1} L{FW - 20},{FLOOR + 1}" stroke="#E4E7EB" stroke-width="3" stroke-linecap="round"/>'
        out.append(f'<svg xmlns="http://www.w3.org/2000/svg" width="{FW}" height="{FH}" viewBox="0 0 {FW} {FH}"><rect width="{FW}" height="{FH}" fill="#FFFFFF"/>{floor}{props}{body}{band}</svg>')
    return out
