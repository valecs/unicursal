"""
Property-based versions of the core invariants: instead of a few hand-drawn
pictures, Hypothesis generates arbitrary small masks and seeds and shrinks any
failure to a minimal example. Same invariants as test_invariants.py.
"""
import os
import random
import sys

import numpy as np
from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st
from hypothesis.extra.numpy import arrays

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
import picmaze as P  # noqa: E402

SETTINGS = settings(max_examples=150, deadline=None,
                    suppress_health_check=[HealthCheck.too_slow])


@st.composite
def pictures(draw, max_side=9):
    """A picture-cell mask with at least one foreground cell (any shape,
    possibly in several pieces), plus a seed."""
    h = draw(st.integers(1, max_side))
    w = draw(st.integers(1, max_side))
    mask = draw(arrays(bool, (h, w)))
    if not mask.any():
        mask[draw(st.integers(0, h - 1)), draw(st.integers(0, w - 1))] = True
    return mask, draw(st.integers(0, 2**32 - 1))


def degrees(grid):
    return np.array([[len(grid.open_nbrs((r, c))) for c in range(grid.W)]
                     for r in range(grid.H)])


def n_edges(grid):
    return int(grid.hp.sum() + grid.vp.sum())


def solution(grid, opening, fine_fg):
    sol = np.zeros_like(fine_fg)
    for p in P.trace_path(grid, opening[0], fine_fg):
        sol[p] = True
    return sol


@SETTINGS
@given(pictures())
def test_bridging_yields_one_piece_containing_the_original(pic):
    mask, _ = pic
    fg = P.largest_component(mask)
    assert P.components(fg)[1] == 1
    assert (fg | ~mask).all()            # bridging only adds cells


@SETTINGS
@given(pictures())
def test_labyrinth_invariants(pic):
    mask, seed = pic
    fg = P.largest_component(mask)
    grid, fine_fg, opening = P.build_labyrinth(fg, random.Random(seed))
    deg = degrees(grid)
    assert (deg == 2).sum() == grid.H * grid.W - 2
    assert deg[opening[0]] == 1 and deg[opening[1]] == 1
    assert n_edges(grid) == grid.H * grid.W - 1
    assert (solution(grid, opening, fine_fg) == fine_fg).all()


@SETTINGS
@given(pictures())
def test_maze_invariants(pic):
    """Default maze (dead-end heuristic on). This property found #7 in seconds
    before the fix, shrunk to a one-cell picture."""
    mask, seed = pic
    fg = P.largest_component(mask)
    grid, fine_fg, opening = P.build_maze(fg, random.Random(seed))
    assert n_edges(grid) == grid.H * grid.W - 1     # spanning tree
    assert (solution(grid, opening, fine_fg) == fine_fg).all()


@SETTINGS
@given(pictures())
def test_maze_without_deadend_fix_invariants(pic):
    mask, seed = pic
    fg = P.largest_component(mask)
    grid, fine_fg, opening = P.build_maze(fg, random.Random(seed), False)
    assert n_edges(grid) == grid.H * grid.W - 1
    assert (solution(grid, opening, fine_fg) == fine_fg).all()
