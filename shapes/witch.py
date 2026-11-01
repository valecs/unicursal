from PIL import Image, ImageDraw
W, H = 1200, 840
im = Image.new("L", (W, H), 255); d = ImageDraw.Draw(im)
INK, HOLE = 0, 255

def thick(p, q, w):
    d.line([p, q], fill=INK, width=w)
    for c in (p, q):
        d.ellipse([c[0] - w / 2, c[1] - w / 2, c[0] + w / 2, c[1] + w / 2], fill=INK)

def broom_y(x):                       # centreline of the handle
    return 610 - 0.1444 * (x - 260)

# broom handle and bristles (bristles run off the left edge: the entrance)
thick((260, 610), (1160, broom_y(1160)), 36)
d.polygon([(290, 585), (290, 640), (170, 700), (0, 735), (40, 700), (0, 670),
           (45, 640), (0, 610), (45, 580), (0, 545), (170, 535)], fill=INK)
d.rectangle([270, 575, 310, 650], fill=INK)                       # binding

# cape: from the shoulders streaming back, arching down onto the broom
cape_outer = [(700, 300), (600, 270), (480, 255), (370, 280), (270, 330),
              (190, 330), (230, 390), (170, 430), (240, 470), (200, 520),
              (270, 545), (300, 600)]
cape_inner = [(360, 590), (370, 500), (420, 430), (500, 400), (580, 410),
              (630, 450)]
d.polygon(cape_outer + cape_inner + [(700, 420)], fill=INK)

# torso, leaning forward
d.polygon([(570, 570), (690, 555), (790, 345), (690, 300), (620, 380)], fill=INK)
# skirt over the broom, legs and pointy boots below
d.polygon([(560, 545), (710, 530), (750, 650), (630, 670), (550, 650)], fill=INK)
thick((705, 645), (765, 740), 38)
d.polygon([(745, 720), (850, 735), (880, 715), (790, 768), (745, 765)], fill=INK)
thick((610, 655), (565, 745), 38)
d.polygon([(585, 725), (480, 740), (450, 720), (530, 768), (585, 765)], fill=INK)

# head, nose, chin
d.ellipse([715, 215, 830, 330], fill=INK)
d.polygon([(815, 255), (905, 290), (822, 298)], fill=INK)
d.polygon([(785, 320), (845, 352), (805, 312)], fill=INK)
# hat: wide brim, tall crooked cone with a band
d.ellipse([635, 190, 895, 238], fill=INK)
d.polygon([(675, 215), (855, 215), (800, 120), (720, 50), (610, 15), (680, 105)], fill=INK)
d.polygon([(700, 160), (805, 160), (822, 200), (690, 200)], fill=HOLE)   # band

# arm: shoulder -> elbow -> hand on the broom, leaving a gap under it
thick((760, 360), (850, 420), 40)
thick((850, 420), (915, broom_y(915) - 15), 38)
d.ellipse([890, broom_y(915) - 45, 950, broom_y(915) + 15], fill=INK)
im.save("witch.png")
