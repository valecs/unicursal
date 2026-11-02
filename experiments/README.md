# Silhouette experiments

Scripts for choosing how the app turns "describe it" into a picture mask.
Results so far are in `../DECISIONS.md` → Findings.

| Script | What it does |
|---|---|
| `ai_silhouettes.py` | First probe: two Gemini image models, raw output. |
| `silhouette_bakeoff.py` | Image model → auto-crop → masks at each preset; optional stage 2 (`--stage2 preview\|text`). |
| `llm_pixel.py` | Text model draws pixel art on the exact grid (`--model gemini-…` or `claude-…`). |
| `judge.py` | Blind recognisability check: a cheap vision model names each mask. |

## Auth

- **Gemini**: a network secret injects `x-goog-api-key` for
  `generativelanguage.googleapis.com`; scripts send no key.
- **Claude**: `ANTHROPIC_API_KEY` in the environment (environment variables
  reach new sessions only). `api.anthropic.com` bypasses the agent proxy, so a
  header-injection secret would not work for it. The scripts pass
  `base_url="https://api.anthropic.com"` because the cloud environment sets
  `ANTHROPIC_BASE_URL` for its own use.

## Gotchas

- The agent proxy cuts tunnels that are silent for ~30 s. Long-thinking calls
  must stream (`llm_pixel.py` uses SSE / SDK streaming). Keep concurrency ≤ 3.
- `gemini-2.5-flash` (text) returns 404 here; use `gemini-3.5-flash` or
  `gemini-3.1-flash-lite`.

## Round 2 (planned)

~12 subjects, best-of-3 per method, judge picks the best candidate:

```
python3 experiments/silhouette_bakeoff.py OUT/img                      # image model
python3 experiments/llm_pixel.py OUT/px_gemini                         # Gemini 3.5 Flash
python3 experiments/llm_pixel.py OUT/px_haiku  --model claude-haiku-5-5
python3 experiments/llm_pixel.py OUT/px_sonnet --model claude-sonnet-5-5
python3 experiments/judge.py OUT/img OUT/px_gemini OUT/px_haiku OUT/px_sonnet
```

Rough cost per 16 drawings (8 subjects × Easy/Medium): Gemini 3.5 Flash
≈ 110k output tokens; Haiku 5.5 ≈ $0.05; Sonnet 5.5 ≈ $1. Image model ≈ $0.07
per image. Round 1 spent ≈ $3–3.5 of Gemini credit in total.
