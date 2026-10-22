# Muscle map assets (1.2): the drawing by a friend of the owner (tools/muscles/src.png, colours = sample groups)
# -> grey base (public/muscles/base.webp), one alpha mask per muscle_svc group (m_<key>.png) and a
# small label map for clicks (labels.png, value = group index + 1 in GROUPS order). Run from the repo root.
from PIL import Image, ImageDraw, ImageFilter
import numpy as np
from scipy import ndimage
src=Image.open(__import__('os').path.join(__import__('os').path.dirname(__file__),'src.png')).convert('RGB'); W,H=src.size
im=np.asarray(src).astype(float)/255
mx=im.max(-1); mn=im.min(-1); sat=(mx-mn)/(mx+1e-6)
r,g,b=im[...,0],im[...,1],im[...,2]
# hue in [0,1)
d=mx-mn+1e-9
h=np.where(mx==r,((g-b)/d)%6,np.where(mx==g,(b-r)/d+2,(r-g)/d+4))/6
bg=np.array([22,26,38])/255
dist=np.sqrt(((im-bg)**2).sum(-1))
body=dist>0.07
col=(sat>0.3)&(mx>0.22)&(h<0.55)&body
col=ndimage.binary_opening(col,iterations=1); col=ndimage.binary_closing(col,iterations=2)
red=col&((h<0.06)|(h>0.95)); yel=col&(h>=0.06)&(h<0.2); grn=col&(h>=0.25)&(h<0.55)
yy,xx=np.mgrid[0:H,0:W]
front=xx<636; back=~front
G={k:np.zeros((H,W),bool) for k in ['calves','quads','hams','glutes','hips','core','back','chest','shoulders','arms','knees','ankles']}
def poly(pts):
    m=Image.new('L',(W,H),0); ImageDraw.Draw(m).polygon(pts,fill=255); return np.asarray(m)>0
def circ(cx,cy,rr):
    return (xx-cx)**2+(yy-cy)**2<rr*rr
G['chest']|=poly([(255,232),(338,226),(352,226),(430,232),(452,262),(445,320),(420,345),(360,348),(345,352),(330,348),(270,345),(245,320),(240,262)])&body
G['core']|=poly([(262,350),(420,350),(432,400),(430,470),(412,520),(372,560),(340,575),(308,560),(270,520),(250,470),(250,400)])&body
G['knees']|=(circ(287,805,30)|circ(410,805,30))&body
G['back']|=poly([(860,175),(900,165),(940,175),(990,215),(1030,240),(1045,290),(1040,380),(1020,450),(985,495),(930,510),(880,510),(840,495),(810,450),(795,380),(795,290),(810,240),(850,215)])&body
G['ankles']|=(poly([(838,990),(872,990),(876,1110),(842,1110)])|poly([(978,990),(1012,990),(1008,1110),(974,1110)]))&body
# coloured ones (override)
def put(k,m):
    for kk in G: G[kk]&=~m
    G[k]|=m
put('shoulders',grn&(yy<300))
# whole deltoids (the drawing is coloured only partly at the top of the shoulder)
put('shoulders',(poly([(250,222),(215,222),(185,245),(175,280),(180,320),(200,330),(225,300),(250,265)])|poly([(436,222),(471,222),(501,245),(511,280),(506,320),(486,330),(461,300),(436,265)])
    |poly([(830,225),(790,225),(760,245),(748,280),(752,310),(775,300),(805,280),(835,255)])|poly([(1010,225),(1050,225),(1080,245),(1092,280),(1088,310),(1065,300),(1035,280),(1005,255)]))&body)
put('arms',yel&front&(yy<440)&((xx<240)|(xx>440)))
put('arms',red&front&(yy<560))
put('quads',grn&front&(yy>500))
put('hips',yel&front&(yy>500)&(yy<820)&(xx>270)&(xx<410))
put('ankles',yel&front&(yy>820))
put('arms',yel&back&(yy<440)&((xx<810)|(xx>1030)))
put('arms',red&back&(yy<580)&((xx<790)|(xx>1050)))
put('glutes',yel&back&(yy>470)&(yy<660))
put('hams',red&back&(yy>600))
put('calves',grn&back&(yy>780))
for k in G:
    m=ndimage.binary_closing(G[k],iterations=4)
    G[k]=ndimage.binary_fill_holes(m)&body
for k,m in G.items(): print(k,m.sum())
# grey base: luminance, recolour coloured areas to match nearby grey
L=0.3*r+0.59*g+0.11*b
grey_mus=body&~col&(L>0.12)
tgt=np.median(L[grey_mus&(yy>500)])
lab,n=ndimage.label(col)
out=L.copy()
for i in range(1,n+1):
    m=lab==i
    if m.sum()<30: continue
    out[m]=np.clip(L[m]*(tgt*1.05/np.median(L[m])),0,1)
# soften seams
blur=ndimage.gaussian_filter(out,1.0); edge=ndimage.binary_dilation(col,iterations=2)&~ndimage.binary_erosion(col,iterations=2)
out[edge]=blur[edge]
a=np.clip((dist-0.04)/0.06,0,1)
rgb=np.stack([out*0.96,out*0.98,out*1.02],-1)
base=Image.fromarray(np.dstack([np.clip(rgb,0,1)*255,a*255]).astype(np.uint8),'RGBA')
S=1000/W; size=(1000,round(H*S))
base=base.resize(size,Image.LANCZOS)
import os; os.makedirs('public/muscles',exist_ok=True)
base.save('public/muscles/base.webp',quality=82,method=6)
keys=list(G)
lbl=np.zeros((H,W),np.uint8)
for i,k in enumerate(keys):
    m=Image.fromarray((G[k]*255).astype(np.uint8)).resize(size,Image.LANCZOS).filter(ImageFilter.GaussianBlur(0.8))
    Image.merge('LA',(Image.new('L',size,255),m)).save(f'public/muscles/m_{k}.png',optimize=True)
    lbl[G[k]]=i+1
Image.fromarray(lbl).resize((250,round(H*250/W)),Image.NEAREST).save('public/muscles/labels.png',optimize=True)
print(keys, size)
