# Pixel-art ghost on exactly the 12 x 16 grid that letter paper gives at
# --corridor 7, so nothing is lost to resampling.
from PIL import Image
import numpy as np
art = """
............
............
....####....
...######...
..########..
..#..##..#..
..#..##..#..
..########..
..###..###..
..###..###..
..########..
..########..
..########..
..##.##.##..
.....##.....
.....##.....
""".split()
a = np.array([[c == "." for c in row] for row in art], np.uint8) * 255
Image.fromarray(a).resize((a.shape[1] * 20, a.shape[0] * 20), Image.NEAREST).save("ghost.png")
