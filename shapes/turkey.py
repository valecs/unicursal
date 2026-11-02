import math
from PIL import Image, ImageDraw, ImageFilter, ImageChops
W, H = 1180, 1100
INK, HOLE = 0, 255
im = Image.new("L", (W, H), 255); d = ImageDraw.Draw(im)

def pt(cx, cy, r, deg):                 # PIL angles: 0 right, 90 down, 270 up
    a = math.radians(deg)
    return (cx + r * math.cos(a), cy + r * math.sin(a))

def thick(draw, p, q, w, fill):
    draw.line([p, q], fill=fill, width=w)
    for c in (p, q):
        draw.ellipse([c[0] - w / 2, c[1] - w / 2, c[0] + w / 2, c[1] + w / 2], fill=fill)

# --- tail fan: 8 feathers over 210 degrees, scalloped tips, notches between
cx, cy, R, n = 590, 620, 380, 8
a0, a1 = 165, 375
d.pieslice([cx - R, cy - R, cx + R, cy + R], a0, a1, fill=INK)
step = (a1 - a0) / n
for k in range(n):
    x, y = pt(cx, cy, R - 10, a0 + (k + 0.5) * step)
    d.ellipse([x - 66, y - 66, x + 66, y + 66], fill=INK)
for k in range(1, n):                    # notches separate the feathers
    a = a0 + k * step
    poly = [pt(cx, cy, 250, a)]
    for r in range(260, 460, 20):
        w = min(30, 0.25 * (r - 250))
        poly.append(pt(cx, cy, r, a - math.degrees(w / r)))
    for r in range(440, 250, -20):
        w = min(30, 0.25 * (r - 250))
        poly.append(pt(cx, cy, r, a + math.degrees(w / r)))
    d.polygon(poly, fill=HOLE)
for k in range(n):                       # a spot near each feather tip
    x, y = pt(cx, cy, R - 95, a0 + (k + 0.5) * step)
    d.ellipse([x - 30, y - 30, x + 30, y + 30], fill=HOLE)

# --- body
body = [380, 560, 800, 960]
d.ellipse(body, fill=INK)

# --- head group (neck, head, beak, wattle), outlined by a gap so it reads
#     in front of the fan
head = Image.new("L", (W, H), 0); h = ImageDraw.Draw(head)
thick(h, (590, 640), (575, 530), 100, 255)                    # neck
h.ellipse([505, 440, 655, 590], fill=255)                     # head
h.polygon([(520, 500), (430, 530), (520, 550)], fill=255)     # beak
h.ellipse([470, 540, 540, 680], fill=255)                     # wattle
gap = head.filter(ImageFilter.MaxFilter(75))                  # ~37 px outline
bodymask = Image.new("L", (W, H), 0)
ImageDraw.Draw(bodymask).ellipse(body, fill=255)
gap = ImageChops.subtract(gap, bodymask)                      # don't cut the body
im.paste(HOLE, mask=gap)
im.paste(INK, mask=head)
d.ellipse([575, 485, 625, 535], fill=HOLE)                    # eye


# --- legs and feet to the bottom edge (entrance)
for x0, x1 in ((530, 500), (650, 680)):
    thick(d, (x0, 930), (x1, 1060), 36, INK)
    for dx in (-60, 0, 60):
        thick(d, (x1, 1060), (x1 + dx, 1100), 28, INK)
# crop to the drawing with a margin (~3 picture cells), legs stay on the edge
x0, y0, x1, y1 = Image.eval(im, lambda v: 255 - v).getbbox()
m = 60
im = im.crop((x0 - m, y0 - m, x1 + m, H))
im.save("turkey.png")
