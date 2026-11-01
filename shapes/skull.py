from PIL import Image, ImageDraw
W, H = 600, 680
im = Image.new("L", (W, H), 255); d = ImageDraw.Draw(im)
d.ellipse([75, 45, 525, 475], fill=0)                       # cranium
d.rounded_rectangle([165, 380, 435, 590], 45, fill=0)        # jaw
d.polygon([(110, 330), (165, 470), (165, 380)], fill=0)      # cheekbones
d.polygon([(490, 330), (435, 470), (435, 380)], fill=0)
d.rectangle([255, 585, 345, 680], fill=0)                    # neck to bottom edge
d.ellipse([140, 235, 280, 365], fill=255)                    # eye sockets
d.ellipse([320, 235, 460, 365], fill=255)
d.ellipse([268, 375, 304, 415], fill=255)                    # nose (inverted heart)
d.ellipse([296, 375, 332, 415], fill=255)
d.polygon([(268, 400), (332, 400), (300, 450)], fill=255)
d.rounded_rectangle([200, 478, 400, 550], 14, fill=255)      # mouth slot
for x in (232, 288, 344):                                    # 3 teeth, 4 wide gaps
    d.rectangle([x, 476, x + 24, 552], fill=0)
im.save("skull.png")
