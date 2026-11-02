"""Blind recognisability judge for bake-off masks.

A vision model sees only the coarse mask (no prompt, no subject) and names it.
A mask "passes" if the answer contains one of the subject's keywords.
Auth: proxy-injected x-goog-api-key header.

Usage: python3 judge.py BAKEDIR [BAKEDIR ...]
"""
import base64
import io
import json
import os
import sys
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor

from PIL import Image

MODEL = "gemini-3.1-flash-lite"
API = "https://generativelanguage.googleapis.com/v1beta/models/{m}:generateContent"
ASK = ("This blocky black-on-white image is a silhouette of a single everyday "
       "thing, as it might appear in a children's puzzle. What is it? Answer "
       "with one or two words only.")
USAGE = []
KEYS = {"cat": ["cat", "kitten"], "jack-o-lantern": ["pumpkin", "jack"],
        "owl": ["owl"], "rocket": ["rocket", "spaceship", "space ship"],
        "fish": ["fish"], "house": ["house", "home", "cottage", "hut"],
        "elephant": ["elephant"], "butterfly": ["butterfly", "moth"],
        "rabbit": ["rabbit", "bunny", "hare"], "sailboat": ["sailboat", "boat", "yacht", "ship"],
        "teapot": ["teapot", "kettle", "tea pot"], "turtle": ["turtle", "tortoise"]}


def keys_for(subject):
    return next(v for k, v in KEYS.items() if k in subject)


def ask(path):
    im = Image.open(path).convert("L")
    w, h = im.size
    s = max(1, 384 // max(w, h))
    b = io.BytesIO()
    im.resize((w * s, h * s), Image.NEAREST).save(b, "PNG")
    body = {"contents": [{"parts": [
        {"text": ASK},
        {"inlineData": {"mimeType": "image/png",
                        "data": base64.b64encode(b.getvalue()).decode()}}]}],
        "generationConfig": {"temperature": 0}}
    req = urllib.request.Request(API.format(m=MODEL), data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    for attempt in range(5):
        try:
            r = json.load(urllib.request.urlopen(req, timeout=120))
            break
        except (urllib.error.URLError, ConnectionError, TimeoutError):
            if attempt == 4:
                raise
            time.sleep(2 ** attempt)
    USAGE.append(r.get("usageMetadata", {}))
    return r["candidates"][0]["content"]["parts"][0]["text"].strip().lower()


def rank(row, m):
    """Best-candidate order: judge pass first, then cleaner preflight metrics."""
    return (not row["pass"], m.get("pieces", 9), m.get("tiny_holes", 9),
            m.get("thin", 1), -m.get("cover", 0))


def main():
    jobs, metr, cache = [], {}, {}
    for d in sys.argv[1:]:
        recs = json.load(open(f"{d}/metrics.json"))
        jpath = f"{d}/judge.json"
        if os.path.exists(jpath):   # reuse verdicts for masks older than them
            t = os.path.getmtime(jpath)
            for r in json.load(open(jpath)):
                png = f"{d}/{r['i']}_{r['stage']}_{r['difficulty']}.png"
                if os.path.exists(png) and os.path.getmtime(png) < t:
                    cache[(d, r["i"], r["stage"], r["difficulty"])] = r["answer"]
        for n, r in enumerate(recs):
            i = r.get("index", n)
            for st in r["stages"]:
                for diff, m in r["stages"][st].items():
                    jobs.append((d, i, r["subject"], st, diff))
                    metr[(d, i, st, diff)] = m

    def run(j):
        d, i, subj, st, diff = j
        ans = cache.get((d, i, st, diff)) or ask(f"{d}/{i}_{st}_{diff}.png")
        return j, ans, any(k in ans for k in keys_for(subj))

    with ThreadPoolExecutor(3) as ex:
        res = list(ex.map(run, jobs))
    out = {}
    for (d, i, subj, st, diff), ans, ok in res:
        out.setdefault(d, []).append({"i": i, "subject": subj, "stage": st,
                                      "difficulty": diff, "answer": ans, "pass": ok})
    for d, rows in out.items():
        json.dump(rows, open(f"{d}/judge.json", "w"), indent=1)
        groups = {}
        for r in rows:
            groups.setdefault((r["i"], r["difficulty"]), []).append(r)
        best = []
        for (i, diff), g in sorted(groups.items()):
            cands = [r for r in g if r["stage"].startswith("c")] or g
            b = min(cands, key=lambda r: rank(r, metr[(d, i, r["stage"], diff)]))
            best.append({**b, "candidates": len(cands),
                         "passing": sum(r["pass"] for r in cands)})
        json.dump(best, open(f"{d}/best.json", "w"), indent=1)
        print(d.rstrip("/").split("/")[-1])
        for st in sorted({r["stage"] for r in rows}):
            line = []
            for diff in ("easy", "medium", "hard"):
                sel = [r for r in rows if r["stage"] == st and r["difficulty"] == diff]
                if sel:
                    line.append(f"{diff} {sum(r['pass'] for r in sel)}/{len(sel)}")
            print(f"  {st}: " + "  ".join(line))
        line = []
        for diff in ("easy", "medium", "hard"):
            sel = [b for b in best if b["difficulty"] == diff]
            if sel:
                line.append(f"{diff} {sum(b['pass'] for b in sel)}/{len(sel)}")
        print("  best-of-N: " + "  ".join(line))
        for b in best:
            if not b["pass"]:
                ans = sorted({r["answer"] for r in groups[(b["i"], b["difficulty"])]})
                print(f"    miss {b['subject'][:18]:18} {b['difficulty']:6} -> {', '.join(ans)}")
    json.dump({"model": MODEL, "kind": "text", "calls": USAGE},
              open(f"{sys.argv[1]}/../judge_usage_{int(time.time())}.json", "w"))
    print(f"judge calls: {len(USAGE)}")


if __name__ == "__main__":
    main()
