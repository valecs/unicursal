# Working across instances

Several Claude sessions (and people) work on this repo at once. To keep them
from stepping on each other:

- **One branch per workstream**, branched from `main` (or stacked on another
  workstream's branch while that one is unmerged). Never push to another
  workstream's branch.
- **Every branch has a draft PR.** Its description says what the workstream
  covers and lists the issues it addresses (`Closes #n` / `Refs #n`).
  Only the owner merges.
- **Issues are the to-do list.** One issue per open decision, bug or research
  question. Labels: `decision` (needs the owner's call), `bug`, `research`,
  `prototype` (Python prototype work), `core` (Rust port / app). Comment on
  the issue when you start, and link findings back to it.
- **DECISIONS.md is the record.** Keep edits small and scoped to your
  workstream's section so concurrent PRs merge cleanly. Before committing,
  `git pull --rebase` on your own branch.
- **Don't open PRs into someone else's branch, and don't merge.** Leave
  merging to the owner.

Current workstreams:

| Branch | Workstream |
|---|---|
| `ccr-28529330-qeng6w` | Picture sourcing: AI silhouette bake-off (`experiments/`) |
| `ux-decisions` | App UX decisions, printed page, mockup, prototype changes they need |
