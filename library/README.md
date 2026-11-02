# Built-in picture library (plan)

The ready-made pictures that ship with the app and power **Surprise me**
offline (DECISIONS 19, 20; issue #14). Claude draws, checks and selects
them in batch, the way the prototype's `shapes/` were drawn.

Status: **spike** (three Thanksgiving subjects through draw → check →
review). Open questions are on the draft PR.

```
uv run library/build.py              # build missing masters, judge new masks, report
uv run library/build.py turkey --remaster
uv run pytest -q tests/test_library.py
```

Output (not committed): `library/out/sheet.png` (contact sheet),
`report.json`, `library.json` (the provisional package).

## Gate

The pilot runs now. **Scaling past the pilot waits for #8** (difficulty
presets). Easy and Medium overrides are drawn at the exact grid a preset
gives, so a change of corridor width invalidates them.

## Content rule (owner)

No iconography where it would be taboo or might cause offence. In practice:

- no deities, prophets or holy figures (e.g. no Ganesha, no nativity figures);
- nothing figurative in Islamic contexts;
- no sacred names or texts;
- no symbols with a hateful second meaning, even if benign in their own
  tradition (e.g. no swastika);
- no stereotyped depictions of peoples (e.g. no feathered headdresses for
  Thanksgiving).

Pictures from religious or cultural contexts all go to owner review, not
just a sample.

## Subjects, tags and duplicates

- A **subject** is a distinct thing to look at. A **context** is a tag. A
  5-point star is one subject tagged `christmas`, `july-4th`, `space`. The
  Star of David is a different subject. So are a Chinese paper lantern and a
  fanous. A plain pumpkin and a carved jack-o'-lantern are different subjects
  (the face is the point).
- Tags come from a controlled list (`tags.toml`): themes (`ocean`, `farm`),
  holidays (`thanksgiving`, `diwali`), traditions (`jewish`, `hindu`).
- Which difficulties a subject is offered at is **computed by the checks**,
  not tagged by hand. Surprise me at Easy picks only from subjects that pass
  at Easy.
- A lint catches duplicates: overlapping names or judge keywords, and
  masters whose masks overlap too closely.

## Layout

One set of files per subject, so batch PRs don't conflict:

```
library/
  tags.toml                  controlled tag vocabulary
  judge.json                 judge verdicts, keyed by mask hash (committed cache)
  subjects/<id>.toml         name, tags, judge keywords, notes
  subjects/<id>.svg          source drawing (owner: SVG)
  subjects/<id>.png          master bitmap, built from the SVG; canonical, committed
  subjects/<id>.easy.txt     optional override, ASCII pixel art at the exact grid
  subjects/<id>.medium.txt   optional override
  subjects/<id>.medium.a4.txt  optional paper-specific override (wins on that paper)
  build.py                   rasterize, check, judge, package, report
```

`<id>` is a stable slug (`turkey`, `star-5`, `lantern-paper`).

**Masters are drawn as SVG** (owner's call), black on white. `build.py`
rasterizes the SVG, crops to the subject, makes it touch one edge (the
`edge` in its TOML, bottom by default; the entrance goes there) with a fixed
margin on the other sides (DECISIONS 18), and downsamples to the 1-bit master
at 2× the Hard grid. The master PNG is committed: it is what ships, and it
must not drift when `rsvg-convert` changes. `build.py --remaster <id>`
regenerates it deliberately.

**Thanksgiving subjects also go into `shapes/`** (owner) as ready-to-print
prototype inputs: `shapes/<id>.png` (the master) and `shapes/<id>.easy.png` /
`.medium.png` (overrides at 1 pixel per cell, which `picmaze.py` uses
unchanged at that preset).

## Sizes

Grids the provisional presets give, automatic orientation, Letter / A4:

| Aspect | Easy | Medium | Hard |
|---|---|---|---|
| 3:4 | 8×11 / 8×11 | 13×17 / 13×17 | 31×41 / 30×40 |
| 1:1 | 8×8 / 8×8 | 13×13 / 13×13 | 31×31 / 30×30 |
| 1:2 | 6×11 / 6×12 | 9×18 / 10×19 | 21×42 / 22×45 |

Easy and Medium usually match across papers, so one override covers both.
Wide subjects are the exception (the pilgrim hat: Medium 18×12 on Letter,
19×13 on A4); they get paper-specific overrides.
`build.py` lists the exact target grids per subject and flags overrides that
no longer match one (e.g. after #8). A forced orientation (Manual) falls back
to the downsampled master.

The master is about 2× Hard (≈ 62×82 at 3:4).

## Checks

Run on exactly what the app will do: master bitmap → Lanczos to the grid →
threshold. For each preset × {Letter, A4}:

1. Preflight metrics: pieces, holes surviving vs the master, thin parts,
   coverage, edge contact (the master is pre-cropped per DECISIONS 18, so
   app preflight is a no-op).
2. Labyrinth construction succeeds and the invariants hold.
3. Blind judge (`experiments/judge.py`, prompt widened from "everyday thing"
   to "thing or symbol"), cached by mask hash. A pass is an answer that
   contains one of the subject's keywords.

Tests (Hypothesis where it fits): the package round-trips every mask
bit-exactly; every shipped subject passes at each difficulty it is offered
at; IDs never disappear and a shipped mask never changes without a revision
bump; tags are from the vocabulary.

## Review

An artifact page with one card per subject: master, the three masks, the
judge's answer, and ✓ / ✗ / comment buttons stored on the page. Claude reads
the verdicts back. The judge was lenient on blocky pixel art in the bake-off,
so owner review carries most weight at Easy.

## Package

One versioned file: per subject its ID, tags, master size and bits (1 bit per
cell, deflate), and overrides. Exact format settled by the pilot's
measurements. Puzzle links carry the ID; the generator version in the link
(DECISIONS 17) pins the library version. The Rust core needs two small
additions (decode the package; use an override when the grid matches),
filed as a `core` issue once the format is settled.

## Pilot

About 58 subjects in four themes. Thanksgiving goes first so its pictures can
be printed with the prototype before 26 November. Provisional list:

- **Thanksgiving:** turkey, pumpkin, pie, cornucopia, acorn, maple leaf,
  corn cob, pilgrim hat, apple.
- **Halloween:** jack-o'-lantern, ghost, skull, witch, cat (also `pets`),
  bat, spider, cauldron, haunted house, owl, candy.
- **Ocean:** fish, turtle, octopus, whale, crab, starfish, seahorse, shark,
  jellyfish, shell, sailboat, anchor, lighthouse.
- **Holidays and celebrations:** Christmas tree, 5-point star (Christmas,
  July 4th), bell, stocking, candy cane, snowman, Easter egg, rabbit (Easter,
  animals), chick, hanukkiah, dreidel, Star of David, crescent moon (Ramadan,
  Eid, night sky), fanous lantern, mosque, diya, lotus, kite (Makar
  Sankranti, general), paper lantern (Lunar New Year, Mid-Autumn), dragon
  boat, koinobori, heart (Valentine's), shamrock, crown (King's Day,
  Epiphany), pretzel.

The list deliberately mixes shapes whose outline carries them at Easy (heart,
bell, crescent, egg) with detailed ones (hanukkiah, spider, dragon boat), and
includes the duplicate cases above.

**The pilot measures:** pass rate per difficulty and paper (judge and owner);
how many subjects need an Easy or Medium override; drawing effort per
subject; bytes per subject; judge–owner agreement. The outcome is a
DECISIONS.md row and a go/no-go for ~1,000 subjects (after #8).

**Spike first:** `build.py` and its tests, and three subjects (an
easy outline, a detailed one, one with a hole) through draw → check → review,
before the full pilot.
