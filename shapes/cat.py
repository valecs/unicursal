from PIL import Image, ImageDraw
W, H = 600, 680
im = Image.new("L", (W, H), 255); d = ImageDraw.Draw(im)
d.ellipse([130, 290, 450, 670], fill=0)                       # body
d.rectangle([165, 600, 415, 680], fill=0)                     # base on the bottom edge
d.ellipse([170, 125, 410, 335], fill=0)                       # head
d.polygon([(180, 190), (200, 55), (275, 145)], fill=0)        # ears
d.polygon([(400, 190), (380, 55), (305, 145)], fill=0)
d.arc([385, 435, 555, 605], 200, 500, fill=0, width=36)       # tail curling back into the body
d.ellipse([207, 205, 275, 247], fill=255)                     # almond eyes
d.ellipse([305, 205, 373, 247], fill=255)
d.polygon([(262, 264), (318, 264), (290, 304)], fill=255)     # nose
d.polygon([(290, 590), (262, 680), (318, 680)], fill=255)     # gap between front paws
im.save("cat.png")
