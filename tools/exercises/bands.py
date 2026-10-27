# 1.4.2: Kouros' own resistance-band exercises with drawn start / end pictures
# (tools/exercises/figure.py). Writes public/ex/<id>.webp (two 360×240 frames side by side, like the
# Free Exercise DB photos) and merges the items into src-tauri/resources/exercises_extra.json
# (`bandList` = quick list «Упражнения с резиной»). Texts are written by Kouros.
# Run from the repo root: python3 tools/exercises/bands.py
import asyncio, io, json, os, sys
sys.path.insert(0, os.path.dirname(__file__))
from figure import Pose, frames
from PIL import Image

ST, NK = (0, -156), (0, -262)          # standing pelvis / neck
HIPS = lambda dx=0: dict(l=(-22 + dx, -150), r=(22 + dx, -150))
EL_OUT = dict(l=(-58, -185), r=(58, -185))


def item(id_, ru, en, tip, p, s=(), st=False, leg=False, syn=(), bw=None, cat='strength', poses=None, band=None, props=None):
    return dict(id=id_, ru=ru, en=en, syn=list(syn), tip=tip, cat=cat, eq='bands', lvl='beginner', mech='compound', force='pull',
                p=list(p), s=list(s), bw=bw, st=st, leg=leg, img=2, _poses=poses, _band=band, _props=props)


def side_lying(top_foot, top_knee=None, knees_bent=False):
    feet = dict(l=(164, -10), r=top_foot) if not knees_bent else dict(l=(118, -10), r=top_foot)
    kn = dict(l=(70, 30), r=top_knee) if knees_bent else None
    fs = None
    return Pose('front', (0, -28), (-105, -28), hands=dict(l=(-150, -30), r=(-40, -6)), feet=feet, knees=kn, fs=fs,
                elbows=dict(l=(-112, -12), r=(-70, -40)))


def quad_rear(lift=False):
    fsq = dict(torso=0.28, shin_l=0.3, shin_r=0.3)
    feet = dict(l=(-20, 4), r=(78, -52) if lift else (20, 4))
    kn = dict(l=(-16, 0), r=(80, -70) if lift else (16, 0))
    return Pose('front', (0, -80), (0, -110), hands=dict(l=(-40, 0), r=(40, 0)), feet=feet, knees=kn, fs=fsq, elbows=dict(l=(-60, -40), r=(60, -40)))


def quad_side(kick=False):
    feet = dict(l=(-135, -4), r=(-140, -165) if kick else (-130, -4))
    kn = dict(l=(-62, 2), r=(-142, -84) if kick else (-58, 2))
    return Pose('side', (-60, -82), (45, -92), hands=dict(l=(40, 0), r=(46, 0)), feet=feet, knees=kn, head=110)


def plank(low=False, leg_up=False, band_back=False):
    pel, neck = ((-40, -40), (65, -44)) if low else ((-40, -66), (65, -84))
    feet = dict(l=(-196, 0), r=(-192, -44) if leg_up else (-194, 0))
    return Pose('side', pel, neck, hands=dict(l=(68, 0), r=(73, 0)), feet=feet, elbows=dict(l=(45, -55), r=(48, -55)) if low else None,
                knees=dict(l=(-95, -40), r=(-95, -60)), head=105)


def seated(hands, foot_ang=90, elbows=None, legs_toward=False):
    if legs_toward:
        fs = dict(thigh_l=0.25, thigh_r=0.25, shin_l=0.25, shin_r=0.25)
        return Pose('front', (0, -16), (0, -121), hands=dict(l=(-52, -4), r=(52, -4)), feet=dict(l=(-30, -2), r=(30, -2)), fs=fs, elbows=dict(l=(-70, -60), r=(70, -60)),
                    knees=dict(l=(-22, -8), r=(22, -8)), foot_ang=foot_ang)
    return Pose('side', (0, -14), (14, -118), hands=hands, feet=dict(l=(156, -8), r=(158, -8)), elbows=elbows,
                knees=dict(l=(80, -30), r=(80, -30)), foot_ang=dict(l=foot_ang, r=foot_ang))


EX = [
    # ---------- mini band ----------
    item('k_mb_bridge', 'Ягодичный мост с мини-резинкой', 'Glute bridge with mini band',
         'Мини-резинка чуть выше коленей, стопы на ширине таза. Поднимай таз, сжимая ягодицы, и держи колени разведёнными против резинки; опускайся медленно.',
         ['glutes'], ['hamstrings', 'abductors'], syn=['мост с резинкой', 'ягодичный мост мини-бэнд'],
         poses=[Pose('side', (0, -14), (-105, -12), hands=dict(l=(10, -6), r=(14, -6)), feet=dict(l=(72, 0), r=(76, 0)), knees=dict(l=(40, -80), r=(40, -80)), elbows=dict(l=(-40, -2), r=(-40, -2))),
                Pose('side', (0, -75), (-88, -14), hands=dict(l=(10, -6), r=(14, -6)), feet=dict(l=(72, 0), r=(76, 0)), knees=dict(l=(40, -100), r=(40, -100)), elbows=dict(l=(-40, -2), r=(-40, -2)))],
         band=[[('loop', 'knee_l', 'knee_r')]] * 2),
    item('k_mb_clamshell', '«Ракушка» с мини-резинкой', 'Clamshell with mini band',
         'Лёжа на боку, колени согнуты, стопы вместе, резинка выше коленей. Не разводя стоп и не заваливая таз назад, поднимай верхнее колено и медленно опускай.',
         ['abductors', 'glutes'], leg=True, syn=['ракушка', 'раскрытие коленей лёжа на боку'],
         poses=[side_lying((116, -26), (70, 10), True), side_lying((116, -26), (60, -80), True)],
         band=[[('loop', 'knee_l', 'knee_r')]] * 2, props=[[], [('arrow', (95, -50), (95, -95))]]),
    item('k_mb_lateral_walk', 'Боковые шаги с мини-резинкой', 'Lateral band walk',
         'Резинка на щиколотках или над коленями, полуприсед, спина ровная. Шагай в сторону, не сводя стопы до конца и не раскачивая корпус; столько же шагов обратно.',
         ['abductors', 'glutes'], ['quadriceps'], syn=['боковая ходьба с резинкой', 'шаги в сторону'],
         poses=[Pose('front', (0, -128), (0, -232), hands=dict(l=(-22, -150), r=(22, -150)), elbows=dict(l=(-60, -180), r=(60, -180)), feet=dict(l=(-22, 0), r=(22, 0)), knees=dict(l=(-40, -60), r=(40, -60))),
                Pose('front', (16, -128), (16, -232), hands=dict(l=(-6, -150), r=(38, -150)), elbows=dict(l=(-44, -180), r=(76, -180)), feet=dict(l=(-22, 0), r=(66, 0)), knees=dict(l=(-30, -60), r=(70, -60)))],
         band=[[('loop', 'ankle_l', 'ankle_r')]] * 2, props=[[], [('arrow', (90, -30), (130, -30))]]),
    item('k_mb_squat', 'Приседания с мини-резинкой', 'Mini band squat',
         'Резинка чуть выше коленей, стопы на ширине плеч. Садись, отводя таз назад, и всё время дави коленями наружу против резинки — колени смотрят туда же, куда носки.',
         ['quadriceps', 'glutes'], ['abductors'], syn=['присед с резинкой на коленях'], bw=0.8,
         poses=[Pose('front', ST, NK, feet=dict(l=(-26, 0), r=(26, 0)), hands=dict(l=(-8, -205), r=(8, -205)), elbows=dict(l=(-45, -215), r=(45, -215))),
                Pose('front', (0, -92), (0, -196), feet=dict(l=(-26, 0), r=(26, 0)), knees=dict(l=(-60, -55), r=(60, -55)), hands=dict(l=(-8, -142), r=(8, -142)), elbows=dict(l=(-45, -150), r=(45, -150)))],
         band=[[('loop', 'knee_l', 'knee_r')]] * 2),
    item('k_mb_kickback_stand', 'Отведение ноги назад стоя с мини-резинкой', 'Standing band kickback',
         'Резинка на щиколотках, держись рукой за опору. Прямую ногу отводи назад за счёт ягодицы, не прогибая поясницу, и медленно возвращай.',
         ['glutes'], ['hamstrings'], leg=True, syn=['мах ногой назад с резинкой'],
         poses=[Pose('side', ST, NK, hands=dict(l=(75, -175), r=(78, -178))),
                Pose('side', (4, -156), (12, -262), hands=dict(l=(75, -175), r=(78, -178)), feet=dict(l=(2, 0), r=(-62, -38)))],
         band=[[('loop', 'ankle_l', 'ankle_r')]] * 2, props=[[('post', 90, 200)], [('post', 90, 200)]]),
    item('k_mb_abduction_stand', 'Отведение ноги в сторону стоя с мини-резинкой', 'Standing band hip abduction',
         'Резинка на щиколотках, руки на поясе или на опоре. Отводи прямую ногу в сторону, носок смотрит вперёд, корпус не наклоняй.',
         ['abductors'], ['glutes'], leg=True, syn=['мах ногой в сторону с резинкой'],
         poses=[Pose('front', ST, NK, hands=HIPS(), elbows=EL_OUT, feet=dict(l=(-16, 0), r=(16, 0))),
                Pose('front', (-6, -156), (-10, -262), hands=HIPS(-6), elbows=dict(l=(-64, -185), r=(52, -185)), feet=dict(l=(-20, 0), r=(86, -16)))],
         band=[[('loop', 'ankle_l', 'ankle_r')]] * 2),
    item('k_mb_fire_hydrant', '«Пожарный гидрант» с мини-резинкой', 'Fire hydrant with mini band',
         'На четвереньках, резинка выше коленей. Не меняя угла в колене, отводи согнутую ногу в сторону до уровня таза; корпус не поворачивается.',
         ['glutes', 'abductors'], leg=True, syn=['гидрант', 'отведение бедра на четвереньках'],
         poses=[quad_rear(False), quad_rear(True)], band=[[('loop', 'knee_l', 'knee_r')]] * 2),
    item('k_mb_donkey_kick', 'Мах ногой назад на четвереньках с мини-резинкой', 'Donkey kick with mini band',
         'На четвереньках, резинка выше коленей. Согнутую ногу поднимай стопой к потолку, пока бедро не станет горизонтально; поясница не прогибается.',
         ['glutes'], ['hamstrings'], leg=True, syn=['ослиный мах', 'донки кик'],
         poses=[quad_side(False), quad_side(True)], band=[[('loop', 'knee_l', 'knee_r')], [('line', ['knee_l', 'knee_r'])]]),
    item('k_mb_side_lying_abd', 'Отведение ноги лёжа на боку с мини-резинкой', 'Side-lying abduction with mini band',
         'Лёжа на боку, ноги прямые, резинка на щиколотках. Поднимай верхнюю ногу чуть назад от линии тела, носок вперёд, и медленно опускай.',
         ['abductors'], ['glutes'], leg=True, syn=['махи лёжа на боку'],
         poses=[side_lying((163, -24)), side_lying((150, -96))], band=[[('loop', 'ankle_l', 'ankle_r')], [('line', ['ankle_l', 'ankle_r'])]]),
    item('k_mb_plank_leg_lift', 'Планка с подъёмом ноги и мини-резинкой', 'Plank leg lift with mini band',
         'Планка на ладонях, резинка на щиколотках. Поочерёдно поднимай прямую ногу на 20–30 см, таз не поворачивается и не проседает.',
         ['abdominals', 'glutes'], ['shoulders'], leg=True, syn=['планка с резинкой'],
         poses=[plank(), plank(leg_up=True)], band=[[('loop', 'ankle_l', 'ankle_r')], [('line', ['ankle_l', 'ankle_r'])]]),
    # ---------- long band ----------
    item('k_lb_row_stand', 'Тяга резины к поясу стоя', 'Standing band row',
         'Резина закреплена впереди на уровне пояса. Тяни ручки к поясу, сводя лопатки, локти идут вдоль корпуса; возвращай руки медленно.',
         ['middle back', 'lats'], ['biceps'], syn=['горизонтальная тяга с резиной'],
         poses=[Pose('side', (-4, -152), (0, -258), hands=dict(l=(104, -175), r=(108, -178))),
                Pose('side', (-4, -152), (-2, -258), hands=dict(l=(24, -168), r=(26, -170)), elbows=dict(l=(-40, -180), r=(-40, -180)))],
         band=[[('line', [('rel', 170, -172), 'hand_r'])]] * 2, props=[[('post', 170, 210)], [('post', 170, 210)]]),
    item('k_lb_bent_row', 'Тяга резины в наклоне', 'Bent-over band row',
         'Встань на середину резины, наклонись вперёд с прямой спиной, колени чуть согнуты. Тяни ручки к поясу, сводя лопатки, и медленно опускай.',
         ['middle back', 'lats'], ['biceps', 'lower back'], syn=['тяга в наклоне с резиной'],
         poses=[Pose('side', (-38, -146), (58, -214), hands=dict(l=(28, -72), r=(32, -74)), feet=dict(l=(-4, 0), r=(0, 0)), knees=dict(l=(20, -80), r=(20, -80))),
                Pose('side', (-38, -146), (58, -214), hands=dict(l=(6, -150), r=(8, -152)), elbows=dict(l=(-32, -175), r=(-32, -175)), feet=dict(l=(-4, 0), r=(0, 0)), knees=dict(l=(20, -80), r=(20, -80)))],
         band=[[('line', [('rel', 8, -2), 'hand_r'])]] * 2),
    item('k_lb_rdl', 'Румынская тяга с резиной', 'Band Romanian deadlift',
         'Встань на резину, ручки в руках. Отводи таз назад и наклоняйся с прямой спиной, колени слегка согнуты, затем выпрямляйся, сжимая ягодицы.',
         ['hamstrings', 'glutes'], ['lower back'], syn=['румынская становая с резиной'],
         poses=[Pose('side', ST, NK, hands=dict(l=(14, -138), r=(16, -140))),
                Pose('side', (-42, -150), (60, -205), hands=dict(l=(36, -62), r=(40, -64)), feet=dict(l=(-4, 0), r=(0, 0)), knees=dict(l=(20, -80), r=(20, -80)))],
         band=[[('line', [('rel', 10, -2), 'hand_r'])]] * 2),
    item('k_lb_pulldown', 'Тяга резины сверху на коленях', 'Kneeling band pulldown',
         'Резина закреплена высоко впереди. Стоя на коленях, тяни ручки вниз к груди, опуская плечи и сводя лопатки; руки вверх возвращай медленно.',
         ['lats'], ['biceps', 'middle back'], syn=['вертикальная тяга с резиной'],
         poses=[Pose('side', (0, -82), (6, -187), hands=dict(l=(52, -288), r=(56, -290)), feet=dict(l=(-74, -4), r=(-70, -4)), knees=dict(l=(2, 0), r=(2, 0))),
                Pose('side', (0, -82), (2, -187), hands=dict(l=(24, -194), r=(26, -196)), elbows=dict(l=(-18, -145), r=(-18, -145)), feet=dict(l=(-74, -4), r=(-70, -4)), knees=dict(l=(2, 0), r=(2, 0)))],
         band=[[('line', [('rel', 96, -330), 'hand_r'])]] * 2, props=[[('post', 96, 340)], [('post', 96, 340)]]),
    item('k_lb_pushup', 'Отжимания с резиной', 'Banded push-up',
         'Резина за спиной на уровне лопаток, концы прижаты ладонями к полу. Тело прямое, опускайся грудью к полу и выжимай себя вверх против резины.',
         ['chest'], ['triceps', 'shoulders'], syn=['отжимания с эспандером'], bw=0.64,
         poses=[plank(), plank(low=True)], band=[[('line', ['hand_l', 'sh_r', 'hand_r'])]] * 2),
    item('k_lb_split_squat', 'Выпады на месте с резиной', 'Band split squat',
         'Встань передней ногой на резину, ручки у плеч. Опускайся вниз, пока оба колена не согнутся под прямым углом, и поднимайся, толкаясь передней ногой.',
         ['quadriceps', 'glutes'], ['hamstrings'], leg=True, syn=['сплит-присед с резиной'], bw=0.8,
         poses=[Pose('side', (-6, -150), (-4, -256), hands=dict(l=(16, -238), r=(18, -240)), elbows=dict(l=(22, -205), r=(22, -205)), feet=dict(l=(-62, -2), r=(48, 0)), knees=dict(l=(-30, -80), r=(30, -80)), foot_ang=dict(l=40, r=90)),
                Pose('side', (-8, -96), (-6, -202), hands=dict(l=(14, -186), r=(16, -188)), elbows=dict(l=(20, -150), r=(20, -150)), feet=dict(l=(-74, -2), r=(48, 0)), knees=dict(l=(-40, -8), r=(56, -76)), foot_ang=dict(l=40, r=90))],
         band=[[('line', [('rel', 52, -2), 'hand_r'])]] * 2),
    item('k_lb_pallof', 'Паллоф-пресс с резиной', 'Pallof press',
         'Резина закреплена сбоку на уровне груди. Встань боком, руки с ручкой у груди. Выпрямляй руки вперёд, не давая резине повернуть корпус, задержись на 2 секунды и верни.',
         ['abdominals'], ['shoulders'], syn=['паллоф', 'антиротация'],
         poses=[Pose('front', ST, NK, hands=dict(l=(-4, -212), r=(4, -212)), elbows=dict(l=(-40, -200), r=(40, -200))),
                Pose('front', ST, NK, hands=dict(l=(-3, -226), r=(3, -226)), fs=dict(up_l=0.45, up_r=0.45, fore_l=0.4, fore_r=0.4), elbows=dict(l=(-14, -240), r=(14, -240)))],
         band=[[('line', [('rel', 175, -212), 'hand_r'])], [('line', [('rel', 175, -212), 'hand_r'])]],
         props=[[('post', 175, 240)], [('post', 175, 240)]]),
    item('k_lb_woodchop', '«Дровосек» с резиной', 'Band woodchop',
         'Резина закреплена высоко сбоку. Прямыми руками тяни ручку по диагонали вниз к противоположному бедру, поворачивая корпус и слегка приседая; вверх — медленно.',
         ['abdominals'], ['shoulders', 'lats'], syn=['дровосек', 'диагональная тяга'],
         poses=[Pose('front', ST, NK, hands=dict(l=(96, -330), r=(102, -334))),
                Pose('front', (0, -138), (-8, -242), hands=dict(l=(-74, -128), r=(-70, -132)), knees=dict(l=(-30, -70), r=(30, -70)), feet=dict(l=(-26, 0), r=(26, 0)))],
         band=[[('line', [('rel', 185, -350), 'hand_r'])]] * 2, props=[[('post', 185, 360)], [('post', 185, 360)]]),
    item('k_lb_overhead_pullapart', 'Разведение резины над головой', 'Overhead band pull-apart',
         'Возьми резину прямыми руками над головой на ширине плеч. Разводи руки в стороны и чуть назад, опуская их до уровня плеч, лопатки сводятся вниз.',
         ['shoulders', 'middle back'], ['traps'], syn=['разведение над головой'],
         poses=[Pose('front', ST, NK, hands=dict(l=(-30, -366), r=(30, -366))),
                Pose('front', ST, NK, hands=dict(l=(-122, -300), r=(122, -300)))],
         band=[[('line', ['hand_l', 'hand_r'])]] * 2),
    item('k_lb_front_raise', 'Подъём рук перед собой с резиной', 'Band front raise',
         'Встань на резину, ручки у бёдер. Поднимай прямые руки вперёд до уровня плеч, не раскачиваясь, и медленно опускай.',
         ['shoulders'], ['chest'], syn=['подъём рук вперёд', 'фронтальный подъём'],
         poses=[Pose('side', ST, NK, hands=dict(l=(8, -146), r=(10, -148))),
                Pose('side', ST, NK, hands=dict(l=(108, -250), r=(112, -252)))],
         band=[[('line', [('rel', 6, -2), 'hand_r'])]] * 2),
    item('k_lb_chest_press', 'Жим резины от груди стоя', 'Standing band chest press',
         'Резина закреплена сзади на уровне груди. Шаг вперёд, ручки у груди. Выпрямляй руки вперёд, сводя их перед грудью, и медленно возвращай.',
         ['chest'], ['triceps', 'shoulders'], syn=['жим от груди с резиной'],
         poses=[Pose('side', (6, -152), (14, -258), hands=dict(l=(30, -222), r=(32, -224)), elbows=dict(l=(-28, -215), r=(-28, -215)), feet=dict(l=(-40, -2), r=(40, 0)), foot_ang=dict(l=40, r=90)),
                Pose('side', (6, -152), (16, -258), hands=dict(l=(118, -236), r=(122, -238)), feet=dict(l=(-40, -2), r=(40, 0)), foot_ang=dict(l=40, r=90))],
         band=[[('line', [('rel', -130, -222), 'hand_r'])]] * 2, props=[[('post', -130, 250)], [('post', -130, 250)]]),
    item('k_lb_curl', 'Сгибание рук с резиной', 'Band biceps curl',
         'Встань на резину, ручки в опущенных руках, ладони вперёд. Сгибай руки, не отводя локти от корпуса, и медленно опускай.',
         ['biceps'], ['forearms'], syn=['бицепс с резиной'],
         poses=[Pose('side', ST, NK, hands=dict(l=(8, -146), r=(10, -148))),
                Pose('side', ST, NK, hands=dict(l=(30, -244), r=(32, -246)), elbows=dict(l=(2, -195), r=(2, -195)))],
         band=[[('line', [('rel', 6, -2), 'hand_r'])]] * 2),
    item('k_lb_pushdown', 'Разгибание рук вниз с резиной', 'Band triceps pushdown',
         'Резина закреплена высоко впереди. Локти прижаты к бокам, разгибай руки вниз до прямых и медленно возвращай до прямого угла в локте.',
         ['triceps'], syn=['трицепс с резиной', 'разгибания на трицепс'],
         poses=[Pose('side', ST, NK, hands=dict(l=(46, -205), r=(48, -207)), elbows=dict(l=(0, -195), r=(0, -195))),
                Pose('side', ST, NK, hands=dict(l=(26, -144), r=(28, -146)))],
         band=[[('line', [('rel', 100, -330), 'hand_r'])]] * 2, props=[[('post', 100, 340)], [('post', 100, 340)]]),
    item('k_lb_seated_row', 'Тяга резины сидя', 'Seated band row',
         'Сядь с прямыми ногами, резина вокруг стоп. Спина прямая; тяни ручки к поясу, сводя лопатки, и медленно выпрямляй руки.',
         ['middle back', 'lats'], ['biceps'], syn=['тяга сидя с резиной'],
         poses=[seated(dict(l=(118, -58), r=(122, -60))), seated(dict(l=(32, -56), r=(34, -58)), elbows=dict(l=(-30, -76), r=(-30, -76)))],
         band=[[('line', ['toe_r', 'hand_r'])]] * 2),
    # ---------- feet and shins ----------
    item('k_fb_dorsiflexion', 'Тыльное сгибание стопы с резиной', 'Banded ankle dorsiflexion',
         'Сидя, нога прямая, резина закреплена впереди и надета на верх стопы. Тяни носок на себя против резины и медленно отпускай. Укрепляет переднюю поверхность голени.',
         ['calves'], leg=True, syn=['носок на себя с резиной', 'передняя большеберцовая'],
         poses=[seated(dict(l=(30, -10), r=(34, -10)), 40), seated(dict(l=(30, -10), r=(34, -10)), 118)],
         band=[[('line', [('rel', 225, -28), 'toe_r'])]] * 2, props=[[('post', 225, 60)], [('post', 225, 60)]]),
    item('k_fb_plantarflexion', 'Носок от себя с резиной', 'Banded ankle plantar flexion',
         'Сидя, нога прямая, резина вокруг стопы, концы в руках. Тяни носок от себя против резины, как будто давишь на педаль, и медленно возвращай.',
         ['calves'], leg=True, syn=['носок от себя', 'подошвенное сгибание'],
         poses=[seated(dict(l=(60, -64), r=(64, -66)), 110), seated(dict(l=(60, -64), r=(64, -66)), 30)],
         band=[[('line', ['hand_r', 'toe_r'])]] * 2),
    item('k_fb_inversion', 'Поворот стопы внутрь с резиной', 'Banded ankle inversion',
         'Сидя, резина закреплена сбоку снаружи от ноги и надета на стопу. Поворачивай стопу подошвой внутрь против резины, голень не двигается.',
         ['calves'], leg=True, syn=['супинация стопы с резиной'],
         poses=[seated(dict(l=(-30, -20), r=(30, -20)), dict(l=0, r=0), legs_toward=True), seated(dict(l=(-30, -20), r=(30, -20)), dict(l=0, r=-35), legs_toward=True)],
         band=[[('line', [('rel', 150, -8), 'toe_r'])]] * 2, props=[[('post', 150, 50)], [('post', 150, 50), ('arrow', (62, -30), (40, -52))]]),
    item('k_fb_eversion', 'Поворот стопы наружу с резиной', 'Banded ankle eversion',
         'Сидя, резина закреплена сбоку с внутренней стороны и надета на стопу. Поворачивай стопу подошвой наружу против резины, голень не двигается.',
         ['calves'], leg=True, syn=['пронация стопы с резиной'],
         poses=[seated(dict(l=(-30, -20), r=(30, -20)), dict(l=0, r=0), legs_toward=True), seated(dict(l=(-30, -20), r=(30, -20)), dict(l=0, r=35), legs_toward=True)],
         band=[[('line', [('rel', -150, -8), 'toe_r'])]] * 2, props=[[('post', -150, 50)], [('post', -150, 50), ('arrow', (-2, -52), (24, -30))]]),
    # ---------- running ----------
    item('k_rb_resisted_march', 'Бег на месте с резиной за пояс', 'Band-resisted running march',
         'Резина вокруг пояса, закреплена сзади. Наклонись чуть вперёд всем телом и выполняй беговые шаги с высоким подъёмом колена и активной работой рук.',
         ['quadriceps', 'glutes'], ['calves', 'abdominals'], cat='plyometrics', syn=['бег с сопротивлением', 'беговые шаги с резиной'],
         poses=[Pose('side', (0, -158), (22, -262), hands=dict(l=(48, -190), r=(-34, -172)), elbows=dict(l=(30, -160), r=(-30, -200)), feet=dict(l=(-4, 0), r=(40, -82)), knees=dict(l=(10, -80), r=(62, -110)), foot_ang=dict(l=90, r=80)),
                Pose('side', (0, -158), (22, -262), hands=dict(l=(-34, -172), r=(48, -190)), elbows=dict(l=(-30, -200), r=(30, -160)), feet=dict(l=(40, -82), r=(-4, 0)), knees=dict(l=(62, -110), r=(10, -80)), foot_ang=dict(l=80, r=90))],
         band=[[('line', [('rel', -160, -156), 'pelvis'])]] * 2, props=[[('post', -160, 200)], [('post', -160, 200)]]),
    item('k_rb_a_march', '«А-шаг» с мини-резинкой', 'A-march with mini band',
         'Мини-резинка на стопах. Поднимай колено до уровня таза, стопа под коленом, носок на себя; опускай ногу под себя активно, на переднюю часть стопы. Руки работают как при беге.',
         ['quadriceps'], ['calves', 'abdominals'], cat='plyometrics', syn=['а-скип', 'высокое колено с резинкой'],
         poses=[Pose('side', ST, NK, hands=dict(l=(46, -188), r=(-30, -172)), elbows=dict(l=(30, -160), r=(-30, -200)), feet=dict(l=(0, 0), r=(28, -78)), knees=dict(l=(6, -80), r=(66, -112)), foot_ang=dict(l=90, r=100)),
                Pose('side', ST, NK, hands=dict(l=(-30, -172), r=(46, -188)), elbows=dict(l=(-30, -200), r=(30, -160)), feet=dict(l=(28, -78), r=(0, 0)), knees=dict(l=(66, -112), r=(6, -80)), foot_ang=dict(l=100, r=90))],
         band=[[('line', ['ankle_l', 'ankle_r'])]] * 2),
]

# Library items with a band that belong in the quick list too.
FROM_LIBRARY = ['Band_Pull_Apart', 'External_Rotation_with_Band', 'Internal_Rotation_with_Band', 'Lateral_Raise_-_With_Bands', 'Monster_Walk',
                'Hip_Extension_with_Bands', 'Hip_Flexion_with_Band', 'Band_Good_Morning', 'Back_Flyes_-_With_Bands', 'Shoulder_Press_-_With_Bands',
                'Upright_Row_-_With_Bands', 'Squats_-_With_Bands', 'Calf_Raises_-_With_Bands', 'Seated_Band_Hamstring_Curl', 'Band_Hip_Adductions']


def foot_closeup(inward):
    """Inversion / eversion: one shin from the front, close up, the foot turning; band to a post."""
    from figure import SK, OL, MU, BD, BDL, FW, FH, FLOOR
    out = []
    px = 470 if inward else 70
    for turn in (0, -32 if inward else 32):
        foot = f'<g transform="rotate({turn} 270 262)"><path d="M232,250 Q270,232 308,250 L314,300 Q270,316 226,300 Z" fill="{SK}" stroke="{OL}" stroke-width="3"/></g>'
        shin = (f'<path d="M248,40 L246,250 M292,40 L294,250" stroke="{OL}" stroke-width="3"/>'
                f'<rect x="246" y="30" width="48" height="225" rx="20" fill="{SK}" stroke="{OL}" stroke-width="3"/>'
                f'<rect x="254" y="70" width="32" height="120" rx="14" fill="{MU}" opacity="0.9"/>')
        tx, ty = 270 + (40 if inward else -40) * (1 if turn == 0 else 0.8), 286 - (0 if turn == 0 else (14 if inward else 14))
        band = (f'<path d="M{px},{FLOOR - 40} L{tx:.0f},{ty:.0f}" stroke="{BDL}" stroke-width="9" stroke-linecap="round"/>'
                f'<path d="M{px},{FLOOR - 40} L{tx:.0f},{ty:.0f}" stroke="{BD}" stroke-width="6" stroke-linecap="round"/>')
        post = f'<rect x="{px - 6}" y="{FLOOR - 90}" width="12" height="90" rx="3" fill="#C9CDD3"/>'
        arrow = ''
        if turn:
            a = 'M330,250 Q345,215 320,195 M320,195 l12,2 M320,195 l2,12' if not inward else 'M210,250 Q195,215 220,195 M220,195 l-12,2 M220,195 l-2,12'
            arrow = f'<path d="{a}" stroke="#4FA3FF" stroke-width="4" fill="none" stroke-linecap="round"/>'
        floor = f'<path d="M20,{FLOOR + 1} L{FW - 20},{FLOOR + 1}" stroke="#E4E7EB" stroke-width="3"/>'
        out.append(f'<svg xmlns="http://www.w3.org/2000/svg" width="{FW}" height="{FH}" viewBox="0 0 {FW} {FH}"><rect width="{FW}" height="{FH}" fill="#FFFFFF"/>{floor}{post}{shin}{foot}{band}{arrow}</svg>')
    return out


async def render(out_dir, only=None):
    from playwright.async_api import async_playwright
    async with async_playwright() as pw:
        b = await pw.chromium.launch()
        pg = await b.new_page(viewport={'width': 1080, 'height': 360}, device_scale_factor=2)
        for e in EX:
            if only and e['id'] not in only:
                continue
            ex = dict(poses=e['_poses'], p=e['p'], s=e['s'], band=e['_band'] or [[], []], props=e['_props'] or [[], []])
            fr = foot_closeup(e['id'] == 'k_fb_inversion') if e['id'] in ('k_fb_inversion', 'k_fb_eversion') else frames(ex)
            await pg.set_content('<html><body style="margin:0;display:flex">' + ''.join(fr) + '</body></html>')
            png = await pg.screenshot()
            im = Image.open(io.BytesIO(png)).convert('RGB').resize((720, 240), Image.LANCZOS)
            im.save(os.path.join(out_dir, e['id'] + '.webp'), 'WEBP', quality=72, method=6)
        await b.close()


if __name__ == '__main__':
    out = sys.argv[1] if len(sys.argv) > 1 else 'public/ex'
    asyncio.run(render(out, set(sys.argv[2:]) or None))
    if out == 'public/ex':
        path = 'src-tauri/resources/exercises_extra.json'
        extra = json.load(open(path, encoding='utf-8'))
        ids = {e['id'] for e in EX}
        extra['items'] = [x for x in extra['items'] if x['id'] not in ids] + [{k: v for k, v in e.items() if not k.startswith('_')} for e in EX]
        extra['bandList'] = [e['id'] for e in EX] + FROM_LIBRARY
        json.dump(extra, open(path, 'w', encoding='utf-8'), ensure_ascii=False, indent=1)
        print(len(EX), 'band exercises')
