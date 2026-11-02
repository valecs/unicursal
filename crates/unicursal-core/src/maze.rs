//! Construction on the fine (corridor) grid: Hamiltonian cycles around random
//! spanning trees, labyrinths (one open corridor + sealed decoy loops) and
//! branching mazes (Okamoto & Uehara, CCCG 2009).

use crate::mask::{Mask, NBRS, components, nbrs};
use crate::rng::Pcg32;

pub type Cell = (usize, usize);

/// Fine grid of `rows × cols` corridor cells and the passages between them.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Grid {
    pub rows: usize,
    pub cols: usize,
    /// `hp[r * (cols - 1) + c]`: passage (r, c)–(r, c + 1).
    pub hp: Vec<bool>,
    /// `vp[r * cols + c]`: passage (r, c)–(r + 1, c).
    pub vp: Vec<bool>,
}

impl Grid {
    pub fn new(rows: usize, cols: usize) -> Self {
        Grid {
            rows,
            cols,
            hp: vec![false; rows * cols.saturating_sub(1)],
            vp: vec![false; rows.saturating_sub(1) * cols],
        }
    }

    #[inline]
    pub fn h(&self, r: usize, c: usize) -> bool {
        self.hp[r * (self.cols - 1) + c]
    }

    #[inline]
    pub fn v(&self, r: usize, c: usize) -> bool {
        self.vp[r * self.cols + c]
    }

    fn slot(&mut self, a: Cell, b: Cell) -> &mut bool {
        let (p, q) = if a <= b { (a, b) } else { (b, a) };
        if p.0 == q.0 {
            debug_assert_eq!(q.1, p.1 + 1);
            &mut self.hp[p.0 * (self.cols - 1) + p.1]
        } else {
            debug_assert_eq!((q.0, q.1), (p.0 + 1, p.1));
            &mut self.vp[p.0 * self.cols + p.1]
        }
    }

    pub fn set(&mut self, a: Cell, b: Cell, open: bool) {
        *self.slot(a, b) = open;
    }

    pub fn get(&self, a: Cell, b: Cell) -> bool {
        let (p, q) = if a <= b { (a, b) } else { (b, a) };
        if p.0 == q.0 {
            self.h(p.0, p.1)
        } else {
            self.v(p.0, p.1)
        }
    }

    /// Open neighbours in the order E, W, S, N (the prototype's order).
    pub fn open_nbrs(&self, (r, c): Cell) -> impl Iterator<Item = Cell> + '_ {
        let e = (c + 1 < self.cols && self.h(r, c)).then_some((r, c + 1));
        let w = (c > 0 && self.h(r, c - 1)).then(|| (r, c - 1));
        let s = (r + 1 < self.rows && self.v(r, c)).then_some((r + 1, c));
        let n = (r > 0 && self.v(r - 1, c)).then(|| (r - 1, c));
        [e, w, s, n].into_iter().flatten()
    }

    pub fn degree(&self, p: Cell) -> usize {
        self.open_nbrs(p).count()
    }

    pub fn n_passages(&self) -> usize {
        self.hp.iter().chain(&self.vp).filter(|&&b| b).count()
    }
}

/// Side of the frame the entrance is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    N,
    S,
    E,
    W,
}

impl Side {
    /// Outward unit vector (x, y), y down.
    pub fn out(self) -> (f64, f64) {
        match self {
            Side::N => (0.0, -1.0),
            Side::S => (0.0, 1.0),
            Side::W => (-1.0, 0.0),
            Side::E => (1.0, 0.0),
        }
    }
}

/// The two ends of the solution (adjacent cells whose passage was cut), and
/// the frame side they open onto, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Opening {
    pub entrance: Cell,
    pub exit: Cell,
    pub side: Option<Side>,
}

/// A constructed puzzle on the fine grid.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Built {
    pub grid: Grid,
    /// The picture, doubled to the fine grid: exactly the solution's cells.
    pub fine_fg: Mask,
    pub opening: Opening,
    /// Notes for the user (e.g. how many sealed loops).
    pub notes: Vec<String>,
}

impl Built {
    /// The solution: cells along the open corridor from the entrance.
    pub fn solution(&self) -> Vec<Cell> {
        trace_path(&self.grid, self.opening.entrance, &self.fine_fg)
    }
}

/// Uniform random spanning tree (Wilson's algorithm) of the grid graph induced
/// by `cells`, which must be in raster order. Returns its edges.
pub fn wilson_tree(cells: &[Cell], rows: usize, cols: usize, rng: &mut Pcg32) -> Vec<(Cell, Cell)> {
    // index of each cell in `cells`, or NONE
    const NONE: usize = usize::MAX;
    let mut index = vec![NONE; rows * cols];
    for (i, &(r, c)) in cells.iter().enumerate() {
        index[r * cols + c] = i;
    }
    let nbr: Vec<Vec<usize>> = cells
        .iter()
        .map(|&(r, c)| {
            nbrs(rows, cols, r, c)
                .map(|(nr, nc)| index[nr * cols + nc])
                .filter(|&j| j != NONE)
                .collect()
        })
        .collect();
    let mut order: Vec<usize> = (0..cells.len()).collect();
    rng.shuffle(&mut order);
    let mut in_tree = vec![false; cells.len()];
    let mut next = vec![NONE; cells.len()];
    let mut edges = Vec::with_capacity(cells.len().saturating_sub(1));
    let Some(&root) = order.first() else {
        return edges;
    };
    in_tree[root] = true;
    for &start in &order[1..] {
        // loop-erased random walk: remembering only the last exit from each
        // cell erases loops
        let mut u = start;
        while !in_tree[u] {
            let v = *rng.choose(&nbr[u]);
            next[u] = v;
            u = v;
        }
        let mut u = start;
        while !in_tree[u] {
            in_tree[u] = true;
            edges.push((cells[u], cells[next[u]]));
            u = next[u];
        }
    }
    edges
}

/// Carve a Hamiltonian cycle through the 2×2 refinement of a connected region
/// of picture cells (raster order) into `grid`: the boundary of a thickened
/// random spanning tree.
pub fn hamiltonian_cycle(region: &[Cell], rng: &mut Pcg32, grid: &mut Grid) {
    for &(i, j) in region {
        let (tl, tr, bl, br) = (
            (2 * i, 2 * j),
            (2 * i, 2 * j + 1),
            (2 * i + 1, 2 * j),
            (2 * i + 1, 2 * j + 1),
        );
        for (a, b) in [(tl, tr), (tr, br), (br, bl), (bl, tl)] {
            grid.set(a, b, true);
        }
    }
    let (rows, cols) = (grid.rows / 2, grid.cols / 2);
    for (a, b) in wilson_tree(region, rows, cols, rng) {
        let (i, j) = a.min(b);
        if a.0 == b.0 {
            // horizontal neighbours (i, j) | (i, j+1)
            grid.set((2 * i, 2 * j + 1), (2 * i + 1, 2 * j + 1), false);
            grid.set((2 * i, 2 * j + 2), (2 * i + 1, 2 * j + 2), false);
            grid.set((2 * i, 2 * j + 1), (2 * i, 2 * j + 2), true);
            grid.set((2 * i + 1, 2 * j + 1), (2 * i + 1, 2 * j + 2), true);
        } else {
            grid.set((2 * i + 1, 2 * j), (2 * i + 1, 2 * j + 1), false);
            grid.set((2 * i + 2, 2 * j), (2 * i + 2, 2 * j + 1), false);
            grid.set((2 * i + 1, 2 * j), (2 * i + 2, 2 * j), true);
            grid.set((2 * i + 1, 2 * j + 1), (2 * i + 2, 2 * j + 1), true);
        }
    }
}

/// Choose a cycle passage to cut so the path ends sit on the outer wall if
/// possible; otherwise cut any foreground passage (entrance inside).
pub fn pick_opening(grid: &Grid, fine_fg: &Mask, rng: &mut Pcg32) -> Opening {
    let (h, w) = (grid.rows, grid.cols);
    let mut cands = Vec::new();
    for c in 0..w.saturating_sub(1) {
        for (r, side) in [(0, Side::N), (h - 1, Side::S)] {
            if fine_fg.get(r, c) && fine_fg.get(r, c + 1) && grid.h(r, c) {
                cands.push(((r, c), (r, c + 1), side));
            }
        }
    }
    for r in 0..h.saturating_sub(1) {
        for (c, side) in [(0, Side::W), (w - 1, Side::E)] {
            if fine_fg.get(r, c) && fine_fg.get(r + 1, c) && grid.v(r, c) {
                cands.push(((r, c), (r + 1, c), side));
            }
        }
    }
    if !cands.is_empty() {
        let &(a, b, side) = rng.choose(&cands);
        return Opening {
            entrance: a,
            exit: b,
            side: Some(side),
        };
    }
    let mut pts = fine_fg.positions();
    rng.shuffle(&mut pts);
    for p in pts {
        if let Some(q) = grid.open_nbrs(p).find(|&(r, c)| fine_fg.get(r, c)) {
            return Opening {
                entrance: p,
                exit: q,
                side: None,
            };
        }
    }
    unreachable!("a doubled picture always has an internal passage")
}

/// Follow the corridor from `start` through `allowed` cells (degree ≤ 2).
pub fn trace_path(grid: &Grid, start: Cell, allowed: &Mask) -> Vec<Cell> {
    let mut path = vec![start];
    let (mut prev, mut cur) = (None, start);
    while let Some(nxt) = grid
        .open_nbrs(cur)
        .find(|&q| Some(q) != prev && allowed.get(q.0, q.1))
    {
        prev = Some(cur);
        cur = nxt;
        path.push(cur);
    }
    path
}

/// Labyrinth: the picture is one open corridor (entrance → exit); each
/// background region is a sealed loop. No junctions anywhere.
pub fn build_labyrinth(fg: &Mask, rng: &mut Pcg32) -> Built {
    let mut grid = Grid::new(2 * fg.rows, 2 * fg.cols);
    let fine_fg = fg.doubled();
    hamiltonian_cycle(&fg.positions(), rng, &mut grid);
    let (lab, k) = components(&fg.not());
    for t in 1..=k {
        let region: Vec<Cell> = (0..lab.len())
            .filter(|&i| lab[i] == t)
            .map(|i| (i / fg.cols, i % fg.cols))
            .collect();
        hamiltonian_cycle(&region, rng, &mut grid);
    }
    let opening = pick_opening(&grid, &fine_fg, rng);
    grid.set(opening.entrance, opening.exit, false);
    let notes = if k > 1 {
        vec![format!("background splits into {k} sealed loops")]
    } else {
        vec![]
    };
    Built {
        grid,
        fine_fg,
        opening,
        notes,
    }
}

/// Branching maze (Okamoto & Uehara): a Hamiltonian path through the picture,
/// then a random spanning tree of everything else that keeps that path as the
/// unique solution. `reduce_deadends` applies their dead-end pairing.
pub fn build_maze(fg: &Mask, rng: &mut Pcg32, reduce_deadends: bool) -> Built {
    let (h, w) = (2 * fg.rows, 2 * fg.cols);
    let mut grid = Grid::new(h, w);
    let fine_fg = fg.doubled();
    hamiltonian_cycle(&fg.positions(), rng, &mut grid);
    let opening = pick_opening(&grid, &fine_fg, rng);
    grid.set(opening.entrance, opening.exit, false);

    let mut uf = UnionFind::new(h * w);
    let idx = |(r, c): Cell| r * w + c;
    for r in 0..h {
        for c in 0..w {
            for q in grid.open_nbrs((r, c)) {
                uf.union(idx((r, c)), idx(q));
            }
        }
    }
    // randomized Kruskal over every passage not internal to the picture
    let mut edges = Vec::new();
    for r in 0..h {
        for c in 0..w {
            for q in [(r, c + 1), (r + 1, c)] {
                if q.0 < h && q.1 < w && !(fine_fg.get(r, c) && fine_fg.get(q.0, q.1)) {
                    edges.push(((r, c), q));
                }
            }
        }
    }
    rng.shuffle(&mut edges);
    for (p, q) in edges {
        if uf.union(idx(p), idx(q)) {
            grid.set(p, q, true);
        }
    }
    if reduce_deadends {
        reduce_deadends_pass(&mut grid, &opening, rng, &fine_fg);
    }
    Built {
        grid,
        fine_fg,
        opening,
        notes: vec![],
    }
}

/// Paper §3.3: while two dead ends u, v are adjacent, cut u's branch off at
/// its junction and join it to v instead. Keeps a tree, so the solution is
/// unchanged, and the count of dead ends drops by one.
///
/// A branch ends where it meets a junction *or the solution path*: path cells
/// look like ordinary corridor (degree 2), so without that stop the walk would
/// run onto the path and cut it (#7).
fn reduce_deadends_pass(grid: &mut Grid, opening: &Opening, rng: &mut Pcg32, fine_fg: &Mask) {
    let (h, w) = (grid.rows, grid.cols);
    let protected = |p: Cell| p == opening.entrance || p == opening.exit;
    let fg = |p: Cell| fine_fg.get(p.0, p.1);
    let mut changed = true;
    while changed {
        changed = false;
        let mut cells: Vec<Cell> = (0..h).flat_map(|r| (0..w).map(move |c| (r, c))).collect();
        rng.shuffle(&mut cells);
        for u in cells {
            if protected(u) || grid.degree(u) != 1 || fg(u) {
                continue;
            }
            for (dr, dc) in NBRS {
                let Some(v) = u.0.checked_add_signed(dr).zip(u.1.checked_add_signed(dc)) else {
                    continue;
                };
                if v.0 >= h || v.1 >= w {
                    continue;
                }
                if protected(v) || grid.degree(v) != 1 || fg(v) || grid.get(u, v) {
                    continue;
                }
                // walk from u to its first junction (or the solution)
                let (mut prev, mut cur) = (None, u);
                loop {
                    let deg = grid.degree(cur);
                    let nx = grid.open_nbrs(cur).find(|&q| Some(q) != prev);
                    match nx {
                        Some(n) if deg < 3 && !fg(cur) => {
                            prev = Some(cur);
                            cur = n;
                        }
                        _ => break,
                    }
                }
                let attach = grid.degree(cur) >= 3 || fg(cur);
                let Some(p) = prev else { continue };
                if !attach || v == cur {
                    continue; // whole thing is one corridor; leave it
                }
                grid.set(p, cur, false); // wall off u's branch at its junction
                grid.set(u, v, true); // and splice it onto v
                changed = true;
                break;
            }
        }
    }
}

struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    /// Join the sets of a and b; false if already joined.
    fn union(&mut self, a: usize, b: usize) -> bool {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return false;
        }
        self.parent[ra] = rb;
        true
    }
}
