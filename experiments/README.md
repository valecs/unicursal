# Silhouette experiments

Scripts for choosing how the app turns "describe it" into a picture mask.
Results so far are in `../DECISIONS.md` → Findings.

| Script | What it does |
|---|---|
| `ai_silhouettes.py` | First probe: two Gemini image models, raw output. |
| `silhouette_bakeoff.py` | Image model → auto-crop → masks at each preset; optional stage 2 (`--stage2 preview\|text`). |
| `llm_pixel.py` | Text model draws pixel art on the exact grid (`--model gemini-…` or `claude-…`). |
| `judge.py` | Blind recognisability check: a cheap vision model names each mask; with candidates `c0..`, picks the best per subject × difficulty (`best.json`). |
| `cost.py` | Spend ledger from the `usage*.json` files every script writes. |
| `contact_sheet.py` | Contact sheets: every candidate per method, plus the picked best per method × difficulty. |

## Auth

- **Gemini**: a network secret injects `x-goog-api-key` for
  `generativelanguage.googleapis.com`; scripts send no key.
- **Claude**: `ANTHROPIC_API_KEY` (or `UNICURSAL_ANTHROPIC_KEY`, in case the
  environment filters `ANTHROPIC_*` names) in the environment (environment variables
  reach new sessions only). `api.anthropic.com` bypasses the agent proxy, so a
  header-injection secret would not work for it. The scripts pass
  `base_url="https://api.anthropic.com"` because the cloud environment sets
  `ANTHROPIC_BASE_URL` for its own use.

## Gotchas

- The agent proxy cuts tunnels that are silent for ~30 s. Long-thinking calls
  must stream (`llm_pixel.py` uses SSE / SDK streaming). Keep concurrency ≤ 3.
- `gemini-2.5-flash` (text) returns 404 here; use `gemini-3.5-flash` or
  `gemini-3.1-flash-lite`.

## Round 2

12 subjects (round 1's 8 + rabbit, sailboat, teapot, turtle), best-of-3
per method, the judge picks the best candidate. Results in `round2/`.

```
R=experiments/round2
python3 experiments/silhouette_bakeoff.py $R/img --n 3          # image model, stage 1 only, 512 px
python3 experiments/llm_pixel.py $R/px_gemini --n 1             # Gemini 3.5 Flash, candidate c0
python3 experiments/llm_pixel.py $R/px_gemini --n 2 --first 1   # ... add c1, c2
python3 experiments/llm_pixel.py $R/px_haiku  --n 3 --model claude-haiku-5-5
python3 experiments/llm_pixel.py $R/px_sonnet --n 3 --model claude-sonnet-5-5
python3 experiments/judge.py $R/img $R/px_gemini $R/px_haiku $R/px_sonnet
python3 experiments/contact_sheet.py $R/sheets $R/img $R/px_gemini ...
python3 experiments/cost.py $R
```

Defaults changed from round 1: `silhouette_bakeoff.py` now runs stage 1 only
(`--stage2 preview|text` restores the two-stage runs) at 512 px (`--size 1K`
for round 1's size), with at most 3 workers.

Prices used by `cost.py` (per M tokens, in/out): Gemini 3.5 Flash $1.50/$9
(thinking bills as output), 3.1 Flash Image $60 per M image tokens (512 px =
747 tokens ≈ $0.045; 1K ≈ $0.067), Haiku 5.5 $0.10/$0.50, Sonnet 5.5 $2/$10.
