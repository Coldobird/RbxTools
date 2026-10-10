"""Draw the background at its native 384x256 resolution, without antialiasing."""
from pathlib import Path
import math
import random
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'assets' / 'pocket-landscape-384x256.png'
PREVIEW = Path(__file__).with_name('pocket-landscape-preview-4x.png')
rng = random.Random(2709)
PAL = [
    '#180f19', '#22131e', '#2b1722', '#351b28',
    '#422130', '#522735', '#622d3c', '#753342',
    '#893d49', '#9e4650', '#b64f56', '#ce6060',
    '#df766b', '#ed927c', '#f5b392', '#ffdab0',
    '#39202b', '#4b2631', '#6b3039', '#a85355',
    '#c16a60', '#e69b80', '#884850', '#5b343e',
]
im = Image.open(ROOT / 'assets' / 'background-canvas-384x256.png').convert('RGB')
assert im.size == (384, 256)
d = ImageDraw.Draw(im)
def rect(box, c): d.rectangle(tuple(map(int, box)), fill=PAL[c])
def poly(points, c): d.polygon([(int(x), int(y)) for x, y in points], fill=PAL[c])
def line(points, c, width=1): d.line([(int(x), int(y)) for x, y in points], fill=PAL[c], width=width)
def dot(x, y, c): d.point((int(x), int(y)), fill=PAL[c])
def oval(box, c): d.ellipse(tuple(map(int, box)), fill=PAL[c])

# Large quiet sky and a restrained horizon haze, all from the fixed palette.
rect((0, 0, 383, 255), 2)
rect((0, 125, 383, 154), 3)
rect((0, 155, 383, 182), 4)
for y, base, upper in [(125, 3, 2), (155, 4, 3)]:
    for yy in range(y-5, y+4):
        for x in range(384):
            if (x % 4 == (yy % 2)*2) and (yy >= y or (x+yy) % 8 == 0):
                dot(x, yy, base)

# Tiny warm stars/embers; the upper central area stays almost empty.
for x, y, c in [(20,82,8),(58,89,7),(82,111,8),(119,116,9),
                (140,132,8),(176,124,8),(237,117,9),(261,99,8),
                (306,118,9),(337,100,8),(354,110,7),(371,96,8)]:
    dot(x,y,c)
    if c == 9: dot(x+1,y,c)

# Peach moon, drawn one row at a time by the raster circle primitive.
oval((272,120,293,141),14)
oval((274,121,293,140),15)
poly([(283,121),(289,122),(292,126),(288,127),(288,125),(284,125)],14)
poly([(276,126),(279,125),(281,127),(280,130),(278,130),(278,133),(275,132)],13)
poly([(285,130),(288,129),(290,131),(288,134),(289,137),(286,138),(284,135)],13)
rect((282,123,283,125),14)
dot(275,135,13)
dot(280,137,14)

def cloud(x,y,w,c):
    poly([(x,y+4),(x+4,y+4),(x+4,y+2),(x+9,y+2),
          (x+12,y-1),(x+15,y-1),(x+18,y-5),(x+23,y-5),
          (x+27,y-1),(x+31,y-1),(x+34,y+2),(x+w-6,y+2),
          (x+w-6,y+4),(x+w,y+4),(x+w,y+6),(x,y+6)],c)
    line([(x+7,y+6),(x+w-7,y+6)],c-1)
cloud(179,137,62,6)
cloud(228,143,53,5)
cloud(281,128,36,5)
cloud(310,126,27,4)
cloud(86,139,31,5)
rect((169,145,178,145),5)
rect((208,154,215,154),6)
rect((251,130,255,130),6)

# Layered jagged ridgelines and warmer illuminated mountain faces.
far = [(0,171),(19,158),(32,165),(44,157),(62,166),(82,150),
       (98,159),(113,156),(137,143),(148,146),(161,156),(173,151),
       (192,165),(211,159),(227,166),(250,151),(264,160),(278,153),
       (295,157),(318,149),(331,160),(349,155),(365,164),(383,155)]
poly(far+[(383,205),(0,205)],7)
line(far,8)
poly([(101,169),(137,146),(147,149),(154,158),(144,155),(145,158),
      (137,155),(130,163),(126,161),(113,171)],8)
poly([(217,177),(250,154),(260,162),(252,160),(246,166),(242,165),
      (234,173)],8)
near = [(0,183),(25,169),(39,177),(60,167),(79,179),(99,168),
        (111,174),(137,151),(146,164),(156,169),(163,166),(182,181),
        (193,174),(217,187),(236,169),(244,177),(264,169),(281,181),
        (299,170),(321,185),(345,174),(364,179),(383,171)]
poly(near+[(383,210),(0,210)],4)
poly([(108,177),(137,153),(139,162),(135,161),(135,166),(126,165),
      (129,170),(120,169),(123,175),(115,173)],6)
poly([(228,181),(236,172),(240,179),(236,177),(233,182)],6)

def pine(x,y,h,c):
    rect((x,y-h,x+1,y),c)
    for a in range(3,h,3):
        half = max(1,int(a*.21))
        poly([(x,y-h+a-5),(x-half,y-h+a+3),(x+half+1,y-h+a+3)],c)

for x in range(-3,390,5):
    ground=192+int(3*math.sin(x*.07))
    pine(x,ground,rng.randrange(7,21),5)
for x in range(8,383,9):
    ground=201+int(3*math.sin(x*.05))
    pine(x,ground,rng.randrange(9,29),3)

# Castle perched on the far shore. Pixel masonry, slate-red roofs, lit windows.
poly([(254,204),(262,183),(276,179),(290,170),(313,173),(329,190),
      (340,205)],3)
def window(x,y,w=2,h=3):
    rect((x-1,y-1,x+w,y+h),1)
    rect((x,y,x+w-1,y+h-1),12)
    line([(x,y),(x,y+h-1)],15)
def tower(x,base,height,w=8):
    top=base-height
    rect((x,top,x+w,base),2)
    rect((x+1,top+1,x+w-2,base),4)
    rect((x+w-2,top+1,x+w,base),1)
    for yy in range(top+4,base-1,5):
        line([(x+2,yy),(x+4,yy)],5)
        dot(x+w-2,yy+2,3)
    poly([(x-3,top+1),(x+w//2,top-11),(x+w+3,top+1)],1)
    poly([(x-1,top-1),(x+w//2,top-10),(x+w//2+1,top-1)],4)
    for yy in range(top-6,top,3):
        span=(yy-(top-11))//2
        line([(x+w//2-span,yy),(x+w//2+span,yy)],3)
    rect((x-2,top+1,x+w+2,top+2),5)
    window(x+w//2-1,top+6,2,3)
    return top
tower(263,195,26,7)
tower(294,193,34,8)
tower(307,191,48,9)
tower(328,196,27,7)
rect((269,175,304,197),2)
rect((272,178,303,194),4)
poly([(267,176),(275,168),(291,168),(303,177)],1)
poly([(269,174),(276,169),(290,169),(297,174)],5)
line([(272,171),(292,171)],6)
rect((285,166,287,171),2)
tower(279,194,25,7)
for x,y in [(274,180),(291,180),(298,180),(310,156),(311,174),(299,170)]:
    window(x,y)
rect((287,187,294,197),1)
rect((289,188,292,196),11)
rect((290,188,291,195),15)
rect((287,191,294,192),14)
line([(270,196),(318,196)],5)
line([(306,143),(306,132)],1)
poly([(307,132),(315,133),(311,135),(307,135)],8)

# Lake and horizontal bands of reflected terrain.
rect((0,203,383,255),4)
rect((0,204,383,207),7)
rect((0,208,383,210),5)
for y in range(211,253):
    for _ in range(13):
        x=rng.randrange(384)
        length=rng.randrange(3,22)
        c=rng.choice([4,5,6,7,17,18])
        line([(x,y),(x+length,y)],c)
for x in [18,51,89,147,187,232,323,352,377]:
    for y in range(209,244,3):
        xx=x+rng.randrange(-5,6)
        line([(xx,y),(xx+rng.randrange(4,13),y)],3)

# Broken peach-red moonlight across the water.
for y in range(205,250):
    spread=2+int((y-205)*.30)
    center=282+int(math.sin(y*1.7)*3)
    if y%3 != 0:
        line([(center-spread-rng.randrange(3),y),
              (center+spread+rng.randrange(3),y)],rng.choice([8,9,10,11]))
    if y%4 in [0,1]:
        line([(center-max(1,spread//3),y),(center+spread//3+1,y)],13)

# The arched stone bridge, with open water visible below each arch.
bridge = Image.new('RGBA', im.size)
bd=ImageDraw.Draw(bridge)
bd.rectangle((126,190,265,207),fill=PAL[3])
bd.rectangle((126,191,265,194),fill=PAL[6])
for cx in [145,174,203,232,260]:
    bd.ellipse((cx-10,195,cx+10,215),fill=(0,0,0,0))
    bd.rectangle((cx-10,205,cx+10,216),fill=(0,0,0,0))
    bd.arc((cx-12,193,cx+12,217),180,360,fill=PAL[8],width=2)
    for a in [205,230,255,280,305,330]:
        rad=math.radians(a)
        xx=cx+int(11*math.cos(rad)); yy=205+int(11*math.sin(rad))
        bd.point((xx,yy),fill=PAL[4])
im.paste(bridge,(0,0),bridge)
d=ImageDraw.Draw(im)
line([(124,189),(267,189)],10)
line([(124,190),(267,190)],6)
line([(125,192),(266,192)],2)
for x in range(127,265,7):
    line([(x,193),(x+3,193)],8)
    dot(x+4,194,4)
for x in [127,156,185,214,243,265]:
    rect((x-1,189,x+1,205),4)
    for y in range(193,205,4): line([(x-1,y),(x+1,y)],7)
    rect((x-2,189,x+2,190),9)
    rect((x,184,x,188),2)
    rect((x-1,182,x+1,184),12)
    dot(x,182,15)
    dot(x,181,14)
    line([(x-2,185),(x+2,185)],1)
    for y in range(209,233,3):
        xx=x+rng.randrange(-2,3)
        line([(xx-2,y),(xx+2,y)],10 if y>220 else 12)
        dot(xx,y,14 if y<220 else 11)

# Leaf clusters use stepped silhouettes and little connected highlight shapes.
def leaf_cluster(x,y,r,c):
    poly([(x-r,y-1),(x-r+2,y-r//2),(x-r//2,y-r//2),
          (x-2,y-r),(x+2,y-r),(x+3,y-r+2),(x+r-2,y-r//2),
          (x+r-2,y),(x+r,y+2),(x+r-2,y+r//2),
          (x+2,y+r//2),(x,y+r),(x-3,y+r-1),(x-3,y+r//2),
          (x-r,y+r//2)],c)
    if r>3:
        line([(x-r+2,y-2),(x-r+4,y-2),(x-r+4,y-4),(x-1,y-4)],min(c+1,11))
        rect((x+1,y-2,x+2,y-1),min(c+1,11))
        dot(x-1,y+2,max(3,c-1))

# Shore shrubs, behind the large framing trees.
for x in range(-5,389,5):
    if 92<x<254: continue
    y=202+int(3*math.sin(x*.15))
    leaf_cluster(x,y,rng.randrange(4,8),rng.choice([3,4,5]))
    if rng.random()<.7: leaf_cluster(x-1,y-3,3,6)

def tree(side):
    if side=='left':
        # Leaning trunk, branches, and connected foliage crowns.
        poly([(10,213),(15,186),(13,155),(9,131),(14,128),(20,155),
              (22,180),(27,208),(32,215)],1)
        poly([(16,209),(17,180),(16,158),(18,163),(20,190),(24,212)],5)
        line([(18,189),(14,179),(8,169),(2,166)],2,3)
        line([(18,172),(30,155),(45,144),(51,132)],1,4)
        line([(17,156),(28,142),(27,123)],1,3)
        line([(15,147),(4,135),(-2,122)],1,4)
        line([(19,171),(32,155),(44,147)],5)
        crowns=[(-3,108,17),(17,112,19),(33,121,18),(49,132,17),
                (61,144,14),(34,143,17),(11,134,19),(-5,150,16),
                (8,158,11),(49,153,10),(20,124,15)]
    else:
        poly([(353,222),(362,209),(364,184),(362,163),(369,156),
              (371,182),(368,207),(374,220),(382,225)],1)
        poly([(363,216),(366,194),(366,169),(369,163),(369,187),
              (367,212),(370,219)],5)
        line([(366,189),(348,170),(340,151)],1,4)
        line([(367,182),(377,168),(383,150)],1,4)
        line([(366,177),(357,153),(360,141)],1,3)
        line([(367,193),(378,183),(384,177)],1,3)
        line([(364,189),(350,173),(345,159)],5)
        crowns=[(383,134,18),(365,139,18),(348,148,17),(335,157,13),
                (355,161,16),(378,160,19),(385,180,11),(348,172,11)]
    for x,y,r in crowns:
        leaf_cluster(x,y,r,1)
        for _ in range(r*3):
            dx=rng.randrange(-r+1,r); dy=rng.randrange(-r+1,r)
            if dx*dx+dy*dy>r*r: continue
            # Upper-left leaves catch the moon's warm ambient light.
            c=rng.choice([3,4,5,6]) if dy>2 else rng.choice([5,6,7,8,9])
            leaf_cluster(x+dx,y+dy,rng.randrange(2,5),c)
    if side=='left':
        line([(13,201),(14,191),(13,185)],8)
        line([(20,205),(19,197)],6)
    else:
        line([(365,207),(365,201),(366,196)],8)
tree('left')
tree('right')

# Dock, with thick timbers, peach edge lights, and a small seated cat.
poly([(0,209),(68,209),(78,213),(0,215)],2)
rect((0,208,69,209),8)
line([(1,208),(28,208)],11)
line([(34,208),(65,208)],10)
rect((0,212,70,214),1)
for x in [26,71]:
    rect((x,205,x+3,229),1)
    rect((x,205,x+1,224),7)
    rect((x,205,x+2,206),12)
    line([(x+1,214),(x+1,219)],9)
line([(1,211),(22,211)],4)
line([(34,211),(57,211)],5)
line([(4,215),(22,215)],6)

# Cat silhouette and face: every mark is a single native pixel or pixel cluster.
poly([(43,206),(42,201),(43,197),(46,194),(46,188),(48,186),
      (50,190),(54,190),(56,186),(58,187),(58,197),(57,199),
      (58,204),(59,207),(54,207),(53,204),(51,207),(47,207)],0)
poly([(44,202),(45,198),(48,196),(54,196),(56,200),(56,206),
      (53,206),(52,202),(51,206),(47,206),(45,204)],12)
poly([(47,190),(48,188),(50,192),(54,192),(56,188),(57,191),
      (57,196),(54,198),(49,197),(47,195)],13)
rect((49,193,55,195),14)
dot(49,193,1); dot(55,193,1); dot(52,195,6)
line([(51,197),(54,197)],15)
line([(47,201),(47,205),(45,205),(44,204)],14)
line([(54,202),(54,205)],15)
line([(44,202),(41,201),(40,198),(41,196)],0,2)
line([(43,201),(41,200),(41,198)],10)
dot(50,190,8)
dot(55,190,8)

# Foreground shore and clumps of ferns frame the bottom without a hard rectangle.
poly([(0,232),(17,229),(37,232),(55,229),(72,233),(94,235),
      (119,239),(144,237),(164,242),(186,241),(211,246),(234,244),
      (255,241),(276,239),(299,233),(322,233),(344,227),(367,228),
      (383,225),(383,255),(0,255)],1)
line([(318,229),(337,229),(342,227),(365,227)],6)
line([(345,225),(355,225),(360,223),(373,223)],8)
for x in range(0,384,4):
    y=238+int(7*math.sin((x-50)*.012))
    if x>313: y-=8
    for _ in range(2):
        leaf_cluster(x+rng.randrange(-3,4),y+rng.randrange(1,10),
                     rng.randrange(2,5),rng.choice([3,4,5,6,7]))

def fern(x,y,h,flip=1,c=0):
    endx=x+flip*int(h*.55)
    line([(x,y),(x+flip*int(h*.2),y-h//2),(endx,y-h)],c)
    for step in range(3,h-2,3):
        sx=x+flip*int(step*.5); sy=y-step
        size=max(2,int((h-step)*.34))
        poly([(sx,sy+2),(sx-flip*size,sy-size),(sx-flip*size,sy-2),
              (sx,sy)],c)
        poly([(sx,sy),(sx+flip*(size+2),sy-2),(sx+flip*(size+1),sy+1),
              (sx,sy+3)],c)
for x,y,h in [(7,255,28),(32,254,18),(84,255,29),(103,255,18),
              (149,255,15),(231,255,16),(295,255,27),(316,255,19),
              (354,255,28),(379,255,24)]:
    fern(x,y,h,-1)
    fern(x,y,h-3,1)

def flower(x,y):
    line([(x,y+2),(x-1,y+10)],5)
    line([(x-1,y+7),(x-5,y+4)],7)
    rect((x-1,y-3,x+1,y+3),12)
    rect((x-3,y-1,x+3,y+1),13)
    dot(x,y-2,15); dot(x-2,y,14); dot(x+2,y,15)
    dot(x,y+2,14); dot(x,y,9)
for x,y in [(13,230),(24,238),(58,237),(70,245),(131,244),
             (146,248),(353,240),(369,233)]: flower(x,y)
for x,y in [(95,247),(116,251),(331,246),(344,249)]:
    dot(x,y-1,10); line([(x-1,y),(x+1,y)],12); dot(x,y+1,9)

# Store a real indexed PNG: no interpolated colors and no partially opaque pixels.
palette_image = Image.new('P',(1,1))
rgb_palette=[tuple(bytes.fromhex(h[1:])) for h in PAL]
flat=[v for color in rgb_palette for v in color]
palette_image.putpalette(flat+[0]*(768-len(flat)))
indexed=im.quantize(palette=palette_image,dither=Image.Dither.NONE)
indexed.save(OUT,optimize=True)
indexed.resize((1536,1024),Image.Resampling.NEAREST).save(PREVIEW)
check=Image.open(OUT)
assert check.size==(384,256)
assert len(check.getcolors())<=len(PAL)
assert set(check.convert('RGB').getdata()) <= set(rgb_palette)
print(f'Original: {OUT}')
print(f'Preview: {PREVIEW}')
print(f'Verified: {check.width}x{check.height}, {len(check.getcolors())} palette colors, indexed PNG')
