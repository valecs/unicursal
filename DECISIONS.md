# UX decisions log

Running record of app-design decisions. Nothing here is final yet.
"First pass" items are initial answers, open to revision; "Proposed" items are
recommendations awaiting a response; "Open" items need research.

## First pass

| # | Topic | Decision |
|---|---|---|
| 1 | Audience | Primary: a parent/teacher making an activity sheet for someone else. Secondary: adult puzzlers / colouring-book fans → a **"Surprise me"** mode that never shows the picture until asked. |
| 1b | Medium | Solved on paper with pencil / marker / pen. On-screen answer view exists only so the maker can check the result. |
| 2 | Platform | Native Android (Kotlin + Compose) wrapping a portable Rust core. |
| 5 | Preflight | Auto-fix wherever a reasonable fix exists; block when none does. A manual mode turns auto-fixes off. |
| 7 | Size control | Detail *is* difficulty. 3–4 presets plus free-form; cells by default, mm in manual mode. Preflight tells the user which features vanish at the chosen difficulty. |
| 11 | Styles | Both Classic (rectilinear grid) and Flow. Branching mazes exist in the prototype for Classic only. |
| 8 | Answer key | Not printed. Saved with the puzzle (viewable in the app); printing it is at most an overflow action. |
| 9 | Advanced controls | Camera-style **Auto / Manual** split. Manual covers mm, seed, flow parameters, colours, turning off preflight fixes. |
| 10 | Scope | On-screen solving is out. Sharing is maybe. |

## Proposed (awaiting sign-off)

- **Picture sources (v1):** photo via subject segmentation, symbol/emoji
  search, text, finger drawing → all feed one silhouette editor + preflight.
- **PWA as a second front end** on the same Rust core (WASM), aimed at
  desktop/teacher printing, iOS stopgap, and opening shared puzzles. Native
  Android stays the flagship. Core requirements this implies now: no filesystem
  or required threads, a stepwise/cancellable flow API with progress, and
  SVG + PDF output produced by the core itself.
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
- **Bug:** branching-maze mode fails "solution covers the picture" for ~1 in 3
  seeds (e.g. `ghost --mode maze --corridor 7 --seed 1`). Passes with
  `--no-deadend-fix`, so the dead-end reduction is opening walls into the
  solution path.
