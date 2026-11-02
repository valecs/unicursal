# UX decisions log

Running record of app-design decisions. Nothing here is final yet.
"First pass" items are initial answers, open to revision; "Proposed" items are
recommendations awaiting a response; "Open" items need research.

## First pass

| # | Topic | Decision |
|---|---|---|
| 1 | Audience | Primary: a parent/teacher making an activity sheet for someone else. Secondary: adult puzzlers / colouring-book fans → a **"Surprise me"** mode that never shows the picture until asked. |
| 1b | Medium | Solved on paper with pencil / marker / pen. On-screen answer view exists only so the maker can check the result. |
| 2 | Platform | Portable Rust core. Front end under reconsideration: see PWA-first proposal. |
| 2b | Core port | "Surface-level" conversion: same behaviour and CLI surface as the Python prototype, idiomatic Rust inside (no need to mirror Python's module shape). Cargo workspace: `unicursal-core` (lib: pure, no I/O; grey buffer + params → geometry, SVG, PDF bytes; all tests), `unicursal-cli` (bin: image decoding, files, flags like the prototype's), `unicursal-wasm` (wasm-bindgen API for the PWA). Port the invariant tests first. |
| 5 | Preflight | Auto-fix wherever a reasonable fix exists; block when none does. A manual mode turns auto-fixes off. |
| 7 | Size control | Detail *is* difficulty. Presets are **data** (`presets.toml`, `--preset`, explicit flags override); exact values deferred until test prints. Cells by default, mm in manual mode. Preflight says which features vanish at the chosen difficulty. |
| 11 | Styles | Both Classic (rectilinear grid) and Flow. Branching mazes exist in the prototype for Classic only; the dead-end heuristic is **off by default** (opt-in `--deadend-fix`) until its bug is fixed. |
| 12 | Output | Configurable paper size; PDF output at true scale; portrait/landscape auto (follows picture) or forced. Prototype: `--paper`, `--orientation auto\|portrait\|landscape`. |
| 8 | Answer key | Not printed. Saved with the puzzle (viewable in the app); printing it is at most an overflow action. |
| 9 | Advanced controls | Camera-style **Auto / Manual** split. Manual covers mm, seed, flow parameters, colours, turning off preflight fixes. |
| 10 | Scope | On-screen solving is out. Sharing is maybe. |

## Proposed (awaiting sign-off)

- **Picture sources (v1):** AI "describe it", image file (1-bit, connected),
  text, grid drawing → all feed preflight. No photo segmentation in v1.
- **PWA first** on the Rust core (WASM). Serves Android, iOS, desktop
  printing and shared-puzzle links from one build; ship to Play later as a
  Trusted Web Activity if wanted. Native Android becomes a later option, not
  the plan. Photo segmentation dropped from v1. Core requirements: no
  filesystem or required threads, a stepwise/cancellable flow API with
  progress, SVG + PDF output produced by the core itself.
- **Shuffle = swipe:** the preview is a horizontal pager of seeds; swiping
  generates the next layout. Press-and-hold to peek at the answer.

Mockup: https://claude.ai/artifact/4MF23wdHWZykkbZL7aivSY (main screen + picture chooser).

## Open

- Picture sourcing: AI generation (image model vs. LLM-drawn shapes) needs a
  bake-off; accepting 1-bit connected images as files is a given.
- Carousel vs. other ways of comparing candidates; needs a prototype test.
- Calibrating difficulty presets against real test prints.

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
- **Bug:** with the dead-end heuristic on, branching-maze mode fails "solution
  covers the picture" for ~1 in 3 seeds (e.g. `ghost --mode maze --corridor 7
  --seed 1 --deadend-fix`); the heuristic opens walls into the solution path.
  Now off by default; tracked by a strict-xfail test.
