# UX decisions log

Running record of app-design decisions. "Settled" items are agreed; "Proposed"
items are recommendations awaiting sign-off; "Open" items need research.

## Settled

| # | Topic | Decision |
|---|---|---|
| 1 | Audience | Primary: a parent/teacher making an activity sheet for someone else. Secondary: adult puzzlers / colouring-book fans → a **"Surprise me"** mode that never shows the picture until asked. |
| 1b | Medium | Solved on paper with pencil / marker / pen. On-screen answer view exists only so the maker can check the result. |
| 2 | Platform priority | Android phones first, using current Android APIs. Cross-platform only if it costs no UX. |
| 5 | Preflight | Auto-fix wherever a reasonable fix exists; block when none does. A manual mode turns auto-fixes off. |
| 7 | Size control | 3–4 difficulty presets plus a free-form setting. Shown as grid cells by default, millimetres in manual mode. |
| 8 | Answer key | Not printed. Saved with the puzzle (viewable in the app); printing it is at most an overflow action. |
| 9 | Advanced controls | Camera-style **Auto / Manual** split. Manual covers mm, seed, flow parameters, colours, turning off preflight fixes. |
| 10 | Scope | On-screen solving is out. Sharing is maybe. |

## Proposed (awaiting sign-off)

- **Platform:** native Android UI (Kotlin + Jetpack Compose) on a portable
  **Rust core** (JNI on Android; WASM/iOS later reuse the same core).
- **Picture sources (v1):** photo via subject segmentation, symbol/emoji
  search, text, finger drawing → all feed one silhouette editor + preflight.
- **Shuffle = swipe:** the preview is a horizontal pager of seeds; swiping
  generates the next layout. Press-and-hold to peek at the answer.

## Open

- Picture sourcing research (segmentation quality on real photos; symbol
  sets and licences; whether pixel-art editing is needed in v1).
- Carousel vs. other ways of comparing candidates; needs a prototype test.
- Calibrating difficulty presets against real test prints.
