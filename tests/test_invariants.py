"""
Structural invariants the generator must always satisfy. These are the
checks a port must keep passing; run with:  python3 -m pytest -q tests
"""
import os
import random
import sys
from pathlib import Path

import numpy as np
import pytest
from PIL import Image, ImageDraw

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
import picmaze as P  # noqa: E402
import flow as F     # noqa: E402


@pytest.fixture(scope="module")
def ring_png(tmp_path_factory):
    """Donut touching the bottom edge: one hole, edge entrance."""
    p = tmp_path_factory.mktemp("img") / "ring.png"
    im = Image.new("L", (300, 300), 255)
    d = ImageDraw.Draw(im)
    d.ellipse([40, 40, 260, 260], fill=0)
    d.ellipse([110, 110, 190, 190], fill=255)
    d.rectangle([130, 250, 170, 300], fill=0)
    im.save(p)
    return str(p)


@pytest.fixture(scope="module")
def two_blobs_png(tmp_path_factory):
    """Two separate pieces: exercises bridging."""
    p = tmp_path_factory.mktemp("img") / "blobs.png"
    im = Image.new("L", (300, 200), 255)
    d = ImageDraw.Draw(im)
    d.ellipse([20, 40, 120, 160], fill=0)
    d.ellipse([180, 40, 280, 160], fill=0)
    im.save(p)
    return str(p)


def degrees(grid):
    return np.array([[len(grid.open_nbrs((r, c))) for c in range(grid.W)]
                     for r in range(grid.H)])


def n_edges(grid):
    return int(grid.hp.sum() + grid.vp.sum())


def solution_mask(grid, opening, fine_fg):
    sol = np.zeros_like(fine_fg)
    for p in P.trace_path(grid, opening[0], fine_fg):
        sol[p] = True
    return sol


@pytest.mark.parametrize("seed", range(5))
def test_labyrinth_has_no_junctions(ring_png, seed):
    fg = P.largest_component(P.load_mask(ring_png, 20))
    grid, fine_fg, opening = P.build_labyrinth(fg, random.Random(seed))
    deg = degrees(grid)
    # every cell is on a corridor with exactly two openings, except the
    # two ends of the solution
    assert (deg == 2).sum() == grid.H * grid.W - 2
    assert deg[opening[0]] == 1 and deg[opening[1]] == 1
    # disjoint cycles + one path  =>  edges == cells - 1
    assert n_edges(grid) == grid.H * grid.W - 1


@pytest.mark.parametrize("seed", range(5))
def test_labyrinth_solution_is_exactly_the_picture(ring_png, seed):
    fg = P.largest_component(P.load_mask(ring_png, 20))
    grid, fine_fg, opening = P.build_labyrinth(fg, random.Random(seed))
    assert (solution_mask(grid, opening, fine_fg) == fine_fg).all()


def test_labyrinth_background_regions_are_single_loops(ring_png):
    fg = P.largest_component(P.load_mask(ring_png, 20))
    grid, fine_fg, opening = P.build_labyrinth(fg, random.Random(1))
    lab, k = P.components(~fine_fg)
    assert k == 2  # outside + the hole
    for t in range(1, k + 1):
        cells = list(zip(*np.nonzero(lab == t)))
        # walk the loop once (trace_path is for open corridors; it would
        # circle a closed one forever)
        start = cells[0]
        prev, cur, n = None, start, 0
        while True:
            nxt = [q for q in grid.open_nbrs(cur) if q != prev and lab[q] == t]
            assert len(grid.open_nbrs(cur)) == 2
            prev, cur, n = cur, nxt[0], n + 1
            if cur == start or n > len(cells):
                break
        assert cur == start and n == len(cells)  # one cycle covers the region


@pytest.mark.parametrize("seed", range(3))
def test_maze_is_spanning_tree_with_picture_solution(ring_png, seed):
    fg = P.largest_component(P.load_mask(ring_png, 20))
    grid, fine_fg, opening = P.build_maze(fg, random.Random(seed))
    assert n_edges(grid) == grid.H * grid.W - 1
    lab, k = P.components(np.ones((grid.H, grid.W), bool))
    # connectivity via BFS over open passages
    seen = {(0, 0)}
    stack = [(0, 0)]
    while stack:
        for q in grid.open_nbrs(stack.pop()):
            if q not in seen:
                seen.add(q)
                stack.append(q)
    assert len(seen) == grid.H * grid.W
    assert (solution_mask(grid, opening, fine_fg) == fine_fg).all()


def test_bridging_joins_pieces(two_blobs_png):
    raw = P.load_mask(two_blobs_png, 30)
    assert P.components(raw)[1] == 2
    fg = P.largest_component(raw, bridge=True)
    assert P.components(fg)[1] == 1
    assert (fg | ~raw).all() and fg.sum() > raw.sum()   # only adds cells


def test_flow_is_sealed_and_wide_enough(ring_png):
    rng = random.Random(4)
    fg = P.largest_component(P.load_mask(ring_png, 14))
    grid, fine_fg, opening = P.build_labyrinth(fg, rng)
    path = P.trace_path(grid, opening[0], fine_fg)
    strands = F.extract_strands(grid, path)
    strands = F.relax(strands, fine_fg, grid.H, grid.W, iters=600,
                      length=0.85, bend=0.3, noise=0.02, seed=4)
    assert F.count_crossings(strands) == 0
    cell = 10.0
    svg = F.render_flow_svg(strands, grid.H, grid.W, cell, margin=P.MARGIN)
    ok, k = F.verify_regions(svg, grid.H, grid.W, cell, len(strands), margin=P.MARGIN)
    assert ok, f"{k} open regions for {len(strands)} strands (a wall leaks)"
    # ink saver: light voids + outlined corridors must be just as sealed
    svg = F.render_flow_svg(strands, grid.H, grid.W, cell, margin=P.MARGIN,
                            void="#cfcfcf", outline=True)
    ok, k = F.verify_regions(svg, grid.H, grid.W, cell, len(strands), margin=P.MARGIN)
    assert ok, f"ink saver: {k} open regions for {len(strands)} strands"
    widths = F.corridor_widths(strands, grid.H, grid.W)
    assert widths.min() > 0.6   # in corridor-spacing units


def test_paper_sizing():
    assert P.parse_paper("letter") == (215.9, 279.4)
    w, h = P.parse_paper("8.5x11in")
    assert abs(w - 215.9) < 1e-6 and abs(h - 279.4) < 1e-6
    # ghost case: the frame fills the printable area of letter at 7 mm
    avail = (215.9 - 2 * 12.7, 279.4 - 2 * 12.7)
    nmax, mmax = P.fit_grid(avail, 7)
    assert (nmax, mmax) == (13, 18)
    assert 2 * nmax * 7 <= avail[0] and 2 * mmax * 7 <= avail[1]
    assert P.cells_to_fit(240, 320, nmax, mmax) == 17


def test_page_is_true_scale():
    svg = ('<svg xmlns="http://www.w3.org/2000/svg" width="100.00pt" '
           'height="50.00pt" viewBox="0 0 100.000 50.000"><rect/></svg>')
    out = P.place_on_page(svg, (215.9, 279.4))
    assert 'width="612.00pt" height="792.00pt"' in out


SHAPES = Path(__file__).resolve().parent.parent / "shapes"


@pytest.mark.parametrize("seed", range(10))
def test_maze_default_solution_covers_picture(seed):
    fg = P.largest_component(P.load_mask(str(SHAPES / "ghost.png"), 15))
    grid, fine_fg, opening = P.build_maze(fg, random.Random(seed))
    assert (solution_mask(grid, opening, fine_fg) == fine_fg).all()


@pytest.mark.xfail(strict=True, reason="dead-end heuristic opens walls into the "
                   "solution path; off by default until fixed")
def test_maze_deadend_fix_known_bug():
    fg = P.largest_component(P.load_mask(str(SHAPES / "ghost.png"), 15))
    for seed in range(10):
        grid, fine_fg, opening = P.build_maze(fg, random.Random(seed), True)
        assert (solution_mask(grid, opening, fine_fg) == fine_fg).all()


PRESETS = Path(__file__).resolve().parent.parent / "presets.toml"


def test_presets_only_set_known_options():
    dests = {a.dest for a in P.build_parser()._actions}
    for name, p in P.load_presets(PRESETS).items():
        assert set(p) - P.PRESET_META <= dests, name


def test_preset_supplies_defaults_and_flags_override():
    a = P.parse_args_with_preset(P.build_parser(), ["x.png", "--preset", "easy"])
    assert a.corridor == P.load_presets(PRESETS)["easy"]["corridor"]
    a = P.parse_args_with_preset(P.build_parser(),
                                 ["x.png", "--preset", "easy", "--corridor", "5"])
    assert a.corridor == 5
