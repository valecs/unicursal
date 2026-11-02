# unicursal

Picture-labyrinth generator: a silhouette becomes a printable maze whose
solution, filled in, reveals the picture. Currently a Python prototype
(`picmaze.py`, `flow.py`) ahead of a Rust core + PWA. Start with `README.md`;
background in `HANDOFF.md`; every product decision in `DECISIONS.md`.

## Setup and checks

```
pip install -r requirements.txt          # numpy, Pillow, scipy, pytest, hypothesis
apt-get update && apt-get install -y librsvg2-bin   # rsvg-convert: PDFs + flow leak check
python3 -m pytest -q tests               # must pass before every push
```

## Testing: test-driven, property-based

- **Write the failing test first.** A bug fix starts with a test that
  reproduces it; a feature starts with tests for its behaviour. Commit the
  test with the change.
- **Prefer properties to examples.** State what must hold for *every* input
  and let Hypothesis (Python) / proptest (Rust) search for counterexamples:
  generate arbitrary masks, seeds, paper sizes, presets. Keep a few
  hand-picked examples for readability and regressions. See
  `tests/test_properties.py`; it found the #7 bug in seconds, shrunk to a
  one-cell picture.
- **The invariants are the spec** (`tests/test_invariants.py`): labyrinth cells
  have exactly two openings except the two solution ends; maze passages form
  a spanning tree; the solution covers exactly the picture; flow has no
  crossings, one open region per strand, and a minimum clear width. Any port
  must pass the same properties.
- Never skip, weaken or xfail a test to get green without the owner's say-so.
  A known bug may be pinned with `xfail(strict=True)` and an issue number.
- Determinism matters (puzzle links regenerate from a seed, see #5): tests
  that fix a seed must keep producing the same result; flag any change that
  alters outputs for existing seeds.

## Work tracking

- **Issues track overall progress**: one per decision, bug, research question
  or chunk of work. Labels: `decision` (needs the owner's call), `bug`,
  `research`, `prototype` (Python), `core` (Rust/app). Close an issue when its
  outcome is recorded (DECISIONS.md for decisions).
- **Draft PRs hold in-progress work and its questions.** Sessions are woken by
  activity on PRs they're subscribed to, not by issue comments. So questions
  about a task in progress go on that task's draft PR (as a comment or in the
  description), not on the issue. Subscribe to your PR's activity when you
  open it.
- **DECISIONS.md is the record** of what was decided and why. Keep edits
  small and scoped to your workstream so concurrent PRs merge cleanly.

## Workstreams, branches and PRs

- **One workstream = one branch = one PR**, branched from up-to-date `main`.
  Open the PR as a draft early; its description says what the workstream
  covers and which issues it addresses (`Closes #n` / `Refs #n`). Mark it
  ready when done.
- **Only the owner merges.** Never merge, approve, or push to `main` directly.
- **Never push to another workstream's branch** or open PRs into it. If two
  workstreams need the same change, land it on `main` through its own small PR.
- **After a merge, start the next piece of work on a fresh branch from `main`.**
  Don't keep adding to a merged branch.
- Keep PRs focused: one concern each. Unrelated fixes found along the way get
  their own issue or PR.

## Git etiquette

- `git pull --rebase` your own branch before pushing; to pick up `main`, merge
  it in (`git merge origin/main`) rather than rebasing a branch others may
  have checked out. No force-pushes to shared branches.
- Commit messages: imperative subject ≤ ~72 chars, a body that says *why*,
  and issue references (`(#7)`).
- Commit tests with the code they cover. Don't commit generated outputs
  (renders, experiment images) unless they're deliberate examples.
- Don't put secrets in the repo. API keys come from the environment (see
  `experiments/README.md`).
