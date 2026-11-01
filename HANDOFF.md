# unicursal — picture-labyrinth generator: handoff for the app build

A prototype (Python) that turns a silhouette into a printable maze whose
solution, once filled in, reveals the picture. This document records what the
prototype does, every place a human (me, by eye) had to make a judgement call,
how much of that can be automated, and what to expose to app users.

Everything here was built and tested in one session; the examples in
`examples/` are the actual outputs.

---

## 1. What's in this folder

| File | What it is |
|---|---|
| `picmaze.py` | Core: image → mask, maze/labyrinth construction, grid & curvy renderers, paper sizing, CLI |
| `flow.py` | "Flow" style: relaxes the labyrinth's corridors into organic curves; walls, width measurement, leak check |
| `tests/test_invariants.py` | The structural guarantees a port must keep (18 tests, ~2 s) |
| `shapes/*.py` | The five silhouettes drawn this session (pumpkin, skull, cat, ghost, witch) + their PNGs |
| `examples/*.pdf` | Puzzle + answer key for each |
| `requirements.txt` | numpy, Pillow, scipy (+ `rsvg-convert` on PATH for PDF and the leak check) |

Typical run:
```
python3 picmaze.py shapes/ghost.png --flow --corridor 7 --seed 3 --fill "#8fa6d6" --out ghost
python3 picmaze.py shapes/witch.png --flow --cells 60 --seed 19 --flow-tol 0.4 --fill "#5a2d82" --out witch
```

## 2. How the generator works

Terminology. **Picture cell** = one pixel of the downsampled silhouette.
**Corridor cell** = one cell of the maze. Each picture cell becomes a 2×2
block of corridor cells; this doubling is what makes the construction
always possible.

Pipeline:

1. **Mask.** Load image → greyscale → resize to *m × n* picture cells → threshold.
   If the foreground is in several pieces, join them with 1-cell bridges
   (shortest background paths). Background may be in several pieces ("holes").
2. **Construction** (on the 2m × 2n corridor grid).
   For each connected region (the foreground, and each background piece): pick a
   uniform random spanning tree of its picture cells (Wilson's algorithm), and
   trace around the "thickened" tree. That gives a Hamiltonian cycle through all
   of the region's corridor cells.
   - **Labyrinth mode** (the main product): cut the foreground cycle once, at the
     frame if the picture touches it. The result is one open corridor (the
     solution) plus sealed decoy loops. No junctions anywhere.
   - **Maze mode** (Okamoto & Uehara, CCCG 2009): keep the foreground path, then a
     random spanning tree over all other cells (Kruskal), plus their dead-end
     pairing heuristic so background texture matches. Real branching maze, same
     hidden picture.
3. **Styling.**
   - *Grid*: straight walls.
   - *Curvy*: round corners (Chaikin, junctions pinned) + a smooth random warp
     with Lipschitz constant < 1, so it cannot fold or cross.
   - *Flow* (labyrinth only): throw the grid away; each corridor is a polyline
     ("strand"). Relax them as elastic filaments: springs (fixed length), bending
     stiffness, short-range repulsion (keeps corridor width), a weak force holding
     the solution inside the picture and decoys outside, and a little noise.
     Strands keep 85 % of their length, which opens black "voids" and lets them
     meander. Walls are rebuilt as Voronoi ridges between nearby samples of
     different strands, clipped to a tube around each strand.
4. **Output.** SVG, centred on the chosen paper at true scale → PDF. Footer records
   grid size, corridor size, seed.
5. **Verification** (flow): no strand crossings; rasterize the puzzle and count
   open regions, which must equal the number of corridors; measure clear corridor
   width wall-to-wall.

Key invariants (all tested):
- Labyrinth: every cell has exactly 2 openings except the 2 solution ends.
  Edges = cells − 1. Each background piece is one single loop.
- Maze: open passages form a spanning tree.
- Both: the solution path covers exactly the foreground cells.
- Flow: zero crossings; open regions = strands; narrowest clear width > 0.6 corridor spacing.

## 3. Where I used judgement, and how to encode it

This is the main point of the handoff. Roughly in order of how much it mattered.

### 3.1 Drawing a silhouette that survives (the biggest one)

Every picture needed at least one redraw after I looked at the downsampled mask:

| Picture | Problem I saw | Fix |
|---|---|---|
| Pumpkin | Face too small, no margin to frame | Bigger features, recentred |
| Skull | Five 1-cell gaps between teeth would smear shut | Three teeth, four wider gaps |
| Cat | Tail hooked out but didn't close, so no hole; nose only 4 cells | Extended the curl; nose ~10 cells |
| Ghost (smooth drawing) | At 10×14 cells the two eyes merged into one hole; ghost filled ~80 % of frame and read as a rectangle | Pixel art drawn directly on the target grid, at 12×16 with a 2-cell border |
| Witch | Cape was a shapeless blob swallowing the body; arm gap tiny | Redrawn with an arched cape, explicit arm triangle, hat band |

Rules of thumb that emerged (calibrated by eye, not measured; see §6):
- A **hole** should be ≥ ~2×2 picture cells (ghost eyes at 2×2 survived). Gaps
  1 picture cell wide are at risk under flow.
- A **thin foreground** part (1 cell wide) is always valid but renders as one
  doubled-back corridor. It reads as a line, not a shape.
- The subject needs a **background border** of ≥ ~2 picture cells and should
  cover roughly ≤ 60 % of the frame, or its outline doesn't read.
- Something must **touch the frame** for the entrance to be on the edge
  (ghost tail, pumpkin stem, cat base, witch bristles, skull neck). Otherwise the
  start/finish are marked inside with diamonds.
- At small grid sizes, **pixel art on the exact grid** beats any smooth drawing.

**Encodable as an automatic "preflight" on the mask, shown live in the UI:**
- *Hole survival.* For each background piece, compute area and inscribed
  radius (max of the distance transform) in picture cells; flag those below
  threshold.
- *Hole merging.* Count holes at source resolution vs at the target grid. If the
  count drops ("eyes merged"), say so. This catches the 8 mm ghost.
- *Coverage & margin.* Foreground fraction; distance from subject to frame.
  Auto-pad/centre the subject to hit a target coverage.
- *Edge contact.* If the foreground doesn't touch the frame, offer either
  "start inside" or an auto-stalk: a 1-cell bridge to the nearest edge, same code
  as the piece bridging.
- *Thin parts.* Flag 1-cell-wide foreground (readability warning, not an error).
- *Disconnected pieces.* Show where bridges were added. Today's choice
  (shortest path) can be ugly; the "Hi!" bridges all ran to the underline.
- *Auto-size.* Search the largest corridor size (or smallest grid) at which all
  holes still survive. Tell the user which features will be lost at the size they
  picked.

### 3.2 Picking the seed

For every final output I generated 2–3 seeds and picked by eye (pumpkin 31,
ghost 3, witch 19): clearest hem, most visible holes, etc.

**Encodable:** a fidelity score per candidate. Rasterize the answer key's
solution region and compare with the target mask (IoU). Add per-hole visibility
(fraction of each hole's target area left uncovered) and a penalty for
voids clumping. Generate N seeds in the background, rank, and show the best few
as thumbnails.

### 3.3 Picking resolution / size

Early on I chose `--cells` (40, 55, 60) by eye. This is now driven by physical
size: `--corridor` mm + `--paper` → picture cells. On letter paper:

| Corridor spacing | Picture cells (portrait) | Clear width (flow, measured) | For |
|---|---|---|---|
| ~2 mm | 60 on long side | 1.7 mm narrowest | pen |
| 4 mm | up to 22 × 30 | — | sharp pencil |
| 7 mm | up to 12 × 16 | 5.7 mm narrowest, 6.8 typical | markers |
| 8 mm (default) | up to 10 × 14 | 6.5 mm narrowest | markers, crayons |
| 11 mm | up to 7 × 10 | — | small kids |

("Up to": the actual grid follows the image's aspect ratio.)

The tool guide (pencil 4, dull pencil 6, marker 8, small kids 11 mm) is my
guess. **Calibrate with real test prints** before baking it into presets.

Clear width ≈ 82 % of spacing in grid mode. In curvy mode it can drop to
`(1 − warp − 0.16)` × spacing (about 30 % at the default warp 0.55); curvy
mode needs a lower warp for kids.

### 3.4 Flow tuning

Defaults came from failed attempts, all recorded here so nobody repeats them:
- Plain curve smoothing (Laplacian) shrank the strands into a few noodles and
  lost the picture.
- Adding attraction bundled strands into pairs.
- **Fixed-length springs + bending + repulsion** is what works. `length` < 1
  opens voids; 0.85 is the default (0.7 = loopier, 1.0 = packed and still
  grid-like).
- `bend` 0.3; `--flow-tol` (how far the outline may drift, in corridors)
  default 1.0. Lowering it to 0.4 made little visible difference at 60 cells;
  thin features are limited by the mask, not the tolerance.
- 1500 iterations, fixed. No convergence test yet. **Encodable:** stop when mean
  displacement falls below a threshold. Could also auto-tune `length` to hit a
  target void fraction.

### 3.5 Rendering choices

- Answer key in flow mode now fills each corridor wall to wall
  (`0.9 × (tube − wall)`). The earlier thin centre-line made thin parts look
  smeared.
- Solution colour chosen per theme (pumpkin orange, skull purple, cat
  near-black `#262233`, ghost blue `#8fa6d6`, witch `#5a2d82`).
  **Encodable:** take the dominant colour of a colour source image.
- **Voids are solid `#111`: a lot of ink.** Needs an ink-saver option (light
  grey or hatching). Plain white would look like open corridor and confuse
  solvers.

### 3.6 Verification caught real bugs, so make it a hard gate

- Walls stopped just short of cusps where two tubes nearly touch, leaking two
  corridors together. Found by the region count, fixed by overshooting the wall
  clip into the void (+0.15). Also densify strands before the Voronoi.
- The checker itself had a scale bug: rsvg renders 1 pt = 4/3 px.
- **In the app:** never show a puzzle that fails crossings/region-count/min-width.
  Retry silently with another seed.

## 4. What to offer users

**Essential (first screen):**
1. **Picture**: built-in library; import a photo/drawing (with threshold +
   invert + the live preflight preview); draw pixel art on the target grid; text.
2. **Size**: a tool/age preset ("crayon", "marker", "pencil", "pen") that sets
   corridor mm, with paper (Letter / A4). Show the resulting grid size and the
   preflight warnings.
3. **Style**: Flow (default) / Curvy / Grid.
4. **Shuffle**: new layout. Show several ranked candidates; a toggle to peek at
   each one's answer. Seeds stay visible (footer) but aren't something users
   type.
5. **Print / share**: one PDF with puzzle + answer key. Option: answer on page 2,
   a separate file, or small and upside-down on the same page.

**Advanced:**
- Mode: labyrinth (one path, kids) vs maze (branches, real puzzle; grid/curvy only).
- Wiggliness (flow `length` / `bend` presets), colours, ink-saver voids.
- Start on the edge (auto-stalk) vs inside.
- Detail vs ease slider (the corridor mm, directly).

**Hidden** (internal constants): repulsion radius, spring/bend constants,
tube radius 0.55, wall 0.14, smoothing passes, iterations.

**Possible app-only feature:** on-screen solving. In a labyrinth the solution is
one polyline, so finger-tracing is easy: project the touch onto it, advance
while inside the corridor, and the picture colours in as you go.

## 5. Porting notes

**Dependencies to replace:**

| Python | Use | Replacement |
|---|---|---|
| scipy `ndimage.label`, `distance_transform_edt` | components, picture force field, preflight | write them (BFS; Felzenszwalb EDT), both small |
| scipy `cKDTree` | flow repulsion, width measure | uniform grid hash (spacing ≈ 1 corridor): simpler and faster here |
| scipy `Voronoi` (Qhull) | flow walls | Delaunay → Voronoi via a Delaunator port (JS/Kotlin/Rust ports exist) |
| `rsvg-convert` | SVG→PDF, leak-check raster | platform PDF (Android `PdfDocument`, iOS `UIGraphicsPDFRenderer`, browser print) and platform canvas for the raster check |

**Performance** (Python/numpy, 1500 iterations): kid sizes (≤ 16 picture cells)
take 2–3 s; 55–60 cells take 45–60 s. Cost scales with corridor count. A native
core with a grid hash should be one to two orders of magnitude faster, and
the per-iteration force loop parallelises trivially.

**Architecture options** (your call):
- *PWA / web*: TS UI + core in Rust or C++ → WASM. One build for Android, iOS
  and desktop; printing is just the browser. Least distribution friction.
- *Kotlin Multiplatform + Compose*: shared Kotlin core; native PDF on each
  platform. Best fit if Android-native feel is the priority.
- *Native Android* with a Rust/C++ core over JNI: fastest, but the core is only
  reusable if you keep it separate.
- *Server-side*: existing Python behind an API, thin client. Fastest to ship;
  needs hosting and doesn't work offline.

Whichever you choose: the generator core is the subtle part, so keep it in one
portable library and port these tests first.

**Determinism.** Python's `random` and numpy's PRNG won't match another platform.
Specify one PRNG (e.g. PCG32) in the core. Even then the flow simulation is
chaotic: float differences across CPUs/compilers will change results. So a
"puzzle code" (image + params + seed + generator version) only reproduces within
the same build. For saved/shared puzzles, store the generated geometry.

## 6. Known gaps and open questions

- **Start and finish are always adjacent.** The solution is a cut cycle, so its
  ends are neighbours. For more challenge, construct a Hamiltonian *path* with
  distant ends (grid-graph parity conditions apply). This is an open research-y
  item.
- Preflight thresholds (§3.1) are from eyeballing five pictures. Calibrate them
  with the fidelity score + region check over a test set.
- Fidelity scoring/seed ranking (§3.2) is not implemented yet.
- Flow only works on labyrinths (no junctions). Extending it to maze mode means
  relaxing a graph with junction nodes; same forces, more bookkeeping.
- The leak check is raster-based (30 px per corridor). It's fine as a gate, but a
  geometric check (planar arrangement of walls) would be exact.
- Bridge placement for multi-piece pictures is purely shortest-path; it can look
  arbitrary.
- `pick_opening` chooses randomly among edge positions; may want a rule (e.g.
  bottom edge preferred, or user-chosen side).
- No convergence test in flow; fixed 1500 iterations.

## 7. Suggested build order

1. Port the core (mask → construction → labyrinth) with the invariant tests.
2. Grid renderer + PDF at true scale + paper/corridor sizing. That's already a
   usable app.
3. Preflight on the mask with live preview: the biggest quality lever.
4. Flow relaxation + walls + verification gate.
5. Seed ranking, shuffle with candidates.
6. Library, pixel-art editor, photo import; then maze mode, curvy, on-screen
   solving.

References: Okamoto & Uehara, "How to make a picturesque maze" (CCCG 2009);
Wong & Takahashi, "Flow-based automatic generation of hybrid picture mazes"
(2009); Pedersen & Singh, "Organic labyrinths and mazes" (NPAR 2006); Bosch,
*Opt Art* (2019); Wilson's algorithm for uniform spanning trees.
