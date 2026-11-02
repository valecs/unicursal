"""Blind recognisability judge for bake-off masks.

A vision model sees only the coarse mask (no prompt, no subject) and names it.
A mask "passes" if the answer contains one of the subject's keywords.
Auth: proxy-injected x-goog-api-key header.

Usage: python3 judge.py BAKEDIR [BAKEDIR ...]
"""
import base64
import io
import json
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
KEYS = {"cat": ["cat", "kitten"], "jack-o-lantern": ["pumpkin", "jack"],
        "owl": ["owl"], "rocket": ["rocket", "spaceship", "space ship"],
        "fish": ["fish"], "house": ["house", "home", "cottage", "hut"],
        "elephant": ["elephant"], "butterfly": ["butterfly", "moth"]}


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
    return r["candidates"][0]["content"]["parts"][0]["text"].strip().lower()


def main():
    jobs = []
    for d in sys.argv[1:]:
        recs = json.load(open(f"{d}/metrics.json"))
        for i, r in enumerate(recs):
            for st in r["stages"]:
                for diff in r["stages"][st]:
                    jobs.append((d, i, r["subject"], st, diff))

    def run(j):
        d, i, subj, st, diff = j
        ans = ask(f"{d}/{i}_{st}_{diff}.png")
        return j, ans, any(k in ans for k in keys_for(subj))

    with ThreadPoolExecutor(3) as ex:
        res = list(ex.map(run, jobs))
    out = {}
    for (d, i, subj, st, diff), ans, ok in res:
        out.setdefault(d, []).append({"i": i, "subject": subj, "stage": st,
                                      "difficulty": diff, "answer": ans, "pass": ok})
    for d, rows in out.items():
        json.dump(rows, open(f"{d}/judge.json", "w"), indent=1)
        print(d.rstrip("/").split("/")[-1])
        for st in ("s1", "s2"):
            line = []
            for diff in ("easy", "medium", "hard"):
                sel = [r for r in rows if r["stage"] == st and r["difficulty"] == diff]
                line.append(f"{diff} {sum(r['pass'] for r in sel)}/{len(sel)}")
            print(f"  {st}: " + "  ".join(line))
        for r in sorted(rows, key=lambda r: (r["i"], r["stage"])):
            if not r["pass"]:
                print(f"    miss {r['subject'][:18]:18} {r['stage']} {r['difficulty']:6} -> {r['answer']}")


if __name__ == "__main__":
    main()
