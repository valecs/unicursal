"""LLM "hand-drawn" silhouettes: a text model writes pixel art on the exact grid.

No resampling happens: the model's grid *is* the picture-cell mask, so what it
draws is what the maze gets. One repair round if the output is malformed
(wrong size or disconnected). Output feeds judge.py (same file layout and
metrics.json as silhouette_bakeoff.py; stages c0.. = independent candidates).
Auth: Gemini via a proxy-injected x-goog-api-key header; Claude via
ANTHROPIC_API_KEY (anthropic SDK).

Usage: python3 llm_pixel.py OUTDIR [--model M] [--n 3] [--difficulties easy,medium]
"""
import argparse
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor

import numpy as np
from PIL import Image

sys.path.insert(0, os.path.dirname(__file__))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
import picmaze as P  # noqa: E402
from silhouette_bakeoff import SUBJECTS, metrics  # noqa: E402

# Streaming (SSE) keeps bytes flowing while the model thinks; long silent
# requests get cut by intermediate proxies.
API = "https://generativelanguage.googleapis.com/v1beta/models/{m}:streamGenerateContent?alt=sse"
GRIDS = {"easy": (7, 9), "medium": (12, 16), "hard": (30, 40)}   # portrait W x H
WIDE = ("fish", "elephant", "teapot", "turtle")                                      # drawn landscape

PROMPT = """Draw {s} as pixel art on a grid exactly {w} columns wide and {h} rows tall.
It is the hidden picture in a children's maze: only the silhouette survives, so
the OUTLINE must make it recognisable on its own.
Rules:
- Use '#' for the picture and '.' for background. Exactly {h} lines of exactly {w} characters.
- All '#' cells form ONE piece connected through edges (not just corners).
- Every part (legs, ears, tail, fins) at least 2 cells thick where possible.
- Interior holes ('.' surrounded by '#') only for a key feature such as eyes,
  and each at least 2x2 cells; otherwise none.
- Exaggerate the features that identify it (cat ears and tail, elephant trunk...).
  Choose the view (side or front) that is most recognisable as a silhouette.
- Fill most of the grid; it may touch the edges.
Reply with the grid only, inside a ``` block."""

REPAIR = ("Your grid has a problem: {err} Fix it and reply with the corrected "
          "{w}x{h} grid only, inside a ``` block.")


def call_claude(model, turns, effort):
    """turns: [(role, text)], role in {"user", "assistant"}. Returns (text, usage)."""
    import anthropic
    # This environment sets ANTHROPIC_BASE_URL for its own use; talk to the
    # public API explicitly.
    client = anthropic.Anthropic(base_url="https://api.anthropic.com", max_retries=4)
    with client.messages.stream(
            model=model, max_tokens=32000, output_config={"effort": effort},
            messages=[{"role": r, "content": t} for r, t in turns]) as stream:
        msg = stream.get_final_message()
    text = "".join(b.text for b in msg.content if b.type == "text")
    return text, {"promptTokenCount": msg.usage.input_tokens,
                  "candidatesTokenCount": msg.usage.output_tokens}


def call(model, turns, cfg):
    if model.startswith("claude-"):
        return call_claude(model, turns, cfg["effort"])
    contents = [{"role": "model" if r == "assistant" else "user", "parts": [{"text": t}]}
                for r, t in turns]
    body = {"contents": contents, "generationConfig": cfg}
    req = urllib.request.Request(API.format(m=model), data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    for attempt in range(4):
        try:
            text, usage = "", {}
            with urllib.request.urlopen(req, timeout=300) as resp:
                for line in resp:
                    line = line.decode().strip()
                    if not line.startswith("data:"):
                        continue
                    ev = json.loads(line[5:])
                    usage = ev.get("usageMetadata", usage)
                    for c in ev.get("candidates", []):
                        for p in c.get("content", {}).get("parts", []):
                            if not p.get("thought"):
                                text += p.get("text", "")
            return text, usage
        except (urllib.error.URLError, ConnectionError, TimeoutError):
            if attempt == 3:
                raise
            time.sleep(2 ** attempt)


def parse(text, w, h):
    m = re.search(r"```[a-z]*\n(.*?)```", text, re.S)
    body = m.group(1) if m else text
    rows = [r.strip() for r in body.strip().splitlines() if r.strip()]
    rows = [r for r in rows if set(r) <= set("#.")]
    if len(rows) != h or any(len(r) != w for r in rows):
        return None, (f"it must be exactly {h} rows of {w} characters, but has "
                      f"{len(rows)} rows of lengths {sorted(set(map(len, rows)))}.")
    a = np.array([[c == "#" for c in r] for r in rows])
    _, k = P.components(a)
    if k != 1:
        return a, f"the '#' cells form {k} separate pieces; they must form one."
    return a, None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("--model", default="gemini-3.5-flash",
                    help="a Gemini model, or a Claude model id (claude-...)")
    ap.add_argument("--difficulties", default="easy,medium")
    ap.add_argument("--thinking", default="medium",
                    help="Gemini thinkingLevel / Claude effort")
    ap.add_argument("--n", type=int, default=1,
                    help="independent candidates per subject x difficulty (stages c0..)")
    ap.add_argument("--first", type=int, default=0,
                    help="index of the first candidate (to add c1, c2 to an earlier run)")
    ap.add_argument("--subjects", default=None,
                    help="comma-separated subject indices (default: all)")
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    diffs = a.difficulties.split(",")
    cfg = ({"effort": a.thinking} if a.model.startswith("claude-") else
           {"temperature": 1.0, "thinkingConfig": {"thinkingLevel": a.thinking,
                                                   "includeThoughts": True}})

    def one(job):
        i, diff, k = job
        s = SUBJECTS[i]
        w, h = GRIDS[diff]
        if any(k in s for k in WIDE):
            w, h = h, w
        turns = [("user", PROMPT.format(s=s, w=w, h=h))]
        usage, err, grid = [], None, None
        for _ in range(2):
            text, u = call(a.model, turns, cfg)
            usage.append(u)
            grid, err = parse(text, w, h)
            if err is None:
                break
            turns += [("assistant", text), ("user", REPAIR.format(err=err, w=w, h=h))]
        if grid is None:
            grid = np.zeros((h, w), bool)
        img = Image.fromarray(np.where(grid, 0, 255).astype(np.uint8))
        img.save(f"{a.out}/{i}_c{k}_{diff}.png")
        m, _ = metrics(grid) if grid.any() else ({"pieces": 0}, None)
        m["grid"] = f"{w}x{h}"
        m["error"] = err
        return i, diff, k, m, usage

    subj = (list(map(int, a.subjects.split(","))) if a.subjects
            else range(len(SUBJECTS)))
    ks = range(a.first, a.first + a.n)
    jobs = [(i, d, k) for i in subj for d in diffs for k in ks]
    with ThreadPoolExecutor(3) as ex:
        res = list(ex.map(one, jobs))
    recs = [{"subject": s, "stages": {}} for s in SUBJECTS]
    mpath = f"{a.out}/metrics.json"
    if os.path.exists(mpath):          # merge with an earlier run
        for r in json.load(open(mpath)):
            recs[SUBJECTS.index(r["subject"])]["stages"] = r["stages"]
    tok_in = tok_out = 0
    calls = []
    for i, d, k, m, usage in res:
        recs[i]["stages"].setdefault(f"c{k}", {})[d] = m
        calls += usage
        for u in usage:
            tok_in += u.get("promptTokenCount", 0)
            tok_out += u.get("candidatesTokenCount", 0) + u.get("thoughtsTokenCount", 0)
        print(f"{SUBJECTS[i][:22]:22} {d:6} c{k} {m}")
    recs = [dict(r, index=i, stages=dict(sorted(r["stages"].items())))
            for i, r in enumerate(recs) if r["stages"]]
    json.dump(recs, open(mpath, "w"), indent=1)
    json.dump({"model": a.model, "kind": "text", "calls": calls},
              open(f"{a.out}/usage_{a.first}_{int(time.time())}.json", "w"), indent=1)
    print(f"tokens: {tok_in} in, {tok_out} out (incl. thinking)")


if __name__ == "__main__":
    main()
