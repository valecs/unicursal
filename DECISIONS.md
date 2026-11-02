# UX decisions log

Running record of app-design decisions. Nothing here is final yet.
Open items are tracked as GitHub issues; see WORKFLOW.md for how instances coordinate.
"First pass" items are initial answers, open to revision; "Proposed" items are
recommendations awaiting a response; "Open" items need research.

## First pass

| # | Topic | Decision |
|---|---|---|
| 1 | Audience | Primary: a parent/teacher making an activity sheet for someone else. Secondary: adult puzzlers / colouring-book fans → a **"Surprise me"** mode that never shows the picture until asked. |
| 1b | Medium | Solved on paper with pencil / marker / pen. On-screen answer view exists only so the maker can check the result. |
| 2 | Platform | **PWA first** (confirmed, #6) on a portable Rust core compiled to WASM. Play Store later via a Trusted Web Activity if wanted. No puzzle backend: static hosting of versioned builds; the only server piece is a thin, stateless proxy for AI "describe it". Core: no filesystem, no required threads, steppable/cancellable flow with progress, SVG + PDF from the core, **deterministic** (no fast-math, no thread-order-dependent sums, pure-Rust libm). |
| 2b | Core port | "Surface-level" conversion: same behaviour and CLI surface as the Python prototype, idiomatic Rust inside (no need to mirror Python's module shape). Cargo workspace: `unicursal-core` (lib: pure, no I/O; grey buffer + params → geometry, SVG, PDF bytes; all tests), `unicursal-cli` (bin: image decoding, files, flags like the prototype's), `unicursal-wasm` (wasm-bindgen API for the PWA). Port the invariant tests first. |
| 5 | Preflight | Auto-fix wherever a reasonable fix exists; block when none does. A manual mode turns auto-fixes off. |
| 7 | Size control | Detail *is* difficulty. Presets are **data** (`presets.toml`, `--preset`, explicit flags override); exact values deferred until test prints. Cells by default, mm in manual mode. Preflight says which features vanish at the chosen difficulty. |
| 11 | Styles | Both Classic (rectilinear grid) and Flow. Branching mazes exist in the prototype for Classic only; the dead-end heuristic is **off by default** (opt-in `--deadend-fix`); its bug (#7) is fixed. |
| 12 | Output | Configurable paper size; PDF output at true scale; portrait/landscape auto (follows picture) or forced. Prototype: `--paper`, `--orientation auto\|portrait\|landscape`. |
| 8 | Answer key | Not printed. Saved with the puzzle (viewable in the app); printing it is at most an overflow action. |
| 9 | Advanced controls | Camera-style **Auto / Manual** split. Manual covers mm, seed, flow parameters, colours, turning off preflight fixes. |
| 10 | Scope | On-screen solving is out. Sharing is maybe. |
| 14 | Printed page | Minimal: the maze, start/finish marks (#2), and the existing one-line footer. No title, name/date lines or instructions (classroom extras are out of scope). A 5 cm check bar in the footer ("5 cm. If not, print at 100% / actual size") guards against fit-to-page printing. **Space goes to the maze**: the frame fills the printable area (the entrance stub sits in the paper margin), and the footer moves to the side away from the entrance. On Letter: Easy 7×9 → 8×11, Medium 12×16 → 13×17, Hard 30×40 → 31×41 (ghost). |
| 15 | Layouts | 3 layouts generated up front + a "more layouts" action. Ranking is only an internal heuristic for which shows first; no "best match" label (#4). |
| 16 | Start | One arrow pointing in at the entrance; no Start/End labels (start and end are adjacent) (#2). |
| 13 | Voids | Solid black by default; puzzles must print well in black and white. Ink-saving is not a priority. (`--ink-saver`, light voids with outlined corridors, exists in the prototype but stays off and out of the UI.) |

## Proposed (awaiting sign-off)

- **Picture sources (v1):** AI "describe it", image file (1-bit, connected),
  text, grid drawing → all feed preflight. No photo segmentation in v1.
- **Puzzle links** (#5): a URL encodes generator version + settings + seed +
  picture mask (~150–250 chars) and regenerates the puzzle exactly. Relies on
  WASM determinism; old versions stay loadable at versioned paths. History =
  a list of these codes. Sharing = link, plus PDF via the share sheet.
- **Preflight** (#3): auto-crop so the subject touches one edge (entrance
  there) with a small margin elsewhere; bridge pieces (highlighted); show
  details that won't survive by highlighting them (no naming); block only
  near-empty pictures; failed generations retry silently.
- **Shuffle = swipe:** the preview is a horizontal pager of seeds; swiping
  generates the next layout. Press-and-hold to peek at the answer.

Mockup: https://claude.ai/artifact/4MF23wdHWZykkbZL7aivSY (main screen + picture chooser).

## Open

- Picture sourcing: AI generation (image model vs. LLM-drawn shapes) needs a
  bake-off; accepting 1-bit connected images as files is a given.
- Calibrating difficulty presets against real test prints.

## Todo

- **Move Python tooling to `uv`** (`pyproject.toml` + `uv.lock`, `uv run pytest`,
  `uv run experiments/...`) to make fresh-session setup faster. System
  packages (`librsvg2-bin`, which needs `apt-get update` first) stay outside uv.
- **Claude arms of bake-off round 2** (Haiku 5.5 best-of-3 Easy+Medium;
  Sonnet 5.5 single shot at Medium) are blocked: no Anthropic key reached the
  session (`ANTHROPIC_API_KEY` / `ANTHROPIC_API_TOKEN` absent; maybe
  `ANTHROPIC_*` names are filtered). Scripts also read `UNICURSAL_ANTHROPIC_KEY`.

## Findings

- **Feature survival by difficulty** (letter paper; holes kept / holes in source):
  ghost 0/3 at Easy (6×8), 3/3 at Medium (11×15); cat 0/4 at Easy (6×7),
  2/4 (1-cell eyes) at Medium (11×13), 4/4 at Hard. At Easy, only the outline
  carries the picture.
- **Presets in the mockup / presets.toml:** Easy 11 mm, Medium 7 mm, Hard 3 mm
  (provisional; difficulty decisions deferred until test prints).
- **AI silhouettes, first probe** (4 subjects × Nano Banana 2.5 Flash Image and
  3.1 Flash Image; `experiments/ai_silhouettes.py`): both obey "pure black and
  white" (<1 % grey pixels) and are recognisable. Both ignore "fill half the
  frame, chunky, no fine detail": coverage 8–38 %, ornate owls. **Auto-crop to
  the subject + fixed margin** roughly doubles usable cells and makes Medium
  readable for simple subjects (pumpkin, rocket). Hard reads well for most;
  Easy reads for none (outline only). Detailed subjects (owl) break into many
  pieces/holes and need simplification or rejection by preflight.
- **Bug (fixed, #7):** with the dead-end heuristic on, branching mazes lost
  the solution for ~1 in 3 seeds: the walk from a dead end to its junction ran
  onto the solution path (path cells look like plain corridor) and cut it
  there. Branches now stop at the path. 0 failures over 240 mazes; the
  heuristic halves the number of dead ends.
- **AI silhouettes, bake-off round 1** (8 subjects; `experiments/`; scored by
  a blind judge, `judge.py`: Gemini 3.1 Flash-Lite sees only the coarse mask and
  names it; pass = right subject):

  | Method | Easy | Medium | Hard |
  |---|---|---|---|
  | Image model (3.1 Flash Image), grid-aware prompt + auto-crop, run 1 | 1/8 | 7/8 | 8/8 |
  | same, run 2 | 2/8 | 6/8 | 8/8 |
  | + stage 2: redraw after seeing its masked grid | 1/8 | 7/8 | 8/8 |
  | + stage 2: redraw from a written critique | 1/8 | 4/8 | 8/8 |
  | Text model (3.5 Flash) drawing pixel art on the exact grid | 2/8 | 6/8 | n/a |

  Two-stage redraw does not help (the model copies the grid preview, or drops
  features). The biggest wins were the stage-1 prompt (icon/stencil style,
  target grid size stated) and auto-crop. Run-to-run variance exceeds the
  stage-2 effect, which argues for **best-of-N + judge** selection. Easy
  (~7x9 cells) fails for almost every subject with every method.
- **AI silhouettes, bake-off round 2** (12 subjects: round 1's 8 + rabbit,
  sailboat, teapot, turtle; `experiments/round2/`, contact sheets in
  `round2/sheets/`). Image model at 512 px, stage 1 only, best of 3 candidates;
  judge as round 1, and among passing candidates the one with the cleanest
  preflight metrics is picked. Budget cap $5 for the round, so pixel arms are
  trimmed (see Todo for the Claude arms).

  | Method | Cands | Easy | Medium | Hard | Cost |
  |---|---|---|---|---|---|
  | Image model (3.1 Flash Image), best of 3 | 3 | 5/12 | 10/12 | 12/12 | $1.65 (36 images) |
  | same, single candidate (c0 / c1 / c2) | 1 | 2 / 2 / 1 | 9 / 10 / 9 | 12 each | $0.55 per set |
  | Gemini 3.5 Flash pixel art, single shot | 1 | 6/12 | 10/12 | n/a | $1.50 (24 drawings, ≈$0.063 each) |

  Visual check (mine, "would a child name it"): agrees with the judge for the
  image model (Medium fails: owl, butterfly). The judge is **lenient on pixel
  art**: at Medium I pass ~8/12 Gemini drawings (elephant has no trunk,
  butterfly reads as a bug; fish, sailboat, turtle borderline), vs the judge's
  10/12. At Easy both methods are ~5–6/12. Takeaways: best-of-3 lifts the
  image model by about 1 at Medium and about 3 at Easy, for 3× the cost; Hard is solved;
  butterfly and owl are the hard subjects at Medium for every method. Text-model drawing is
  not cheaper than images (≈$0.06 vs $0.045 per candidate) and is blockier.
  Round-2 spend so far ≈ $3.3 (Gemini credit used in total ≈ $6.8 of $20).
