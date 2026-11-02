"""Spend ledger: sums every usage*.json under the given round directories.

Prices are USD per million tokens (list prices, Nov 2026; check before
relying on them). Gemini "thinking" tokens bill as output. Image models bill
image tokens at the image rate (a 512 px image = 747 tokens, a 1K = 1120).

Usage: python3 cost.py ROUNDDIR [ROUNDDIR ...]
"""
import glob
import json
import sys

PRICES = {  # model: (input, text/thinking output, image output)
    "gemini-3.1-flash-image": (0.50, 3.00, 60.00),
    "gemini-3.5-flash": (1.50, 9.00, None),
    "gemini-3.1-flash-lite": (0.25, 1.50, None),
    "claude-haiku-5-5": (0.10, 0.50, None),
    "claude-sonnet-5-5": (2.00, 10.00, None),
}


def cost(model, u):
    pin, pout, pimg = PRICES[model]
    img = sum(d["tokenCount"] for d in u.get("candidatesTokensDetails", [])
              if d.get("modality") == "IMAGE")
    out = u.get("candidatesTokenCount", 0) - img + u.get("thoughtsTokenCount", 0)
    return (u.get("promptTokenCount", 0) * pin + out * pout + img * (pimg or 0)) / 1e6


def ledger(dirs):
    rows = {}
    for d in dirs:
        for f in sorted(glob.glob(f"{d}/**/usage*.json", recursive=True)
                        + glob.glob(f"{d}/judge_usage_*.json")):
            u = json.load(open(f))
            key = (f.split("/")[-2] if "judge_usage" not in f else "judge", u["model"])
            c = sum(cost(u["model"], x) for x in u["calls"])
            n0, c0 = rows.get(key, (0, 0))
            rows[key] = (n0 + len(u["calls"]), c0 + c)
    return rows


if __name__ == "__main__":
    rows = ledger(sys.argv[1:])
    tot = 0
    for (name, model), (n, c) in sorted(rows.items()):
        print(f"{name:14} {model:24} {n:4} calls  ${c:6.3f}")
        tot += c
    print(f"{'total':14} {'':24} {'':4}        ${tot:6.3f}")
