"""
flow.py — "organic" relaxation of a picmaze labyrinth.

In labyrinth mode every cell has exactly two openings, so the whole maze is
nothing but a family of disjoint curves: the solution strand (open) plus the
sealed decoy loops. The walls are just "the boundaries between neighbouring
strands". So we can throw the grid away entirely, move the strands freely,
and recover walls afterwards.

Dynamics (after Pedersen & Singh, "Organic labyrinths and mazes", 2006):
  * fairing      pulls each sample toward the midpoint of its neighbours
                 (curve shortening -> smooth, flowing strands)
  * repulsion    keeps samples on different strands (or far apart on the
                 same strand) at least `R` apart -> corridors keep a width
  * picture      the solution strand is pushed back inside the picture's
                 signed-distance field, decoys are pushed out, so the image
                 survives
  * noise        a little Brownian jitter to break grid symmetry
  * resampling   keeps sample spacing ~0.5 corridor widths
Steps are capped well below the repulsion radius, and adjacent samples stay
within reach of each other, so strands never jump through one another;
crossings are also checked explicitly at the end.

Rendering: the frame is filled with the "void" colour, each strand is drawn
as a white tube of radius `rmax`, and walls are Voronoi ridges between samples
that are not close neighbours along the same strand, clipped to the tubes.
Where strands pull apart further than 2*rmax, void shows through.
"""
import numpy as np
from scipy.spatial import cKDTree, Voronoi
from scipy import ndimage


# ------------------------------------------------------------ strands ------

def extract_strands(grid, path):
    """Return [(points(N,2) in lattice xy, closed, is_solution)]."""
    H, W = grid.H, grid.W
    seen = np.zeros((H, W), bool)
    strands = []
    P = np.array([(c + .5, r + .5) for r, c in path], float)
    for r, c in path:
        seen[r, c] = True
    strands.append([P, False, True])
    for r0 in range(H):
        for c0 in range(W):
            if seen[r0, c0]:
                continue
            loop, prev, cur = [], None, (r0, c0)
            while True:
                loop.append(cur)
                seen[cur] = True
                nb = [q for q in grid.open_nbrs(cur) if q != prev]
                nxt = nb[0]
                if nxt == (r0, c0):
                    break
                prev, cur = cur, nxt
            strands.append([np.array([(c + .5, r + .5) for r, c in loop], float),
                            True, False])
    return strands


def resample(P, closed, lo, hi):
    """Insert midpoints on long segments, drop points on short ones."""
    Q = np.vstack([P, P[:1]]) if closed else P
    seg = np.linalg.norm(np.diff(Q, axis=0), axis=1)
    out = [P[0]]
    n = len(P)
    for i in range(len(seg)):
        a, b = Q[i], Q[i + 1]
        if seg[i] > hi:
            k = int(np.ceil(seg[i] / hi))
            for t in range(1, k):
                out.append(a + (b - a) * t / k)
        if i + 1 < n:
            out.append(b)
    P = np.array(out)
    if len(P) > 6:  # merge too-close pairs (keep open ends pinned)
        keep = np.ones(len(P), bool)
        last = 0
        for i in range(1, len(P)):
            if np.linalg.norm(P[i] - P[last]) < lo and not (not closed and i == len(P) - 1):
                keep[i] = False
            else:
                last = i
        P = P[keep]
    return P


# ---------------------------------------------------------- dynamics -------

def relax(strands, fine_fg, H, W, iters=600, R=1.0, seg=0.5, length=1.0,
          spring=0.5, bend=0.15, rep=0.5, pic=0.3, tol=1.0, noise=0.01,
          step=0.04, seed=0, log=None):
    """Elastic-filament relaxation. Each strand keeps (roughly) its length,
    so smoothing can't shrink it; it buckles into flowing meanders instead.
    `length` > 1 slowly grows strands (more meander, more crowding);
    < 1 lets them retract (more void)."""
    rng = np.random.default_rng(seed)
    inside = ndimage.distance_transform_edt(fine_fg)
    outside = ndimage.distance_transform_edt(~fine_fg)
    phi = inside - outside
    gy, gx = np.gradient(phi)

    def sample(Fld, P):
        return ndimage.map_coordinates(Fld, [P[:, 1] - .5, P[:, 0] - .5],
                                       order=1, mode="nearest")

    for s in strands:  # uniform spacing `seg` along each strand
        P, closed = s[0], s[1]
        Q = np.vstack([P, P[:1]]) if closed else P
        d = np.concatenate([[0], np.cumsum(np.linalg.norm(np.diff(Q, axis=0), axis=1))])
        n = max(3, int(round(d[-1] / seg)))
        t = np.linspace(0, d[-1], n + (0 if closed else 1), endpoint=not closed)
        s[0] = np.stack([np.interp(t, d, Q[:, 0]), np.interp(t, d, Q[:, 1])], 1)

    lens = [len(s[0]) for s in strands]
    off = np.concatenate([[0], np.cumsum(lens)])
    P = np.vstack([s[0] for s in strands])
    N = len(P)
    cl_s = np.array([s[1] for s in strands])
    sid = np.repeat(np.arange(len(strands)), lens)
    loc = np.concatenate([np.arange(n) for n in lens])
    nlen = np.repeat(lens, lens)
    closed = np.repeat(cl_s, lens)
    is_sol = np.repeat([s[2] for s in strands], lens)
    first, last = off[:-1], off[1:] - 1
    prv, nxt = np.arange(N) - 1, np.arange(N) + 1
    prv[first] = np.where(cl_s, last, first)
    nxt[last] = np.where(cl_s, first, last)
    endpt = np.zeros(N, bool)
    endpt[first[~cl_s]] = endpt[last[~cl_s]] = True
    # segments: i -> nxt[i], skipping the missing one at open ends
    si = np.arange(N)[nxt != np.arange(N)]
    sj = nxt[si]
    rest = np.full(len(si), seg)

    for it in range(iters):
        rest_now = rest * (1 + (length - 1) * min(1, it / (0.7 * iters)))
        F = np.zeros_like(P)
        e = P[sj] - P[si]
        L = np.linalg.norm(e, axis=1) + 1e-12
        f = (spring * (L - rest_now) / L)[:, None] * e
        np.add.at(F, si, f)
        np.add.at(F, sj, -f)

        Lp = 0.5 * (P[prv] + P[nxt]) - P
        Lp[endpt] = 0
        F -= bend * (0.5 * (Lp[prv] + Lp[nxt]) - Lp)

        pairs = cKDTree(P).query_pairs(R, output_type="ndarray")
        if len(pairs):
            i, j = pairs[:, 0], pairs[:, 1]
            di = np.abs(loc[i] - loc[j])
            di = np.where(closed[i], np.minimum(di, nlen[i] - di), di)
            ok = ~((sid[i] == sid[j]) & (di <= 2))
            i, j = i[ok], j[ok]
            d = P[i] - P[j]
            r = np.linalg.norm(d, axis=1) + 1e-9
            f = (rep * (R - r) / R / r)[:, None] * d
            np.add.at(F, i, f)
            np.add.at(F, j, -f)

        ph = sample(phi, P)
        g = np.stack([sample(gx, P), sample(gy, P)], 1)
        g /= np.linalg.norm(g, axis=1, keepdims=True) + 1e-9
        push = np.where(is_sol, np.clip(-tol - ph, 0, None), np.clip(ph - tol, 0, None))
        F += pic * push[:, None] * np.where(is_sol[:, None], g, -g)

        F += noise * rng.standard_normal(F.shape)
        m = np.linalg.norm(F, axis=1, keepdims=True)
        F *= np.minimum(1, step / (m + 1e-12))
        F[endpt] = 0
        P += F
        np.clip(P[:, 0], 0.5, W - 0.5, out=P[:, 0])
        np.clip(P[:, 1], 0.5, H - 0.5, out=P[:, 1])
        if log and it % 100 == 0:
            log(f"iter {it}: mean seg {L.mean():.3f}, close pairs {len(pairs)}")

    for k, s in enumerate(strands):
        s[0] = P[off[k]:off[k + 1]].copy()
    return strands


# ------------------------------------------------------------ checks -------

def count_crossings(strands):
    """Segment-segment intersections between non-adjacent segments."""
    segs, sid, loc, nl, cl = [], [], [], [], []
    for k, (P, closed, _) in enumerate(strands):
        Q = np.vstack([P, P[:1]]) if closed else P
        n = len(Q) - 1
        segs.append(np.stack([Q[:-1], Q[1:]], 1))
        sid += [k] * n; loc += list(range(n)); nl += [n] * n; cl += [closed] * n
    S = np.concatenate(segs)
    sid, loc, nl, cl = map(np.array, (sid, loc, nl, cl))
    mid = S.mean(1)
    L = np.linalg.norm(S[:, 1] - S[:, 0], axis=1).max()
    pairs = cKDTree(mid).query_pairs(L + 1e-6, output_type="ndarray")
    if not len(pairs):
        return 0
    i, j = pairs.T
    di = np.abs(loc[i] - loc[j])
    di = np.where(cl[i], np.minimum(di, nl[i] - di), di)
    keep = ~((sid[i] == sid[j]) & (di <= 1))
    i, j = i[keep], j[keep]
    a, b, c, d = S[i, 0], S[i, 1], S[j, 0], S[j, 1]
    def orient(p, q, r):
        return np.sign((q[:, 0] - p[:, 0]) * (r[:, 1] - p[:, 1]) -
                       (q[:, 1] - p[:, 1]) * (r[:, 0] - p[:, 0]))
    x = (orient(a, b, c) * orient(a, b, d) < 0) & (orient(c, d, a) * orient(c, d, b) < 0)
    return int(x.sum())


# ------------------------------------------------------------ render -------

def render_flow_svg(strands, H, W, cell, rmax=0.55, wall=0.14, margin=2.0,
                    void="#111", fill="#f07a14", show_solution=False,
                    gap_arc=1.05, entrance=None, outline=False):
    """outline: draw each corridor's edge where it meets a void, so voids can
    be a light colour (ink saver) without corridors visually leaking into them."""
    ox = oy = margin
    Wt, Ht = (W + 2 * margin) * cell, (H + 2 * margin) * cell
    T = lambda P: (np.asarray(P) + [ox, oy]) * cell
    def poly(P, closed=False):
        P = T(P)
        return "M" + "L".join(f"{x:.2f} {y:.2f}" for x, y in P) + ("Z" if closed else "")

    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{Wt:.2f}pt" '
           f'height="{Ht:.2f}pt" viewBox="0 0 {Wt:.3f} {Ht:.3f}">',
           '<rect width="100%" height="100%" fill="#fff"/>',
           f'<rect x="{ox*cell:.3f}" y="{oy*cell:.3f}" width="{W*cell:.3f}" '
           f'height="{H*cell:.3f}" fill="{void}"/>']

    # extend the solution strand out through the frame
    sol = [s for s in strands if s[2]][0][0]
    ext = []
    if entrance is not None:
        for e, P0 in ((entrance[0], sol[0]), (entrance[1], sol[-1])):
            ext.append(P0 + np.array(e) * 1.2)
    solP = np.vstack([ext[0][None], sol, ext[1][None]]) if ext else sol

    # white tubes
    tubes = []
    for P, closed, is_sol in strands:
        tubes.append(poly(solP if is_sol else P, closed))
    if outline:  # black halo under the tubes; only survives where no tube covers it
        out.append(f'<path fill="none" stroke="#000" stroke-width="{(2*rmax+wall)*cell:.3f}" '
                   f'stroke-linecap="round" stroke-linejoin="round" d="{"".join(tubes)}"/>')
    out.append(f'<path fill="none" stroke="#fff" stroke-width="{2*rmax*cell:.3f}" '
               f'stroke-linecap="round" stroke-linejoin="round" d="{"".join(tubes)}"/>')
    if show_solution:
        out.append(f'<path fill="none" stroke="{fill}" stroke-width="{(2*rmax-wall)*0.9*cell:.3f}" '
                   f'stroke-linecap="round" stroke-linejoin="round" d="{poly(solP)}"/>')

    segs = flow_walls(strands, H, W, rmax, gap_arc)
    d = "".join(f"M{T(a)[0]:.2f} {T(a)[1]:.2f}L{T(b)[0]:.2f} {T(b)[1]:.2f}" for a, b in segs)
    out.append(f'<path fill="none" stroke="#000" stroke-width="{wall*cell:.3f}" '
               f'stroke-linecap="round" d="{d}"/>')
    # frame on top, with a gap for the entrance
    out.append(f'<rect x="{ox*cell:.3f}" y="{oy*cell:.3f}" width="{W*cell:.3f}" '
               f'height="{H*cell:.3f}" fill="none" stroke="#000" stroke-width="{wall*cell:.3f}"/>')
    if ext:
        out.append(f'<path fill="none" stroke="#fff" stroke-width="{(2*rmax-wall)*cell:.3f}" '
                   f'd="{poly(np.vstack([sol[0], ext[0]]))}{poly(np.vstack([sol[-1], ext[1]]))}"/>')
    if not ext:  # entrance/exit inside the picture: mark them
        for q in (sol[0], sol[-1]):
            c = T(q); r = 0.32 * cell
            out.append(f'<path fill="#1a9e3a" d="M{c[0]:.2f} {c[1]-r:.2f}L{c[0]+r:.2f} '
                       f'{c[1]:.2f}L{c[0]:.2f} {c[1]+r:.2f}L{c[0]-r:.2f} {c[1]:.2f}z"/>')
    out.append("</svg>")
    return "\n".join(out)


def densify(P, closed, h=0.2):
    Q = np.vstack([P, P[:1]]) if closed else P
    d = np.concatenate([[0], np.cumsum(np.linalg.norm(np.diff(Q, axis=0), axis=1))])
    n = max(3, int(np.ceil(d[-1] / h)))
    t = np.linspace(0, d[-1], n + (0 if closed else 1), endpoint=not closed)
    return np.stack([np.interp(t, d, Q[:, 0]), np.interp(t, d, Q[:, 1])], 1)


def flow_walls(strands, H, W, rmax=0.55, gap_arc=1.05):
    """Wall segments (lattice coords): Voronoi ridges between samples that
    aren't arc-neighbours on the same strand, clipped to the tubes."""
    strands = [(densify(P, c), c, s_) for P, c, s_ in strands]
    pts, sid, arc, alen, cl = [], [], [], [], []
    for k, (P, closed, _) in enumerate(strands):
        Q = np.vstack([P, P[:1]]) if closed else P
        s = np.concatenate([[0], np.cumsum(np.linalg.norm(np.diff(Q, axis=0), axis=1))])
        pts.append(P); sid += [k] * len(P); arc.append(s[:len(P)])
        alen += [s[-1]] * len(P); cl += [closed] * len(P)
    # a point on a ridge is inside a tube if within rmax of the strand's
    # *polyline*; samples sit up to maxseg/2 off that, so widen the clip
    maxseg = max(np.linalg.norm(np.diff(np.vstack([q, q[:1]]) if c else q, axis=0),
                                axis=1).max() for q, c, _ in strands)
    rclip = np.sqrt(rmax ** 2 + (maxseg / 2) ** 2) + 0.15  # overshoot into void
    P = np.vstack(pts); sid = np.array(sid); arc = np.concatenate(arc)
    alen = np.array(alen); cl = np.array(cl)
    # frame points so outer ridges exist
    fr = []
    for x in np.arange(0, W + .01, .5): fr += [(x, -.5), (x, H + .5)]
    for y in np.arange(0, H + .01, .5): fr += [(-.5, y), (W + .5, y)]
    allP = np.vstack([P, np.array(fr)])
    vor = Voronoi(allP)
    segs = []
    n = len(P)
    for (i, j), rv in zip(vor.ridge_points, vor.ridge_vertices):
        if i >= n or j >= n or -1 in rv:
            continue
        if sid[i] == sid[j]:
            da = abs(arc[i] - arc[j])
            if cl[i]:
                da = min(da, alen[i] - da)
            if da < gap_arc:
                continue
        a, b = vor.vertices[rv[0]], vor.vertices[rv[1]]
        # clip ridge to within rmax of its generators
        pi, pj = P[i], P[j]
        mpt = 0.5 * (pi + pj)
        h2 = rclip ** 2 - np.sum((pi - mpt) ** 2)
        if h2 <= 0:
            continue
        h = np.sqrt(h2)
        u = np.array([-(pj - pi)[1], (pj - pi)[0]])
        u /= np.linalg.norm(u) + 1e-12
        ta, tb = np.dot(a - mpt, u), np.dot(b - mpt, u)
        lo, hi = max(min(ta, tb), -h), min(max(ta, tb), h)
        if hi <= lo:
            continue
        segs.append((mpt + lo * u, mpt + hi * u))
    return segs


def corridor_widths(strands, H, W, rmax=0.55, wall=0.14, gap_arc=1.05):
    """Clear corridor width (wall edge to wall edge, lattice units) at points
    along every strand: distance to the nearest wall on the left plus on the
    right, minus the wall thickness. A side with no wall (void) counts as
    the tube edge."""
    segs = flow_walls(strands, H, W, rmax, gap_arc)
    wp = []
    for a, b in segs:
        n = max(2, int(np.ceil(np.linalg.norm(b - a) / 0.04)) + 1)
        t = np.linspace(0, 1, n)[:, None]
        wp.append(a + t * (b - a))
    wp = np.vstack(wp)
    tree = cKDTree(wp)
    cap = rmax + wall / 2
    out = []
    for P, closed, _ in strands:
        Q = densify(P, closed, 0.1)
        if not closed:
            Q = Q[6:-6]  # skip the open ends, which run through the frame gap
        if len(Q) < 3:
            continue
        tng = np.gradient(Q, axis=0)
        tng /= np.linalg.norm(tng, axis=1, keepdims=True) + 1e-12
        dist, idx = tree.query(Q, k=32, distance_upper_bound=1.5)
        left = np.full(len(Q), cap)
        right = np.full(len(Q), cap)
        for k in range(dist.shape[1]):
            ok = np.isfinite(dist[:, k])
            v = wp[np.minimum(idx[:, k], len(wp) - 1)] - Q
            side = tng[:, 0] * v[:, 1] - tng[:, 1] * v[:, 0]
            dk = np.where(ok, dist[:, k], np.inf)
            left = np.where(side > 0, np.minimum(left, dk), left)
            right = np.where(side <= 0, np.minimum(right, dk), right)
        # the frame is a wall too
        fx = np.minimum(Q[:, 0], W - Q[:, 0])
        fy = np.minimum(Q[:, 1], H - Q[:, 1])
        fr = np.minimum(fx, fy)
        near = fr < cap
        # frame is on whichever side is currently larger
        big_left = left >= right
        left = np.where(near & big_left, np.minimum(left, fr), left)
        right = np.where(near & ~big_left, np.minimum(right, fr), right)
        out.append(left + right - wall)
    return np.concatenate(out)


def verify_regions(svg_text, H, W, cell, n_strands, margin=2.0, px_per_cell=30):
    """Rasterize the (unwrapped) puzzle SVG and count open regions inside
    the frame. A correct maze has exactly one per strand. Returns (ok, count)."""
    import subprocess, tempfile, os
    from PIL import Image
    zoom = px_per_cell / (cell * 96 / 72)  # rsvg renders 1pt as 4/3 px
    with tempfile.TemporaryDirectory() as td:
        svg_path, png = os.path.join(td, "v.svg"), os.path.join(td, "v.png")
        with open(svg_path, "w") as f:
            f.write(svg_text)
        subprocess.run(["rsvg-convert", "-z", f"{zoom:.4f}", "-o", png, svg_path], check=True)
        a = np.asarray(Image.open(png).convert("L")) > 240   # corridors are pure white; voids any grey
    px = cell * zoom * 96 / 72
    x0, pad = margin * px, 4
    inner = a[int(x0) + pad:int(x0 + H * px) - pad, int(x0) + pad:int(x0 + W * px) - pad]
    lab, k = ndimage.label(inner)
    sizes = np.bincount(lab.ravel())[1:]
    k = int((sizes > 20).sum())         # ignore antialiasing specks
    return k == n_strands, k
