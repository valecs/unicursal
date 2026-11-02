

@SETTINGS
@given(pictures())
def test_maze_with_deadend_fix_invariants(pic):
    """Found #7 in seconds before the fix (shrunk to a one-cell picture)."""
    mask, seed = pic
    fg = P.largest_component(mask)
    grid, fine_fg, opening = P.build_maze(fg, random.Random(seed), True)
    assert n_edges(grid) == grid.H * grid.W - 1
    assert (solution(grid, opening, fine_fg) == fine_fg).all()
