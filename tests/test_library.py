"""
The built-in picture library (library/, #14). Two kinds of test:

- properties of the build tools (packing, sizing, override files), over
  arbitrary inputs;
- checks every committed subject must pass, so a PR that adds or edits a
  subject without rebuilding it, or breaks one, goes red.

No network: judge verdicts come from the committed cache (library/judge.json).
"""
import importlib.util
import io
import os
import random
import re
import sys
from pathlib import Path

import numpy as np
import pytest
from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st
from hypothesis.extra.numpy import arrays
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
import picmaze as P  # noqa: E402

_spec = importlib.util.spec_from_file_location("library_build", ROOT / "library" / "build.py")
L = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(L)

SETTINGS = settings(max_examples=150, deadline=None,
                    suppress_health_check=[HealthCheck.too_slow])
SUBJECTS = L.subject_ids()


# ------------------------------------------------------------ tools ------

@SETTINGS
@given(arrays(bool, st.tuples(st.integers(1, 120), st.integers(1, 120))))
def test_pack_round_trips_every_mask(mask):
    assert (L.unpack(L.pack(mask)) == mask).all()
    assert L.unpack(L.pack(mask)).shape == mask.shape


@SETTINGS
@given(arrays(bool, st.tuples(st.integers(1, 40), st.integers(1, 40))))
def test_art_round_trips_every_mask(mask):
    assert (L.parse_art(L.format_art(mask)) == mask).all()


def test_parse_art_rejects_ragged_rows():
    with pytest.raises(ValueError):
        L.parse_art("##.\n#.\n")


@SETTINGS
@given(w=st.integers(1, 4000), h=st.integers(1, 4000),
       paper=st.sampled_from(["letter", "a4"]),
       corridor=st.floats(2.0, 15.0))
def test_page_grid_is_the_largest_grid_that_fits(w, h, paper, corridor):
    """page_grid (what picmaze.py's main uses) gives the picture-cell grid
    whose frame fits the printable area, and no larger one fits."""
    pap, avail, cells = P.page_grid(w, h, P.parse_paper(paper), corridor)
    assert (pap[0] > pap[1]) == (w > h)        # auto orientation follows the image
    nmax, mmax = P.fit_grid(avail, corridor)
    assert cells == P.cells_to_fit(w, h, nmax, mmax)
    cols, rows = L.cells_shape(w, h, cells)
    if cells:
        assert cols <= nmax and rows <= mmax
        assert 2 * cols * corridor <= avail[0] + 1e-9
        assert 2 * rows * corridor <= avail[1] + 1e-9
    c2, r2 = L.cells_shape(w, h, cells + 1)
    assert c2 > nmax or r2 > mmax


@SETTINGS
@given(arrays(bool, st.tuples(st.integers(3, 30), st.integers(3, 30))))
def test_image_at_one_pixel_per_cell_loads_unchanged(mask):
    """Overrides export to shapes/ at 1 px per cell; picmaze must read them
    back cell for cell when the grid matches."""
    png = L.mask_png(mask)
    w, h = mask.shape[1], mask.shape[0]
    back = P.load_mask(io.BytesIO(png), max(w, h))
    if back.shape == mask.shape:
        assert (back == mask).all()


def test_master_normalisation_touches_edge_with_margin():
    """A blob anywhere on the canvas comes out touching the chosen edge,
    with clear margin on the other three sides."""
    a = np.zeros((300, 200), bool)
    a[40:120, 60:150] = True
    for edge in "NSEW":
        m = L.normalise(a, edge)
        sides = {"N": m[0], "S": m[-1], "W": m[:, 0], "E": m[:, -1]}
        assert sides[edge].any()
        assert not any(v.any() for k, v in sides.items() if k != edge)


# ---------------------------------------------------------- subjects -----

def test_there_are_subjects():
    assert SUBJECTS, "library/subjects/ has no subjects"


def test_subject_ids_are_unique_slugs():
    assert len(set(SUBJECTS)) == len(SUBJECTS)
    for sid in SUBJECTS:
        assert re.fullmatch(r"[a-z0-9]+(-[a-z0-9]+)*", sid), sid


def test_no_two_subjects_share_a_primary_keyword():
    """Duplicate lint: two subjects whose main name (first keyword) is the
    same are probably one subject in two contexts, which belong in tags.
    Broader keywords ("hat") may be shared: any hat answer is a pass."""
    seen = {}
    for sid in SUBJECTS:
        k = L.load_subject(sid)["keywords"][0]
        assert k not in seen, f"{sid} and {seen[k]} are both {k!r}"
        seen[k] = sid


@pytest.mark.parametrize("sid", SUBJECTS)
def test_subject_metadata_is_valid(sid):
    s = L.load_subject(sid)
    vocab = L.load_tags()
    assert s["name"] and s["keywords"]
    assert all(k == k.lower() for k in s["keywords"])
    assert s["edge"] in "NSEW" and len(s["edge"]) == 1
    unknown = set(s["tags"]) - vocab
    assert not unknown, f"tags not in tags.toml: {sorted(unknown)}"
    assert (L.SUBJECTS_DIR / f"{sid}.svg").exists()


@pytest.mark.parametrize("sid", SUBJECTS)
def test_master_is_built_at_twice_hard(sid):
    """The committed master is 1-bit, touches its edge, has margin on the
    other sides, and is 2x the Hard grid for its aspect (so it is rebuilt
    when sizing changes)."""
    s = L.load_subject(sid)
    m = L.load_master(sid)
    sides = {"N": m[0], "S": m[-1], "W": m[:, 0], "E": m[:, -1]}
    assert sides[s["edge"]].any()
    assert not any(v.any() for k, v in sides.items() if k != s["edge"])
    assert m.shape == L.master_shape(m.shape[1], m.shape[0])


@pytest.mark.parametrize("sid", SUBJECTS)
def test_overrides_match_a_current_grid(sid):
    """An override drawn for a grid the presets no longer produce is stale
    (e.g. after #8 changes a corridor width)."""
    for key, art in L.load_overrides(sid).items():
        preset, _, paper = key.partition(".")
        grids = {L.target_shape(sid, preset, p) for p in ([paper] if paper else L.PAPERS)}
        assert art.shape in grids, (f"{sid}.{key}.txt is {art.shape}, "
                                    f"grids are {sorted(grids)}")


def test_paper_specific_override_wins(tmp_path, monkeypatch):
    """<id>.<preset>.<paper>.txt is used on that paper; <id>.<preset>.txt
    elsewhere, when its shape matches the grid."""
    sid = SUBJECTS[0]
    monkeypatch.setattr(L, "SUBJECTS_DIR", tmp_path)
    for ext in ("toml", "svg", "png"):
        (tmp_path / f"{sid}.{ext}").write_bytes((ROOT / "library" / "subjects" /
                                                 f"{sid}.{ext}").read_bytes())
    h, w = L.target_shape(sid, "medium", "letter")
    generic = np.zeros((h, w), bool)
    generic[-1, :] = True
    special = generic.copy()
    special[0, 0] = True
    (tmp_path / f"{sid}.medium.txt").write_text(L.format_art(generic))
    (tmp_path / f"{sid}.medium.letter.txt").write_text(L.format_art(special))
    ms = L.masks(sid)
    assert (ms[("medium", "letter")] == L._quiet_bridge(special)).all()
    if L.target_shape(sid, "medium", "a4") == (h, w):
        assert (ms[("medium", "a4")] == generic).all()


@pytest.mark.parametrize("sid", SUBJECTS)
def test_every_mask_makes_a_valid_labyrinth(sid):
    for (preset, paper), fg in L.masks(sid).items():
        fg = P.largest_component(fg)
        grid, fine_fg, opening = P.build_labyrinth(fg, random.Random(0))
        deg = np.array([[len(grid.open_nbrs((r, c))) for c in range(grid.W)]
                        for r in range(grid.H)])
        assert (deg == 2).sum() == grid.H * grid.W - 2, (preset, paper)
        sol = np.zeros_like(fine_fg)
        for p in P.trace_path(grid, opening[0], fine_fg):
            sol[p] = True
        assert (sol == fine_fg).all(), (preset, paper)
        assert opening[2], f"{sid} {preset}/{paper}: entrance is not on the frame"


@pytest.mark.parametrize("sid", SUBJECTS)
def test_every_mask_has_a_judge_verdict(sid):
    """The build was run after the last change: every mask the app can
    produce at a preset has a cached blind-judge answer."""
    cache = L.load_judge_cache()
    for key, fg in L.masks(sid).items():
        assert L.mask_hash(fg) in cache, f"{sid} {key}: run library/build.py"


@pytest.mark.parametrize("sid", SUBJECTS)
def test_every_subject_reads_at_hard(sid):
    """Hard is solved for every sourcing method (DECISIONS findings); a
    subject that fails there is not worth shipping."""
    assert "hard" in L.offered(sid)


@pytest.mark.parametrize("sid", [s for s in SUBJECTS
                                 if "thanksgiving" in L.load_subject(s)["tags"]])
def test_thanksgiving_subjects_are_in_shapes(sid):
    """Owner: Thanksgiving pictures also ship as prototype inputs."""
    def pixels(p):
        return np.asarray(Image.open(p).convert("L")) < 128

    master = L.load_master(sid)
    got = pixels(ROOT / "shapes" / f"{sid}.png")
    assert got.shape == master.shape and (got == master).all()
    for key, art in L.load_overrides(sid).items():
        p = ROOT / "shapes" / f"{sid}.{key}.png"
        got = pixels(p)
        assert got.shape == art.shape and (got == art).all(), p
    extra = {p.name for p in (ROOT / "shapes").glob(f"{sid}.*.png")} - {
        f"{sid}.{k}.png" for k in L.load_overrides(sid)}
    assert not extra, f"stale override exports in shapes/: {sorted(extra)}"


def _review_for(sid, preset, verdict, hashes=None):
    ms = L.masks(sid)
    hs = hashes if hashes is not None else sorted(
        {L.mask_hash(fg) for (p, _), fg in ms.items() if p == preset})
    return {f"{sid}--{preset}": {"verdict": verdict, "hashes": hs}}


@pytest.mark.parametrize("sid", SUBJECTS)
@pytest.mark.parametrize("preset", list(L.PRESETS))
def test_owner_verdict_decides_for_the_masks_reviewed(sid, preset):
    """The owner's review overrides the judge, for exactly the masks they
    looked at; a verdict on masks since changed falls back to the judge."""
    cache = L.load_judge_cache()
    judge_only = preset in L.offered(sid, cache, review={})
    assert preset in L.offered(sid, cache, review=_review_for(sid, preset, "yes"))
    assert preset not in L.offered(sid, cache, review=_review_for(sid, preset, "no"))
    stale = _review_for(sid, preset, "no" if judge_only else "yes", hashes=["0" * 20])
    assert (preset in L.offered(sid, cache, review=stale)) == judge_only


def test_review_page_carries_every_subject_and_mask(tmp_path):
    """The review page embeds each subject's masks and the hashes a verdict
    is tied to, and nothing in the data can close the script tag."""
    out = tmp_path / "review.html"
    L.write_review(SUBJECTS, L.load_judge_cache(), "test batch", out)
    page = out.read_text()
    assert "/*DATA*/" not in page
    start = page.index("const DATA = ") + len("const DATA = ")
    data = __import__("json").JSONDecoder().raw_decode(page[start:])[0]
    assert [s["id"] for s in data["subjects"]] == SUBJECTS
    for s in data["subjects"]:
        ms = L.masks(s["id"])
        for preset, d in s["presets"].items():
            want = sorted({L.mask_hash(fg) for (p, _), fg in ms.items() if p == preset})
            assert d["hashes"] == want
    script = page[page.index("<script>"):]
    assert script.count("</script>") == 1
