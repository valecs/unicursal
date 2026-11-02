"""Silhouette bake-off: one-stage vs two-stage AI generation.

Stage 1: text -> silhouette image.
Stage 2: we show the model its own stage-1 image *and* what our pipeline turns
it into at the target grid (auto-crop + downsample + threshold), and ask it to
redraw so the result survives that transform.

Every candidate is auto-cropped, masked at each preset's grid, and measured.
Auth: a proxy-injected x-goog-api-key header (no key is read here).

Usage: python3 silhouette_bakeoff.py OUTDIR [--n 3] [--stage2 none|preview|text]
       [--target medium] [--model M] [--size 512]
"""
import argparse
import base64
import io
import json
import os
import sys
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor

import numpy as np
from PIL import Image, ImageDraw, ImageOps
from scipy import ndimage

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
import picmaze as P  # noqa: E402

API = "https://generativelanguage.googleapis.com/v1beta/models/{m}:generateContent"

SUBJECTS = ["a cat sitting, seen from the side", "a jack-o-lantern pumpkin",
            "an owl", "a rocket ship", "a fish", "a house", "an elephant",
            "a butterfly",
            # round 2
            "a rabbit", "a sailboat", "a teapot", "a turtle"]

STAGE1 = (
    "A simple, bold icon-style silhouette of {s}: one solid black shape on a "
    "plain pure-white background, like a road sign or a stencil. Pure black and "
    "pure white only: no grey, shading, outlines, texture, ground, shadow or "
    "text. One connected piece. At most two or three large white cut-outs "
    "inside the shape for key features (such as eyes). No thin lines, no fine "
    "detail: the picture will be shrunk to about {w} by {h} squares, so every "
    "part must still read at that size. The shape fills most of the image.")

STAGE2 = (
    "The first image is your silhouette of {s}. The second image shows exactly "
    "what survives when it is cropped and shrunk to a {w} by {h} grid of "
    "squares (black = picture). Redraw the silhouette so that, at that grid "
    "size, it is still clearly recognisable as {s}: exaggerate the parts that "
    "identify it, make limbs and gaps at least two squares wide, make any white "
    "cut-outs at least two squares across or remove them, and drop anything "
    "that vanished. Keep it one solid black connected shape on pure white, "
    "pure black and white only, filling most of the image.")

STAGE2_TEXT = (
    "This is your silhouette of {s}. We shrink it to a grid of {w} by {h} "
    "squares and keep only the black. At that size: {problems} Redraw it, "
    "keeping the same design, so it survives that shrinking: make every part "
    "and every gap at least two squares wide (about {px}% of the picture's "
    "width), make white cut-outs at least two squares across or remove them, "
    "and keep it one connected solid black shape on pure white. Pure black and "
    "white only, filling most of the image.")


def critique(m):
    out = []
    if m["pieces"] > 1:
        out.append(f"it falls apart into {m['pieces']} separate pieces.")
    if m["tiny_holes"]:
        out.append(f"{m['tiny_holes']} white cut-out(s) are only one square and will vanish.")
    if m["thin"] > 0.05:
        out.append(f"{m['thin']:.0%} of the shape is only one square thick and will smear.")
    return " ".join(out) or "it mostly survives, but details are at risk."


def call(model, parts, aspect="3:4", size=None, usage=None):
    cfg = {"aspectRatio": aspect}
    if size:
        cfg["imageSize"] = size
    body = {"contents": [{"parts": parts}],
            "generationConfig": {"responseModalities": ["IMAGE"], "imageConfig": cfg}}
    req = urllib.request.Request(API.format(m=model), data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    for attempt in range(4):
        try:
            r = json.load(urllib.request.urlopen(req, timeout=240))
            break
        except (urllib.error.URLError, ConnectionError, TimeoutError):
            if attempt == 3:
                raise
            time.sleep(2 ** attempt)
    if usage is not None:
        usage.append(r.get("usageMetadata", {}))
    for c in r.get("candidates", []):
        for p in c.get("content", {}).get("parts", []):
            if "inlineData" in p:
                return Image.open(io.BytesIO(base64.b64decode(p["inlineData"]["data"])))
    raise RuntimeError("no image: " + json.dumps(r)[:300])


def png_part(im):
    b = io.BytesIO()
    im.save(b, "PNG")
    return {"inlineData": {"mimeType": "image/png",
                           "data": base64.b64encode(b.getvalue()).decode()}}


def autocrop(im, pad=0.08, speck=0.005):
    """Threshold, drop specks, crop to the subject, add a fixed white margin."""
    a = np.asarray(im.convert("L")) < 128
    lab, n = ndimage.label(a)
    if n == 0:
        return im.convert("L")
    sz = ndimage.sum(a, lab, range(1, n + 1))
    keep = np.isin(lab, 1 + np.where(sz > speck * a.sum())[0])
    ys, xs = np.where(keep)
    bw = Image.fromarray(np.where(keep, 0, 255).astype(np.uint8))
    bw = bw.crop((xs.min(), ys.min(), xs.max() + 1, ys.max() + 1))
    return ImageOps.expand(bw, int(pad * max(bw.size)), 255)


def grid_for(im, corridor, paper="letter", margin_mm=12.7):
    pw, ph = P.parse_paper(paper)
    w, h = im.size
    if (w > h) != (pw > ph):
        pw, ph = ph, pw
    nmax = int((pw - 2 * margin_mm) / corridor - 2 * P.MARGIN) // 2
    mmax = int((ph - 2 * margin_mm) / corridor - 2 * P.MARGIN) // 2
    return P.cells_to_fit(w, h, nmax, mmax)


def mask_at(im, cells, path):
    im.save(path)
    return P.load_mask(path, cells)


def metrics(raw):
    """Automatic preflight-style measures on a coarse mask (True = picture)."""
    _, pieces = P.components(raw)
    fg = P.largest_component(raw, bridge=True)
    lab, n = ndimage.label(~fg)
    border = set(np.unique(np.r_[lab[0], lab[-1], lab[:, 0], lab[:, -1]]))
    hole_sizes = [int((lab == i).sum()) for i in range(1, n + 1) if i not in border]
    opened = ndimage.binary_opening(fg, np.ones((2, 2), bool))
    return {"pieces": int(pieces), "holes": len(hole_sizes),
            "tiny_holes": sum(s == 1 for s in hole_sizes),
            "thin": round(float((fg & ~opened).sum() / max(fg.sum(), 1)), 3),
            "cover": round(float(fg.mean()), 3)}, fg


def preview(fg, scale=24):
    """The coarse mask as a picture the model can look at (with grid lines)."""
    H, W = fg.shape
    im = Image.fromarray(np.where(fg, 0, 255).astype(np.uint8)).resize(
        (W * scale, H * scale), Image.NEAREST).convert("RGB")
    d = ImageDraw.Draw(im)
    for x in range(0, W * scale, scale):
        d.line([(x, 0), (x, H * scale)], fill=(200, 200, 200))
    for y in range(0, H * scale, scale):
        d.line([(0, y), (W * scale, y)], fill=(200, 200, 200))
    return im


def mask_all(crop, st, out, i, presets):
    """Masks + metrics of one cropped candidate at every preset."""
    crop.save(f"{out}/{i}_{st}_crop.png")
    res = {}
    for name, p in presets.items():
        cells = grid_for(crop, p["corridor"])
        m, fg = metrics(P.load_mask(f"{out}/{i}_{st}_crop.png", cells))
        m["grid"] = f"{fg.shape[1]}x{fg.shape[0]}"
        res[name] = m
        Image.fromarray(np.where(fg, 0, 255).astype(np.uint8)).save(
            f"{out}/{i}_{st}_{name}.png")
    return res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("--target", default="medium")
    ap.add_argument("--model", default="gemini-3.1-flash-image")
    ap.add_argument("--subjects", type=int, default=len(SUBJECTS))
    ap.add_argument("--stage2", choices=["none", "preview", "text"], default="none",
                    help="none (round 2 default): stage 1 only; preview: show the "
                         "model its masked grid and redraw; text: redraw from a "
                         "written critique of the preflight metrics")
    ap.add_argument("--n", type=int, default=1,
                    help="independent stage-1 candidates per subject (stages c0..)")
    ap.add_argument("--size", default="512", help="imageSize: 512, 1K, 2K")
    ap.add_argument("--workers", type=int, default=3)
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    presets = P.load_presets(os.path.join(os.path.dirname(__file__), "..", "presets.toml"))
    tgt = presets[a.target]["corridor"]
    # nominal portrait grid for the prompt text
    tw = grid_for(Image.new("L", (3, 4)), tgt)
    th = round(tw * 4 / 3)
    usage = []

    def candidate(job):
        i, k = job
        s = SUBJECTS[i]
        st = f"c{k}" if a.stage2 == "none" else "s1"
        im1 = call(a.model, [{"text": STAGE1.format(s=s, w=tw, h=th)}],
                   size=a.size, usage=usage)
        im1.save(f"{a.out}/{i}_{st}_raw.png")
        c1 = autocrop(im1)
        stages = {st: mask_all(c1, st, a.out, i, presets)}
        if a.stage2 == "none":
            return i, stages
        c = grid_for(c1, tgt)
        m1, fg1 = metrics(mask_at(c1, c, f"{a.out}/{i}_s1_crop.png"))
        H, W = fg1.shape
        if a.stage2 == "preview":
            prev = preview(fg1)
            prev.save(f"{a.out}/{i}_s1_preview.png")
            parts = [{"text": STAGE2.format(s=s, w=W, h=H)},
                     png_part(c1.convert("RGB")), png_part(prev)]
        else:
            txt = STAGE2_TEXT.format(s=s, w=W, h=H, problems=critique(m1),
                                     px=round(200 / W))
            parts = [{"text": txt}, png_part(c1.convert("RGB"))]
        im2 = call(a.model, parts, size=a.size, usage=usage)
        im2.save(f"{a.out}/{i}_s2_raw.png")
        stages["s2"] = mask_all(autocrop(im2), "s2", a.out, i, presets)
        return i, stages

    n = a.n if a.stage2 == "none" else 1
    jobs = [(i, k) for i in range(a.subjects) for k in range(n)]
    with ThreadPoolExecutor(min(a.workers, 3)) as ex:
        res = list(ex.map(candidate, jobs))
    recs = [{"subject": SUBJECTS[i], "stages": {}} for i in range(a.subjects)]
    for i, stages in res:
        recs[i]["stages"].update(stages)
    for r in recs:
        r["stages"] = dict(sorted(r["stages"].items()))
    json.dump(recs, open(f"{a.out}/metrics.json", "w"), indent=1)
    json.dump({"model": a.model, "kind": "image", "calls": usage},
              open(f"{a.out}/usage.json", "w"), indent=1)
    for r in recs:
        for st, d in r["stages"].items():
            print(f"{r['subject'][:22]:22} {st} " + " | ".join(
                f"{k[0].upper()} {v['grid']} pc{v['pieces']} h{v['holes']} "
                f"t{v['tiny_holes']} thin{v['thin']:.2f} cov{v['cover']:.2f}"
                for k, v in d.items()))


if __name__ == "__main__":
    main()
