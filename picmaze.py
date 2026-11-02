#!/usr/bin/env python3
"""
picmaze.py — picture mazes whose solution path draws an image.

Two modes:

  maze       Okamoto & Uehara (CCCG 2009), "How to make a picturesque maze".
             A normal branching maze; the unique solution path fills the
             foreground pixels exactly. Includes their dead-end-pairing
             heuristic so the background isn't visibly finer than the path.

  labyrinth  The branchless variant. Every region is a single corridor:
             the foreground is one open Hamiltonian path (entrance -> exit),
             and each background region is a sealed Hamiltonian cycle that
             looks identical but goes nowhere. No junctions anywhere;
             filling the open corridor reveals the picture.

Core trick (from the paper): if the foreground is 4-connected on an m x n
grid, then doubling each pixel into a 2x2 block always admits a Hamiltonian
cycle -- trace around a random spanning tree of the coarse cells.

Size: the main knob is --corridor, the spacing of corridors on paper in mm.
Together with --paper it sets how many corridors fit, and hence how detailed
the picture and how hard the puzzle. Rough guide for the clear width you get:
   4 mm  sharp pencil / fine pen        (very detailed, long solve)
   6 mm  dull pencil, fine marker
   8 mm  classic markers, crayons       (default)
  11 mm  chunky markers, little kids
Output is a PDF at true scale for the chosen paper: print at 100%.
--cells overrides this (fixed picture resolution, scaled to fit the page).

Usage:
  python3 picmaze.py ghost.png --flow --corridor 8 --out ghost
  python3 picmaze.py image.png --mode maze --paper a4 --corridor 5 --seed 7

Outputs <out>.svg / <out>.pdf (puzzle) and <out>_solution.svg / .pdf.
Requires numpy, Pillow; scipy for component labelling; reportlab for PDF.
"""
import argparse
import random
import os
import sys

import numpy as np
from PIL import Image

try:
    from scipy import ndimage
except ImportError:  # pragma: no cover
    ndimage = None


# ---------------------------------------------------------------- image ----

def load_mask(path, cells, threshold=128, invert=False):
    """Binarize an image to a boolean mask, `cells` cells on the long side.
    True = foreground (dark pixels unless --invert)."""
    im = Image.open(path).convert("L")
    w, h = im.size
    if w >= h:
        n, m = cells, max(1, round(cells * h / w))
    else:
        m, n = cells, max(1, round(cells * w / h))
    im = im.resize((n, m), Image.LANCZOS)
    a = np.asarray(im)
    fg = a < threshold
    return ~fg if invert else fg


def components(mask):
    """4-connected components of a boolean mask -> (labels, count)."""
    if ndimage is not None:
        return ndimage.label(mask)
    # tiny fallback BFS
    lab = np.zeros(mask.shape, int)
    k = 0
    m, n = mask.shape
    for i in range(m):
        for j in range(n):
            if mask[i, j] and not lab[i, j]:
                k += 1
                stack = [(i, j)]
                lab[i, j] = k
                while stack:
                    a, b = stack.pop()
                    for da, db in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                        x, y = a + da, b + db
                        if 0 <= x < m and 0 <= y < n and mask[x, y] and not lab[x, y]:
                            lab[x, y] = k
                            stack.append((x, y))
    return lab, k


def bridge_components(mask):
    """Join all foreground pieces with 1-cell-wide bridges, each the
    shortest background path from the growing main piece to the nearest
    other piece. Returns (new_mask, number_of_bridges)."""
    from collections import deque
    mask = mask.copy()
    m, n = mask.shape
    nb = 0
    while True:
        lab, k = components(mask)
        if k <= 1:
            return mask, nb
        main = 1 + int(np.argmax(np.bincount(lab.ravel())[1:]))
        prev = {}
        q = deque()
        for p in zip(*np.nonzero(lab == main)):
            prev[p] = None
            q.append(p)
        hit = None
        while q and hit is None:
            p = q.popleft()
            for di, dj in NBRS:
                x = (p[0] + di, p[1] + dj)
                if 0 <= x[0] < m and 0 <= x[1] < n and x not in prev:
                    prev[x] = p
                    if mask[x]:          # reached another piece
                        hit = x
                        break
                    q.append(x)
        p = prev[hit]
        while prev[p] is not None:      # paint the bridge
            mask[p] = True
            p = prev[p]
        nb += 1


def largest_component(mask, bridge=True):
    lab, k = components(mask)
    if k == 0:
        sys.exit("error: image has no foreground pixels")
    if k > 1 and bridge:
        mask, nb = bridge_components(mask)
        print(f"note: joined {k} foreground pieces with {nb} thin bridges",
              file=sys.stderr)
        return mask
    if k > 1:
        sizes = np.bincount(lab.ravel())[1:]
        keep = 1 + int(np.argmax(sizes))
        dropped = int(sizes.sum() - sizes[keep - 1])
        print(f"note: foreground has {k} pieces; kept the largest, "
              f"dropped {dropped} cells. Thicken thin parts or join "
              f"pieces in the source image to keep them.", file=sys.stderr)
        return lab == keep
    return mask


# -------------------------------------------------------- spanning tree ----

NBRS = ((0, 1), (1, 0), (0, -1), (-1, 0))


def wilson_tree(cells, rng):
    """Uniform random spanning tree (Wilson's algorithm) of the grid graph
    induced by `cells` (a set of (i,j)). Returns list of edges ((a),(b))."""
    cells = list(cells)
    cellset = set(cells)
    nbr = {c: [(c[0] + di, c[1] + dj) for di, dj in NBRS
                if (c[0] + di, c[1] + dj) in cellset] for c in cells}
    rng.shuffle(cells)
    in_tree = {cells[0]}
    edges = []
    for start in cells[1:]:
        if start in in_tree:
            continue
        nxt = {}
        u = start
        while u not in in_tree:            # loop-erased random walk
            v = rng.choice(nbr[u])
            nxt[u] = v
            u = v
        u = start
        while u not in in_tree:
            in_tree.add(u)
            edges.append((u, nxt[u]))
            u = nxt[u]
    return edges


# ---------------------------------------------------------- passages -------

class Grid:
    """Fine grid of H x W cells. hp[r,c]: passage (r,c)-(r,c+1).
    vp[r,c]: passage (r,c)-(r+1,c)."""

    def __init__(self, H, W):
        self.H, self.W = H, W
        self.hp = np.zeros((H, W - 1), bool)
        self.vp = np.zeros((H - 1, W), bool)

    def set(self, a, b, val=True):
        (r1, c1), (r2, c2) = sorted((a, b))
        if r1 == r2:
            self.hp[r1, c1] = val
        else:
            self.vp[r1, c1] = val

    def get(self, a, b):
        (r1, c1), (r2, c2) = sorted((a, b))
        return self.hp[r1, c1] if r1 == r2 else self.vp[r1, c1]

    def open_nbrs(self, p):
        r, c = p
        out = []
        if c + 1 < self.W and self.hp[r, c]: out.append((r, c + 1))
        if c > 0 and self.hp[r, c - 1]:      out.append((r, c - 1))
        if r + 1 < self.H and self.vp[r, c]: out.append((r + 1, c))
        if r > 0 and self.vp[r - 1, c]:      out.append((r - 1, c))
        return out


def hamiltonian_cycle(region_cells, rng, grid):
    """Carve a Hamiltonian cycle through the 2x2-refinement of a connected
    coarse region into `grid` (the boundary of a thickened spanning tree)."""
    for i, j in region_cells:                       # each block: a 4-cycle
        tl, tr, bl, br = (2*i, 2*j), (2*i, 2*j+1), (2*i+1, 2*j), (2*i+1, 2*j+1)
        for a, b in ((tl, tr), (tr, br), (br, bl), (bl, tl)):
            grid.set(a, b)
    for (a, b) in wilson_tree(region_cells, rng):   # splice along tree edges
        (i1, j1), (i2, j2) = sorted((a, b))
        if i1 == i2:  # horizontal neighbours: c=(i,j) left of d=(i,j+1)
            i, j = i1, j1
            grid.set((2*i, 2*j+1), (2*i+1, 2*j+1), False)
            grid.set((2*i, 2*j+2), (2*i+1, 2*j+2), False)
            grid.set((2*i, 2*j+1), (2*i, 2*j+2))
            grid.set((2*i+1, 2*j+1), (2*i+1, 2*j+2))
        else:         # vertical neighbours
            i, j = i1, j1
            grid.set((2*i+1, 2*j), (2*i+1, 2*j+1), False)
            grid.set((2*i+2, 2*j), (2*i+2, 2*j+1), False)
            grid.set((2*i+1, 2*j), (2*i+2, 2*j))
            grid.set((2*i+1, 2*j+1), (2*i+2, 2*j+1))


def pick_opening(grid, fine_fg, rng):
    """Choose a cycle edge to cut so the path ends sit on the outer wall if
    possible. Returns (entrance, exit, side) with side in 'NSEW' or None."""
    H, W = grid.H, grid.W
    cands = []
    for c in range(W - 1):
        for r, side in ((0, "N"), (H - 1, "S")):
            if fine_fg[r, c] and fine_fg[r, c+1] and grid.hp[r, c]:
                cands.append(((r, c), (r, c+1), side))
    for r in range(H - 1):
        for c, side in ((0, "W"), (W - 1, "E")):
            if fine_fg[r, c] and fine_fg[r+1, c] and grid.vp[r, c]:
                cands.append(((r, c), (r+1, c), side))
    if cands:
        return rng.choice(cands)
    # foreground doesn't touch the border: cut any edge, mark it inside
    pts = list(zip(*np.nonzero(fine_fg)))
    rng.shuffle(pts)
    for p in pts:
        for q in grid.open_nbrs(p):
            if fine_fg[q]:
                return p, q, None
    raise RuntimeError("no foreground edge to open")


def trace_path(grid, start, allowed):
    """Follow corridor from `start` through `allowed` cells (degree <= 2)."""
    path, prev, cur = [start], None, start
    while True:
        nxt = [q for q in grid.open_nbrs(cur) if q != prev and allowed[q]]
        if not nxt:
            return path
        prev, cur = cur, nxt[0]
        path.append(cur)


# ------------------------------------------------------------- modes -------

def build_labyrinth(fg, rng):
    m, n = fg.shape
    grid = Grid(2*m, 2*n)
    fine_fg = np.kron(fg, np.ones((2, 2), bool))
    hamiltonian_cycle(list(zip(*np.nonzero(fg))), rng, grid)
    lab, k = components(~fg)
    for t in range(1, k + 1):                      # sealed decoy cycle(s)
        hamiltonian_cycle(list(zip(*np.nonzero(lab == t))), rng, grid)
    a, b, side = pick_opening(grid, fine_fg, rng)
    grid.set(a, b, False)
    if k > 1:
        print(f"note: background splits into {k} sealed loops", file=sys.stderr)
    return grid, fine_fg, (a, b, side)


def build_maze(fg, rng, reduce_deadends=False):
    """Okamoto-Uehara: Hamiltonian path in the foreground, then a random
    spanning tree of everything else that keeps that path as the solution."""
    m, n = fg.shape
    H, W = 2*m, 2*n
    grid = Grid(H, W)
    fine_fg = np.kron(fg, np.ones((2, 2), bool))
    hamiltonian_cycle(list(zip(*np.nonzero(fg))), rng, grid)
    a, b, side = pick_opening(grid, fine_fg, rng)
    grid.set(a, b, False)

    parent = list(range(H * W))
    def find(x):
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x
    idx = lambda p: p[0] * W + p[1]
    # the solution path is already in the tree
    for r in range(H):
        for c in range(W):
            for q in grid.open_nbrs((r, c)):
                parent[find(idx((r, c)))] = find(idx(q))
    # randomized Kruskal over every edge not internal to the foreground
    edges = []
    for r in range(H):
        for c in range(W):
            for q in ((r, c+1), (r+1, c)):
                if q[0] < H and q[1] < W and not (fine_fg[r, c] and fine_fg[q]):
                    edges.append(((r, c), q))
    rng.shuffle(edges)
    for p, q in edges:
        rp, rq = find(idx(p)), find(idx(q))
        if rp != rq:
            parent[rp] = rq
            grid.set(p, q)
    if reduce_deadends:
        _reduce_deadends(grid, {a, b}, rng)
    return grid, fine_fg, (a, b, side)


def _reduce_deadends(grid, protected, rng):
    """Paper section 3.3: while two dead ends u,v are adjacent, cut u's
    branch off at its junction and join it to v instead. Keeps a tree, so
    the solution is unchanged, and the count of dead ends drops by one."""
    H, W = grid.H, grid.W
    deg = lambda p: len(grid.open_nbrs(p))
    changed = True
    while changed:
        changed = False
        cells = [(r, c) for r in range(H) for c in range(W)]
        rng.shuffle(cells)
        for u in cells:
            if u in protected or deg(u) != 1:
                continue
            for dr, dc in NBRS:
                v = (u[0] + dr, u[1] + dc)
                if not (0 <= v[0] < H and 0 <= v[1] < W):
                    continue
                if v in protected or deg(v) != 1 or grid.get(u, v):
                    continue
                # walk from u to its first junction b_u
                prev, cur = None, u
                while True:
                    nx = [q for q in grid.open_nbrs(cur) if q != prev]
                    if len(grid.open_nbrs(cur)) >= 3 or not nx:
                        break
                    prev, cur = cur, nx[0]
                if len(grid.open_nbrs(cur)) < 3 or prev is None:
                    continue  # whole thing is one corridor; leave it
                if v == cur:
                    continue
                grid.set(prev, cur, False)   # wall off P_u at b_u
                grid.set(u, v, True)         # and splice it onto v
                changed = True
                break


# ------------------------------------------------------------ render -------

def render_svg(grid, cell, fill_cells=None, opening=None, margin=2.0,
               stroke=0.18, color="#000", fill="#e0218a"):
    """Return SVG text. Units: `cell` per fine cell (in pt for PDF parity)."""
    H, W = grid.H, grid.W
    a, b, side = opening
    out = []
    Wt, Ht = W * cell + 2 * margin * cell, H * cell + 2 * margin * cell
    ox = oy = margin * cell
    out.append(f'<svg xmlns="http://www.w3.org/2000/svg" '
               f'width="{Wt:.2f}pt" height="{Ht:.2f}pt" '
               f'viewBox="0 0 {Wt:.3f} {Ht:.3f}">')
    out.append(f'<rect width="100%" height="100%" fill="#fff"/>')
    if fill_cells is not None:
        # one rect per run of filled cells in a row
        parts = []
        for r in range(H):
            c = 0
            while c < W:
                if fill_cells[r, c]:
                    s = c
                    while c < W and fill_cells[r, c]:
                        c += 1
                    parts.append(f'M{ox+s*cell:.3f} {oy+r*cell:.3f}h{(c-s)*cell:.3f}'
                                 f'v{cell:.3f}h{-(c-s)*cell:.3f}z')
                else:
                    c += 1
        out.append(f'<path fill="{fill}" d="{"".join(parts)}"/>')
    segs = []
    sw = stroke * cell
    # interior walls, merged into runs
    for c in range(W - 1):                   # vertical walls between columns
        r = 0
        while r < H:
            if not grid.hp[r, c]:
                s = r
                while r < H and not grid.hp[r, c]:
                    r += 1
                x = ox + (c + 1) * cell
                segs.append(f'M{x:.3f} {oy+s*cell:.3f}V{oy+r*cell:.3f}')
            else:
                r += 1
    for r in range(H - 1):
        c = 0
        while c < W:
            if not grid.vp[r, c]:
                s = c
                while c < W and not grid.vp[r, c]:
                    c += 1
                y = oy + (r + 1) * cell
                segs.append(f'M{ox+s*cell:.3f} {y:.3f}H{ox+c*cell:.3f}')
            else:
                c += 1
    # outer border, with gaps at the entrance/exit if they're on it
    gaps = set()
    if side:
        for p in (a, b):
            gaps.add((side, p[1] if side in "NS" else p[0]))
    def border(side_, n, fixed, horiz):
        k = 0
        while k < n:
            if (side_, k) in gaps:
                k += 1
                continue
            s = k
            while k < n and (side_, k) not in gaps:
                k += 1
            if horiz:
                segs.append(f'M{ox+s*cell:.3f} {fixed:.3f}H{ox+k*cell:.3f}')
            else:
                segs.append(f'M{fixed:.3f} {oy+s*cell:.3f}V{oy+k*cell:.3f}')
    border("N", W, oy, True)
    border("S", W, oy + H * cell, True)
    border("W", H, ox, False)
    border("E", H, ox + W * cell, False)
    out.append(f'<path fill="none" stroke="{color}" stroke-width="{sw:.3f}" '
               f'stroke-linecap="square" d="{"".join(segs)}"/>')
    # entrance / exit markers: arrows outside the border, or dots inside
    for p in (a, b):
        cx, cy = ox + (p[1] + .5) * cell, oy + (p[0] + .5) * cell
        if side:
            d = {"N": (0, -1), "S": (0, 1), "W": (-1, 0), "E": (1, 0)}[side]
            cx, cy = cx + d[0] * cell, cy + d[1] * cell
        rr = .32 * cell
        out.append(f'<path fill="#1a9e3a" d="M{cx:.3f} {cy-rr:.3f}L{cx+rr:.3f} {cy:.3f}'
                   f'L{cx:.3f} {cy+rr:.3f}L{cx-rr:.3f} {cy:.3f}z"/>')
    out.append('</svg>')
    return "\n".join(out)


# ------------------------------------------------------ curvy render -------
#
# Same maze topology, drawn on a bent sheet. Two ingredients:
#   1. corner rounding: every wall is split into chains between junctions /
#      tips, and each chain is Chaikin-smoothed with its ends pinned, so
#      walls still meet exactly where they did;
#   2. a smooth random displacement field (a sum of random Fourier modes),
#      scaled so its Jacobian norm is below 1 everywhere. A map p -> p + d(p)
#      with Lipschitz(d) < 1 is injective, so walls can never cross or fold.

def make_warp(H, W, rng, strength=0.6, scale=10.0, nmodes=40, pin=3.0):
    """Return f: (N,2) array of (x, y) lattice coords -> warped coords."""
    nr = np.random.default_rng(rng.randrange(2**32))
    theta = nr.uniform(0, 2*np.pi, nmodes)
    lam = scale * np.exp(nr.uniform(np.log(0.6), np.log(2.5), nmodes))
    kmag = 2*np.pi / lam
    K = np.stack([np.cos(theta), np.sin(theta)], 1) * kmag[:, None]
    phi = nr.uniform(0, 2*np.pi, nmodes)
    ang = nr.uniform(0, 2*np.pi, nmodes)
    U = np.stack([np.cos(ang), np.sin(ang)], 1) / kmag[:, None]

    def raw(P):
        D = np.sin(P @ K.T + phi) @ U
        if pin > 0:  # taper to zero at the outer border so the frame stays square
            dist = np.minimum.reduce([P[:, 0], W - P[:, 0], P[:, 1], H - P[:, 1]])
            t = np.clip(dist / pin, 0, 1)
            D = D * (t * t * (3 - 2 * t))[:, None]
        return D

    if strength <= 0:
        return lambda P: P
    # numerical Lipschitz constant on a fine sample grid
    h = 0.25
    xs, ys = np.arange(0, W + h, h), np.arange(0, H + h, h)
    X, Y = np.meshgrid(xs, ys)
    D = raw(np.stack([X.ravel(), Y.ravel()], 1)).reshape(len(ys), len(xs), 2)
    dDy, dDx = np.gradient(D, h, h, axis=(0, 1))
    J = np.stack([dDx, dDy], -1)                      # [..., component, dir]
    smax = np.linalg.norm(J, ord=2, axis=(-2, -1)).max()
    s = strength / smax
    return lambda P: P + s * raw(P)


def chaikin(P, iters, closed=False):
    for _ in range(iters):
        if len(P) < 3 and not closed:
            return P
        Q = np.roll(P, -1, 0) if closed else P[1:]
        A = P if closed else P[:-1]
        cuts = np.empty((2 * len(A), 2))
        cuts[0::2] = 0.75 * A + 0.25 * Q
        cuts[1::2] = 0.25 * A + 0.75 * Q
        P = cuts if closed else np.vstack([P[:1], cuts[1:-1], P[-1:]])
    return P


def wall_chains(grid, gaps):
    """Walls as polylines in lattice (x, y) coords, split at junctions/tips."""
    H, W = grid.H, grid.W
    adj = {}
    def add(a, b):
        adj.setdefault(a, []).append(b)
        adj.setdefault(b, []).append(a)
    for r in range(H):
        for c in range(W - 1):
            if not grid.hp[r, c]:
                add((c + 1, r), (c + 1, r + 1))
    for r in range(H - 1):
        for c in range(W):
            if not grid.vp[r, c]:
                add((c, r + 1), (c + 1, r + 1))
    for c in range(W):
        if ("N", c) not in gaps: add((c, 0), (c + 1, 0))
        if ("S", c) not in gaps: add((c, H), (c + 1, H))
    for r in range(H):
        if ("W", r) not in gaps: add((0, r), (0, r + 1))
        if ("E", r) not in gaps: add((W, r), (W, r + 1))

    seen = set()
    chains = []
    def walk(a, b):
        pts = [a, b]
        seen.add(frozenset((a, b)))
        prev, cur = a, b
        while len(adj[cur]) == 2:
            nxt = adj[cur][0] if adj[cur][1] == prev else adj[cur][1]
            e = frozenset((cur, nxt))
            if e in seen:
                break
            seen.add(e)
            pts.append(nxt)
            prev, cur = cur, nxt
        return pts
    for p, nb in adj.items():
        if len(nb) != 2:
            for q in nb:
                if frozenset((p, q)) not in seen:
                    chains.append((walk(p, q), False))
    for p, nb in adj.items():              # leftover pure loops
        for q in nb:
            if frozenset((p, q)) not in seen:
                pts = walk(p, q)
                chains.append((pts[:-1] if pts[0] == pts[-1] else pts, True))
    return chains


def render_curvy_svg(grid, cell, opening, warp, smooth=3, path=None,
                     margin=2.0, stroke=0.16, color="#000", fill="#e0218a",
                     path_width=0.78):
    H, W = grid.H, grid.W
    a, b, side = opening
    ox = oy = margin
    Wt, Ht = (W + 2 * margin) * cell, (H + 2 * margin) * cell
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{Wt:.2f}pt" '
           f'height="{Ht:.2f}pt" viewBox="0 0 {Wt:.3f} {Ht:.3f}">',
           '<rect width="100%" height="100%" fill="#fff"/>']
    def d_of(P, closed=False):
        P = (P + [ox, oy]) * cell
        s = "M" + "L".join(f"{x:.2f} {y:.2f}" for x, y in P)
        return s + ("Z" if closed else "")
    out_dir = {"N": (0, -1), "S": (0, 1), "W": (-1, 0), "E": (1, 0)}.get(side)

    if path is not None:  # solution as a fat smoothed centreline
        P = np.array([(c + .5, r + .5) for r, c in path], float)
        if out_dir:
            P = np.vstack([P[:1] + out_dir, P, P[-1:] + out_dir])
        P = warp(chaikin(P, smooth))
        out.append(f'<path fill="none" stroke="{fill}" stroke-width='
                   f'"{path_width*cell:.3f}" stroke-linecap="round" '
                   f'stroke-linejoin="round" d="{d_of(P)}"/>')

    gaps = set()
    if side:
        for p in (a, b):
            gaps.add((side, p[1] if side in "NS" else p[0]))
    parts = []
    for pts, closed in wall_chains(grid, gaps):
        P = warp(chaikin(np.array(pts, float), smooth, closed))
        parts.append(d_of(P, closed))
    out.append(f'<path fill="none" stroke="{color}" stroke-width="{stroke*cell:.3f}" '
               f'stroke-linecap="round" stroke-linejoin="round" d="{"".join(parts)}"/>')

    rr = .32
    for p in (a, b):
        cx, cy = p[1] + .5, p[0] + .5
        if out_dir:
            cx, cy = cx + 1.1 * out_dir[0], cy + 1.1 * out_dir[1]
        C = warp(np.array([[cx, cy]]))[0]
        D = np.array([[0, -rr], [rr, 0], [0, rr], [-rr, 0]]) + C
        out.append(f'<path fill="#1a9e3a" d="{d_of(D, True)}"/>')
    out.append('</svg>')
    return "\n".join(out)


def svg_to_pdf(svg_path, pdf_path):
    try:
        import subprocess
        subprocess.run(["rsvg-convert", "-f", "pdf", "-o", pdf_path, svg_path],
                       check=True)
        return True
    except Exception:
        try:
            from svglib.svglib import svg2rlg
            from reportlab.graphics import renderPDF
            renderPDF.drawToFile(svg2rlg(svg_path), pdf_path)
            return True
        except Exception:
            return False


# ------------------------------------------------------------- paper -------

PAPERS = {"letter": (215.9, 279.4), "legal": (215.9, 355.6),
          "tabloid": (279.4, 431.8), "a4": (210.0, 297.0), "a3": (297.0, 420.0)}
MARGIN = 1.5      # frame-to-edge-of-drawing margin, in corridors
PT_PER_MM = 72 / 25.4


def parse_paper(spec):
    spec = spec.lower().strip()
    if spec in PAPERS:
        return PAPERS[spec]
    import re
    m = re.fullmatch(r"([\d.]+)\s*x\s*([\d.]+)\s*(mm|in)?", spec)
    if not m:
        raise SystemExit(f"error: unknown paper '{spec}' (try letter, a4, "
                         f"or e.g. 8.5x11in / 200x200mm)")
    k = 25.4 if m[3] == "in" else 1.0
    return float(m[1]) * k, float(m[2]) * k


def cells_to_fit(img_w, img_h, nmax, mmax):
    """Largest --cells value whose coarse grid fits in nmax x mmax."""
    for c in range(max(nmax, mmax), 0, -1):
        if img_w >= img_h:
            n, m = c, max(1, round(c * img_h / img_w))
        else:
            m, n = c, max(1, round(c * img_w / img_h))
        if n <= nmax and m <= mmax:
            return c
    return 0


def place_on_page(svg, paper_mm, footer=None):
    """Centre a renderer's SVG on a page of the given size (true scale)."""
    import re
    PW, PH = paper_mm[0] * PT_PER_MM, paper_mm[1] * PT_PER_MM
    m = re.match(r'<svg xmlns="http://www.w3.org/2000/svg"\s+width="([\d.]+)pt"\s+'
                 r'height="([\d.]+)pt"\s+viewBox="0 0 ([\d.]+) ([\d.]+)">', svg)
    w, h = float(m[1]), float(m[2])
    x, y = (PW - w) / 2, (PH - h) / 2
    head = (f'<svg xmlns="http://www.w3.org/2000/svg" width="{PW:.2f}pt" '
            f'height="{PH:.2f}pt" viewBox="0 0 {PW:.3f} {PH:.3f}">'
            f'<rect width="100%" height="100%" fill="#fff"/>')
    if footer:
        head += (f'<text x="{PW/2:.2f}" y="{PH - 14:.2f}" text-anchor="middle" '
                 f'font-family="sans-serif" font-size="7" fill="#999">{footer}</text>')
    head += (f'<svg x="{x:.3f}" y="{y:.3f}" width="{w:.3f}" height="{h:.3f}" '
             f'viewBox="0 0 {m[3]} {m[4]}" overflow="visible">')
    return head + svg[m.end():] + "</svg>"


# --------------------------------------------------------------- main ------

PRESET_META = {"label", "tool"}          # display-only keys in presets.toml


def load_presets(path):
    import tomllib
    with open(path, "rb") as f:
        return tomllib.load(f).get("preset", {})


def parse_args_with_preset(ap, argv=None):
    """Parse argv; a --preset supplies defaults that explicit flags override."""
    pre = argparse.ArgumentParser(add_help=False)
    pre.add_argument("--preset")
    pre.add_argument("--presets", default=os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "presets.toml"))
    pre.add_argument("--list-presets", action="store_true")
    known, _ = pre.parse_known_args(argv)
    if known.list_presets:
        for name, p in load_presets(known.presets).items():
            opts = ", ".join(f"{k}={v}" for k, v in p.items() if k not in PRESET_META)
            print(f"{name:10} {p.get('label', ''):8} {p.get('tool', ''):26} {opts}")
        sys.exit(0)
    if known.preset:
        presets = load_presets(known.presets)
        if known.preset not in presets:
            ap.error(f"unknown preset '{known.preset}' (have: {', '.join(presets)})")
        dests = {a.dest for a in ap._actions}
        vals = {k: v for k, v in presets[known.preset].items() if k not in PRESET_META}
        bad = set(vals) - dests
        if bad:
            ap.error(f"preset '{known.preset}' sets unknown options: {', '.join(sorted(bad))}")
        ap.set_defaults(**vals)
    return ap.parse_args(argv)


def build_parser():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("image")
    ap.add_argument("--preset", help="difficulty preset from presets.toml "
                    "(e.g. easy, medium, hard); explicit flags override it")
    ap.add_argument("--presets", help="presets file (default: presets.toml "
                    "next to this script)")
    ap.add_argument("--list-presets", action="store_true",
                    help="list presets and exit")
    ap.add_argument("--corridor", type=float, default=8.0,
                    help="corridor spacing on paper, mm (sets size/difficulty; "
                         "see guide above). Default 8")
    ap.add_argument("--paper", default="letter",
                    help="letter, legal, tabloid, a4, a3, or e.g. 8.5x11in, "
                         "200x250mm. See --orientation.")
    ap.add_argument("--orientation", choices=["auto", "portrait", "landscape"],
                    default="auto", help="page orientation; auto follows the image")
    ap.add_argument("--paper-margin", type=float, default=12.7,
                    help="blank margin at the paper edge, mm (default 12.7 = 0.5in)")
    ap.add_argument("--cells", type=int, default=None,
                    help="override: fixed picture resolution (long side, before "
                         "2x2 doubling); corridor size then follows from the paper")
    ap.add_argument("--mode", choices=["labyrinth", "maze"], default="labyrinth")
    ap.add_argument("--seed", type=int, default=None)
    ap.add_argument("--threshold", type=int, default=128)
    ap.add_argument("--invert", action="store_true",
                    help="treat light pixels as the picture")
    ap.add_argument("--no-bridges", action="store_true",
                    help="keep only the largest piece instead of joining pieces")
    ap.add_argument("--deadend-fix", action="store_true",
                    help="maze mode: apply the dead-end pairing heuristic. Off by "
                         "default: it can open walls into the solution path")
    ap.add_argument("--fill", default="#e0218a", help="solution fill colour")
    ap.add_argument("--curvy", action="store_true",
                    help="round the corners and bend the sheet so paths curve")
    ap.add_argument("--warp", type=float, default=0.55,
                    help="curvy: max local stretch/squeeze, 0..<1; corridors never "
                         "get narrower than (1 - warp) of a cell. 0 = round corners only")
    ap.add_argument("--warp-scale", type=float, default=14.0,
                    help="curvy: typical bend wavelength, in maze cells")
    ap.add_argument("--smooth", type=int, default=3,
                    help="curvy: corner-rounding passes (0 = sharp)")
    ap.add_argument("--flow", action="store_true",
                    help="labyrinth only: throw away the grid and relax the "
                         "corridors into flowing, organic curves (slow-ish)")
    ap.add_argument("--flow-iters", type=int, default=1500)
    ap.add_argument("--flow-length", type=float, default=0.85,
                    help="flow: corridor length kept, <1 opens voids and "
                         "lets paths meander (0.7-1.0)")
    ap.add_argument("--flow-bend", type=float, default=0.3,
                    help="flow: stiffness; higher = broader, smoother curves")
    ap.add_argument("--flow-tol", type=float, default=1.0,
                    help="flow: how far (in corridors) the picture's edge may "
                         "drift; lower keeps thin parts crisper (0.3-1.5)")
    ap.add_argument("--void", default="#111", help="flow: colour of the voids")
    ap.add_argument("--ink-saver", action="store_true",
                    help="flow: light grey voids with outlined corridors")
    ap.add_argument("--out", default="picmaze")
    return ap


def main():
    ap = build_parser()
    args = parse_args_with_preset(ap)
    if args.ink_saver and args.void == "#111":
        args.void = "#cfcfcf"
    if args.flow and args.mode != "labyrinth":
        ap.error("--flow needs --mode labyrinth (it relies on there being no junctions)")

    seed = args.seed if args.seed is not None else random.randrange(10**6)
    rng = random.Random(seed)

    paper = parse_paper(args.paper)
    img_w, img_h = Image.open(args.image).size
    landscape = {"auto": img_w > img_h, "portrait": False,
                 "landscape": True}[args.orientation]
    if landscape != (paper[0] > paper[1]):
        paper = (paper[1], paper[0])
    avail = (paper[0] - 2 * args.paper_margin, paper[1] - 2 * args.paper_margin)
    if args.cells is None:
        pitch = args.corridor
        nmax = int(avail[0] / pitch - 2 * MARGIN) // 2
        mmax = int(avail[1] / pitch - 2 * MARGIN) // 2
        cells = cells_to_fit(img_w, img_h, nmax, mmax)
        if cells < 4:
            sys.exit(f"error: {pitch} mm corridors leave only {nmax}x{mmax} picture "
                     f"cells on this paper; use a smaller --corridor or bigger --paper")
    else:
        cells, pitch = args.cells, None

    fg = largest_component(load_mask(args.image, cells,
                                     args.threshold, args.invert),
                           bridge=not args.no_bridges)
    if args.mode == "labyrinth":
        grid, fine_fg, opening = build_labyrinth(fg, rng)
    else:
        grid, fine_fg, opening = build_maze(fg, rng, args.deadend_fix)

    # solution = cells on the open corridor from the entrance
    path = trace_path(grid, opening[0], fine_fg)
    sol = np.zeros_like(fine_fg)
    for p in path:
        sol[p] = True
    assert sol.sum() == fine_fg.sum(), "solution does not cover the picture"

    if pitch is None:
        pitch = min(avail[0] / (grid.W + 2 * MARGIN), avail[1] / (grid.H + 2 * MARGIN))
    cell = pitch * PT_PER_MM
    footer = (f"{grid.W // 2}x{grid.H // 2} cells · {pitch:.1f} mm corridors · "
              f"seed {seed}")
    print(f"paper {paper[0]:.0f}x{paper[1]:.0f} mm, picture {grid.W // 2}x{grid.H // 2} "
          f"cells, corridor spacing {pitch:.2f} mm, seed {seed}", file=sys.stderr)

    def emit(name, svg):
        with open(name + ".svg", "w") as f:
            f.write(place_on_page(svg, paper, footer))
        ok = svg_to_pdf(name + ".svg", name + ".pdf")
        print(f"wrote {name}.svg" + (f" and {name}.pdf" if ok else ""))
    if args.flow:
        import flow
        strands = flow.extract_strands(grid, path)
        strands = flow.relax(strands, fine_fg, grid.H, grid.W,
                             iters=args.flow_iters, length=args.flow_length,
                             bend=args.flow_bend, tol=args.flow_tol,
                             pic=0.3 if args.flow_tol >= 1 else 0.6,
                             noise=0.02, seed=rng.randrange(2**31),
                             log=lambda m: print(m, file=sys.stderr))
        nx = flow.count_crossings(strands)
        if nx:
            print(f"warning: {nx} strand crossings; try a smaller step", file=sys.stderr)
        side = opening[2]
        dirs = {"N": (0, -1), "S": (0, 1), "W": (-1, 0), "E": (1, 0)}
        ent = (dirs[side], dirs[side]) if side else None
        for name, show in ((args.out, False), (args.out + "_solution", True)):
            svg = flow.render_flow_svg(strands, grid.H, grid.W, cell, void=args.void,
                                       fill=args.fill, show_solution=show,
                                       entrance=ent, margin=MARGIN,
                                       outline=args.ink_saver)
            emit(name, svg)
            if not show:
                puzzle_svg = svg
        wid = flow.corridor_widths(strands, grid.H, grid.W) * pitch
        print(f"clear corridor width: narrowest {wid.min():.1f} mm, "
              f"1% narrower than {np.percentile(wid, 1):.1f} mm, "
              f"typical {np.median(wid):.1f} mm", file=sys.stderr)
        try:
            good, k = flow.verify_regions(puzzle_svg, grid.H, grid.W, cell,
                                          len(strands), margin=MARGIN)
            print(("verified: " if good else "WARNING: ") +
                  f"{k} open regions for {len(strands)} strands", file=sys.stderr)
        except Exception as e:  # rsvg-convert missing, etc.
            print(f"(skipped region check: {e})", file=sys.stderr)
        return
    if args.curvy:
        warp = make_warp(grid.H, grid.W, rng, args.warp, args.warp_scale)
    for name, fill in ((args.out, None), (args.out + "_solution", sol)):
        if args.curvy:
            svg = render_curvy_svg(grid, cell, opening, warp, args.smooth,
                                   path=path if fill is not None else None,
                                   fill=args.fill, margin=MARGIN)
        else:
            svg = render_svg(grid, cell, fill_cells=fill, opening=opening,
                             fill=args.fill, margin=MARGIN)
        emit(name, svg)
    squeeze = (1 - args.warp) if args.curvy else 1.0
    stroke = 0.16 if args.curvy else 0.18
    print(f"clear corridor width: {pitch * (1 - stroke):.1f} mm typical" +
          (f", never below {pitch * (squeeze - stroke):.1f} mm (lower --warp to raise)"
           if args.curvy else ""), file=sys.stderr)
    print(f"grid {grid.H}x{grid.W}, path length {int(sol.sum())}")


if __name__ == "__main__":
    main()
