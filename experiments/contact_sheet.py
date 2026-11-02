"""Contact sheets for a bake-off round.

Per method: rows = subjects, columns = difficulty x candidate; green frame =
judge pass, red = fail, thick frame = the candidate judge.py picked.
Summary: rows = subjects, columns = method x difficulty, best candidate only.

Usage: python3 contact_sheet.py OUTDIR METHODDIR [METHODDIR ...]
"""
import json
import os
import sys

from PIL import Image, ImageDraw, ImageFont

BOX, PAD, LAB = 96, 8, 150
DIFFS = ("easy", "medium", "hard")
GREEN, RED, GREY = (30, 150, 60), (200, 40, 40), (200, 200, 200)


def font(sz):
    for p in ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",):
        if os.path.exists(p):
            return ImageFont.truetype(p, sz)
    return ImageFont.load_default()


def tile(path, ok, best):
    t = Image.new("RGB", (BOX, BOX), "white")
    if os.path.exists(path):
        m = Image.open(path).convert("L")
        s = min((BOX - 12) / m.width, (BOX - 12) / m.height)
        m = m.resize((max(1, round(m.width * s)), max(1, round(m.height * s))), Image.NEAREST)
        t.paste(m, ((BOX - m.width) // 2, (BOX - m.height) // 2))
    d = ImageDraw.Draw(t)
    col = GREY if ok is None else (GREEN if ok else RED)
    w = 5 if best else 2
    d.rectangle([0, 0, BOX - 1, BOX - 1], outline=col, width=w)
    return t


def load(d):
    judge = {(r["i"], r["stage"], r["difficulty"]): r for r in json.load(open(f"{d}/judge.json"))}
    best = {(b["i"], b["difficulty"]): b for b in json.load(open(f"{d}/best.json"))}
    return judge, best


def grid_sheet(title, rowlabels, collabels, cells, path):
    f, fs = font(16), font(13)
    W = LAB + len(collabels) * (BOX + PAD) + PAD
    H = 40 + 24 + len(rowlabels) * (BOX + PAD) + PAD
    im = Image.new("RGB", (W, H), "white")
    d = ImageDraw.Draw(im)
    d.text((PAD, 10), title, fill="black", font=f)
    for c, lab in enumerate(collabels):
        d.text((LAB + c * (BOX + PAD), 44), lab, fill="black", font=fs)
    for r, lab in enumerate(rowlabels):
        y = 64 + r * (BOX + PAD)
        d.text((PAD, y + BOX // 2 - 8), lab, fill="black", font=fs)
        for c in range(len(collabels)):
            t = cells(r, c)
            if t is not None:
                im.paste(t, (LAB + c * (BOX + PAD), y))
    im.save(path)
    return path


def main():
    out, dirs = sys.argv[1], sys.argv[2:]
    os.makedirs(out, exist_ok=True)
    summary_cols = []
    for d in dirs:
        name = d.rstrip("/").split("/")[-1]
        recs = json.load(open(f"{d}/metrics.json"))
        judge, best = load(d)
        idx = [r.get("index", n) for n, r in enumerate(recs)]
        stages = sorted({s for r in recs for s in r["stages"]})
        diffs = [x for x in DIFFS if any(x in r["stages"][s] for r in recs for s in r["stages"])]
        cols = [(x, s) for x in diffs for s in stages]

        def cell(r, c, d=d, idx=idx, cols=cols, judge=judge, best=best):
            i, (diff, st) = idx[r], cols[c]
            j = judge.get((i, st, diff))
            b = best.get((i, diff))
            return tile(f"{d}/{i}_{st}_{diff}.png", j and j["pass"],
                        b is not None and b["stage"] == st)
        grid_sheet(f"{name}: all candidates (green = judge pass, thick = picked)",
                   [r["subject"] for r in recs], [f"{x[:3]} {s}" for x, s in cols],
                   cell, f"{out}/sheet_{name}.png")
        for diff in diffs:
            summary_cols.append((name, d, diff, idx, best))
    subjects = json.load(open(f"{dirs[0]}/metrics.json"))
    sidx = [r.get("index", n) for n, r in enumerate(subjects)]

    def scell(r, c):
        name, d, diff, idx, best = summary_cols[c]
        b = best.get((sidx[r], diff))
        if b is None:
            return None
        return tile(f"{d}/{b['i']}_{b['stage']}_{diff}.png", b["pass"], False)
    grid_sheet("Best candidate per method x difficulty (green = judge pass)",
               [r["subject"] for r in subjects],
               [f"{n[:9]} {x[:3]}" for n, _, x, _, _ in summary_cols],
               scell, f"{out}/sheet_best.png")
    print("wrote", out)


if __name__ == "__main__":
    main()
