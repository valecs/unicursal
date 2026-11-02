"""Build and check the built-in picture library (#14, library/README.md).

For every subject in library/subjects/:

1. master: rasterize <id>.svg, crop to the subject, make it touch its edge
   with a fixed margin on the other sides, downsample to 2x the Hard grid.
   Written to <id>.png once; --remaster rebuilds it (the PNG is what ships).
2. masks: the master downsampled exactly as the app will (picmaze.load_mask
   at every preset x paper), or the hand-drawn override at that grid.
3. checks: preflight metrics, and the blind judge (Gemini; answers cached in
   judge.json by mask hash, so only new masks cost a call).
4. outputs: Thanksgiving subjects exported to shapes/; library/out/ gets a
   contact sheet, the package and a JSON report (not committed).

  uv run library/build.py                 # build what's missing, judge, report
  uv run library/build.py --remaster pumpkin
  uv run library/build.py --no-judge
"""
import argparse
import base64
import hashlib
import io
import json
import struct
import subprocess
import sys
import time
import tomllib
import urllib.error
import urllib.request
import zlib
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw
from scipy import ndimage

LIB = Path(__file__).resolve().parent
ROOT = LIB.parent
SUBJECTS_DIR = LIB / "subjects"
OUT = LIB / "out"
sys.path.insert(0, str(ROOT))
import picmaze as P  # noqa: E402

PAPERS = ("letter", "a4")
PRESETS = {k: v["corridor"] for k, v in P.load_presets(ROOT / "presets.toml").items()}
MARGIN = 0.08      # background margin on the three free sides, x the long side
RASTER_PX = 1600   # long side of the SVG rasterization

JUDGE_MODEL = "gemini-3.1-flash-lite"
JUDGE_PROMPT = ("This blocky black-on-white image is a silhouette of a single "
                "thing, animal or symbol, as it might appear in a children's "
                "puzzle. What is it? Answer with one or two words only.")
JUDGE_VERSION = 1  # bump when the model or prompt changes: old verdicts no longer count
JUDGE_API = "https://generativelanguage.googleapis.com/v1beta/models/{m}:generateContent"


# ------------------------------------------------------------ files ------

def subject_ids():
    return sorted(p.stem for p in SUBJECTS_DIR.glob("*.toml"))


def load_subject(sid):
    with open(SUBJECTS_DIR / f"{sid}.toml", "rb") as f:
        s = tomllib.load(f)
    s.setdefault("edge", "S")
    s.setdefault("tags", [])
    s["id"] = sid
    return s


def load_tags():
    with open(LIB / "tags.toml", "rb") as f:
        return set(tomllib.load(f)["tags"])


def parse_art(text):
    """ASCII pixel art ('#' picture, '.' background) -> bool array."""
    rows = [r.strip() for r in text.splitlines() if r.strip()]
    if not rows or len({len(r) for r in rows}) != 1:
        raise ValueError("pixel art rows must all be the same length")
    if set("".join(rows)) - set("#."):
        raise ValueError("pixel art uses only '#' and '.'")
    return np.array([[c == "#" for c in r] for r in rows])


def format_art(mask):
    return "\n".join("".join("#" if v else "." for v in row) for row in mask) + "\n"


def load_overrides(sid):
    """{key: art} for <id>.<preset>.txt (key "medium") and paper-specific
    <id>.<preset>.<paper>.txt (key "medium.a4"), which wins on that paper."""
    out = {}
    for preset in PRESETS:
        for key in [preset] + [f"{preset}.{paper}" for paper in PAPERS]:
            p = SUBJECTS_DIR / f"{sid}.{key}.txt"
            if p.exists():
                out[key] = parse_art(p.read_text())
    return out


def override_for(over, preset, paper, shape):
    """The override used at preset/paper for a grid of `shape`, or None."""
    for key in (f"{preset}.{paper}", preset):
        if key in over and over[key].shape == shape:
            return over[key]
    return None


def mask_png(mask):
    b = io.BytesIO()
    Image.fromarray(np.where(mask, 0, 255).astype(np.uint8)).save(b, "PNG")
    return b.getvalue()


def png_mask(data):
    return np.asarray(Image.open(io.BytesIO(data)).convert("L")) < 128


def load_master(sid):
    return png_mask((SUBJECTS_DIR / f"{sid}.png").read_bytes())


# ---------------------------------------------------------- packing ------

def pack(mask):
    """1 bit per cell, row-major, raw deflate, behind a rows/cols header."""
    h, w = mask.shape
    bits = np.packbits(np.asarray(mask, bool).ravel()).tobytes()
    c = zlib.compressobj(9, zlib.DEFLATED, -15)
    return struct.pack(">HH", h, w) + c.compress(bits) + c.flush()


def unpack(data):
    h, w = struct.unpack(">HH", data[:4])
    bits = np.frombuffer(zlib.decompress(data[4:], -15), np.uint8)
    return np.unpackbits(bits)[:h * w].astype(bool).reshape(h, w)


def mask_hash(mask):
    return hashlib.sha256(pack(mask)).hexdigest()[:20]


# ----------------------------------------------------------- sizing ------

def cells_shape(w, h, cells):
    """(cols, rows) picmaze.load_mask gives an w x h image at `cells`."""
    if w >= h:
        return cells, max(1, round(cells * h / w))
    return max(1, round(cells * w / h)), cells


def master_shape(w, h):
    """(rows, cols) of the master for a w x h picture: 2x the largest Hard
    grid over the supported papers."""
    cells = max(P.page_grid(w, h, P.parse_paper(p), PRESETS["hard"])[2] for p in PAPERS)
    cols, rows = cells_shape(w, h, cells)
    return 2 * rows, 2 * cols


def target_shape(sid, preset, paper):
    m = load_master(sid)
    cells = P.page_grid(m.shape[1], m.shape[0], P.parse_paper(paper), PRESETS[preset])[2]
    cols, rows = cells_shape(m.shape[1], m.shape[0], cells)
    return rows, cols


# ----------------------------------------------------------- master ------

def rasterize(svg_path, px=RASTER_PX):
    """SVG -> bool array (True = picture), long side `px`."""
    png = subprocess.run(["rsvg-convert", "-w", str(px), "-h", str(px),
                          "--keep-aspect-ratio", "-b", "white", str(svg_path)],
                         check=True, capture_output=True).stdout
    return png_mask(png)


def _crop(a):
    ys, xs = np.nonzero(a)
    return a[ys.min():ys.max() + 1, xs.min():xs.max() + 1]


def _pad(a, edge, m):
    top, bottom, left, right = (0 if edge == e else m for e in "NSWE")
    return np.pad(a, ((top, bottom), (left, right)))


def normalise(a, edge):
    """Crop to the subject; margin on every side except `edge`."""
    c = _crop(a)
    return _pad(c, edge, round(MARGIN * max(c.shape)))


def _snap_to_edge(a, edge):
    """Shift the subject so it touches `edge` (thresholding can leave a thin
    stalk short of it)."""
    ys, xs = np.nonzero(a)
    shift = {"S": (a.shape[0] - 1 - ys.max(), 0), "N": (-ys.min(), 0),
             "E": (0, a.shape[1] - 1 - xs.max()), "W": (0, -xs.min())}[edge]
    return np.roll(a, shift, axis=(0, 1))


def build_master(sid):
    s = load_subject(sid)
    c = _crop(rasterize(SUBJECTS_DIR / f"{sid}.svg"))
    approx = normalise(c, s["edge"])
    rows, cols = master_shape(approx.shape[1], approx.shape[0])
    for _ in range(4):                       # settle rounding to a fixed point
        nxt = master_shape(cols, rows)
        if nxt == (rows, cols):
            break
        rows, cols = nxt
    m = round(MARGIN * max(rows, cols) / (1 + MARGIN))
    ir = rows - (m if s["edge"] in "NS" else 2 * m)
    ic = cols - (m if s["edge"] in "WE" else 2 * m)
    im = Image.fromarray(np.where(c, 0, 255).astype(np.uint8)).resize((ic, ir), Image.LANCZOS)
    inner = _snap_to_edge(np.asarray(im) < 128, s["edge"])
    master = _pad(inner, s["edge"], m)
    assert master.shape == (rows, cols) == master_shape(cols, rows), sid
    (SUBJECTS_DIR / f"{sid}.png").write_bytes(mask_png(master))
    return master


# ------------------------------------------------------------ masks ------

def masks(sid):
    """{(preset, paper): mask} as the puzzle will use it (pieces bridged)."""
    master = load_master(sid)
    png = mask_png(master)
    over = load_overrides(sid)
    out = {}
    for preset, corridor in PRESETS.items():
        for paper in PAPERS:
            cells = P.page_grid(master.shape[1], master.shape[0],
                                P.parse_paper(paper), corridor)[2]
            raw = P.load_mask(io.BytesIO(png), cells)
            art = override_for(over, preset, paper, raw.shape)
            out[(preset, paper)] = _quiet_bridge(raw if art is None else art)
    return out


def _quiet_bridge(raw):
    if P.components(raw)[1] <= 1:
        return raw
    return P.bridge_components(raw)[0]


def holes(mask):
    lab, n = ndimage.label(~mask)
    border = set(np.unique(np.r_[lab[0], lab[-1], lab[:, 0], lab[:, -1]]))
    return [int((lab == i).sum()) for i in range(1, n + 1) if i not in border]


def metrics(raw, master):
    fg = _quiet_bridge(raw)
    opened = ndimage.binary_opening(fg, np.ones((2, 2), bool))
    return {"grid": f"{raw.shape[1]}x{raw.shape[0]}",
            "pieces": int(P.components(raw)[1]),
            "holes": len(holes(fg)), "master_holes": len(holes(master)),
            "thin": round(float((fg & ~opened).sum() / max(fg.sum(), 1)), 3),
            "cover": round(float(fg.mean()), 3)}


# ------------------------------------------------------------ judge ------

def load_judge_cache():
    p = LIB / "judge.json"
    if not p.exists():
        return {}
    data = json.loads(p.read_text())
    return {h: v for h, v in data.items() if v.get("version") == JUDGE_VERSION}


def save_judge_cache(cache, keep=None):
    """Merge new verdicts in; with `keep` (hashes), drop all others."""
    p = LIB / "judge.json"
    old = json.loads(p.read_text()) if p.exists() else {}
    old.update(cache)
    if keep is not None:
        old = {h: v for h, v in old.items() if h in keep}
    p.write_text(json.dumps(dict(sorted(old.items())), indent=1) + "\n")


def judge_ask(mask):
    im = Image.fromarray(np.where(mask, 0, 255).astype(np.uint8))
    s = max(1, 384 // max(mask.shape))
    b = io.BytesIO()
    im.resize((mask.shape[1] * s, mask.shape[0] * s), Image.NEAREST).save(b, "PNG")
    body = {"contents": [{"parts": [
        {"text": JUDGE_PROMPT},
        {"inlineData": {"mimeType": "image/png",
                        "data": base64.b64encode(b.getvalue()).decode()}}]}],
        "generationConfig": {"temperature": 0}}
    req = urllib.request.Request(JUDGE_API.format(m=JUDGE_MODEL),
                                 data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    for attempt in range(5):
        try:
            r = json.load(urllib.request.urlopen(req, timeout=120))
            break
        except (urllib.error.URLError, ConnectionError, TimeoutError):
            if attempt == 4:
                raise
            time.sleep(2 ** attempt)
    return r["candidates"][0]["content"]["parts"][0]["text"].strip().lower()


def passes(sid, answer):
    return any(k in answer for k in load_subject(sid)["keywords"])


def offered(sid, cache=None):
    """Presets at which the subject reads on every paper (judge pass)."""
    cache = load_judge_cache() if cache is None else cache
    ok = {}
    for (preset, _paper), fg in masks(sid).items():
        v = cache.get(mask_hash(fg))
        ok[preset] = ok.get(preset, True) and bool(v) and passes(sid, v["answer"])
    return {p for p, good in ok.items() if good}


# ----------------------------------------------------------- report ------

def contact_sheet(rows, path, cell=12):
    """One row per subject: master, then each preset's Letter mask, labelled."""
    tiles = []
    for sid, ms, cache in rows:
        imgs = [(load_master(sid), "master")]
        for preset in PRESETS:
            fg = ms[(preset, "letter")]
            v = cache.get(mask_hash(fg), {})
            mark = "" if not v else ("ok " if passes(sid, v["answer"]) else "X ")
            imgs.append((fg, f"{preset}: {mark}{v.get('answer', '?')}"))
        tiles.append((sid, imgs))
    H = 240
    W = 4 * 220
    sheet = Image.new("RGB", (W, (H + 30) * len(tiles)), "white")
    d = ImageDraw.Draw(sheet)
    for r, (sid, imgs) in enumerate(tiles):
        y = r * (H + 30)
        d.text((4, y + 2), sid, fill="black")
        for c, (m, label) in enumerate(imgs):
            s = max(1, min(200 // m.shape[1], H // m.shape[0]))
            im = Image.fromarray(np.where(m, 0, 255).astype(np.uint8)).resize(
                (m.shape[1] * s, m.shape[0] * s), Image.NEAREST)
            x = c * 220 + 10
            sheet.paste(im.convert("RGB"), (x, y + 16))
            d.rectangle([x - 1, y + 15, x + im.width, y + 16 + im.height], outline="#bbb")
            d.text((x, y + 18 + im.height), label, fill="black")
    sheet.save(path)


def export_shapes(sid):
    """Thanksgiving subjects ship as prototype inputs too (owner, #14)."""
    shapes = ROOT / "shapes"
    (shapes / f"{sid}.png").write_bytes((SUBJECTS_DIR / f"{sid}.png").read_bytes())
    over = load_overrides(sid)
    for key, art in over.items():
        (shapes / f"{sid}.{key}.png").write_bytes(mask_png(art))
    for p in shapes.glob(f"{sid}.*.png"):          # overrides since removed
        if p.name[len(sid) + 1:-4] not in over:
            p.unlink()


def package(ids):
    """The library file (format provisional; settled by the pilot)."""
    entries = []
    for sid in ids:
        s = load_subject(sid)
        e = {"id": sid, "name": s["name"], "tags": s["tags"],
             "master": base64.b64encode(pack(load_master(sid))).decode()}
        over = load_overrides(sid)
        if over:
            e["overrides"] = {k: base64.b64encode(pack(v)).decode() for k, v in over.items()}
        entries.append(e)
    return {"version": 0, "pictures": entries}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("ids", nargs="*", help="subjects to build (default: all)")
    ap.add_argument("--remaster", action="store_true",
                    help="rebuild the master PNG from the SVG even if it exists")
    ap.add_argument("--no-judge", action="store_true", help="don't call the judge")
    args = ap.parse_args(argv)
    ids = args.ids or subject_ids()
    OUT.mkdir(exist_ok=True)
    cache = load_judge_cache()
    new = {}
    rows, report = [], {}
    for sid in ids:
        s = load_subject(sid)
        if args.remaster or not (SUBJECTS_DIR / f"{sid}.png").exists():
            build_master(sid)
            print(f"{sid}: built master {load_master(sid).shape[1]}x{load_master(sid).shape[0]}")
        master = load_master(sid)
        if "thanksgiving" in s["tags"]:
            export_shapes(sid)
        ms = masks(sid)
        png = mask_png(master)
        rep = {}
        for (preset, paper), fg in ms.items():
            cells = P.page_grid(master.shape[1], master.shape[0],
                                P.parse_paper(paper), PRESETS[preset])[2]
            raw = P.load_mask(io.BytesIO(png), cells)
            art = override_for(load_overrides(sid), preset, paper, raw.shape)
            m = metrics(raw if art is None else art, master)
            m["override"] = art is not None
            h = mask_hash(fg)
            if h not in cache and not args.no_judge:
                cache[h] = new[h] = {"answer": judge_ask(fg), "model": JUDGE_MODEL,
                                     "version": JUDGE_VERSION}
            v = cache.get(h)
            m["judge"] = v["answer"] if v else None
            m["pass"] = bool(v) and passes(sid, v["answer"])
            rep[f"{preset}/{paper}"] = m
        report[sid] = {"offered": sorted(offered(sid, cache)), "masks": rep,
                       "bytes": len(pack(master)) + sum(len(pack(a)) for a in
                                                        load_overrides(sid).values())}
        rows.append((sid, ms, cache))
        print(f"{sid}: offered {', '.join(report[sid]['offered']) or 'nowhere'}; "
              f"{report[sid]['bytes']} B")
        for k, m in rep.items():
            print(f"  {k:13} {m['grid']:>6} pieces {m['pieces']} holes {m['holes']}/"
                  f"{m['master_holes']} thin {m['thin']:.2f} cover {m['cover']:.2f}"
                  f"{' override' if m['override'] else ''}  judge: {m['judge']}"
                  f" {'ok' if m['pass'] else 'FAIL'}")
    used = {mask_hash(fg) for _sid, ms, _c in rows for fg in ms.values()}
    if new or not args.ids:
        save_judge_cache(new, keep=used if not args.ids else None)
    contact_sheet(rows, OUT / "sheet.png")
    lib = package(ids)
    raw = json.dumps(lib, separators=(",", ":")).encode()
    (OUT / "library.json").write_bytes(raw)
    (OUT / "report.json").write_text(json.dumps(report, indent=1) + "\n")
    print(f"package: {len(raw)} B for {len(ids)} pictures "
          f"({len(zlib.compress(raw, 9))} B gzipped); sheet: {OUT / 'sheet.png'}")


if __name__ == "__main__":
    main()
