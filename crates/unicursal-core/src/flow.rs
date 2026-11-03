//! Flow style: the labyrinth's corridors relaxed into organic curves (after
//! Pedersen & Singh, "Organic labyrinths and mazes", 2006), port of `flow.py`.
//!
//! In labyrinth mode every cell has two openings, so the maze is a family of
//! disjoint curves ("strands"): the open solution plus sealed decoy loops.
//! The grid is thrown away; strands relax as elastic filaments (fixed-length
//! springs, bending stiffness, short-range repulsion, a weak force keeping the
//! solution inside the picture and decoys outside, a little noise). Walls are
//! rebuilt afterwards as Voronoi ridges between strands.
//!
//! Determinism (#5, #10): each step computes every sample's force from the
//! previous positions only, summing its terms in a fixed order (neighbours in
//! a fixed bucket order), and the noise for sample `i` at step `t` is a pure
//! function of (seed, t, i). So the result does not depend on how many
//! threads run the loop (feature `parallel`), or on whether any do.

use crate::field::{Field, signed_distance};
use crate::mask::Mask;
use crate::maze::{Cell, Grid};

pub type Pt = [f64; 2];

/// One corridor as a polyline in lattice coordinates (x right, y down; cell
/// (r, c) has its centre at (c + 0.5, r + 0.5)).
#[derive(Clone, Debug, PartialEq)]
pub struct Strand {
    pub pts: Vec<Pt>,
    pub closed: bool,
    pub solution: bool,
}

impl Strand {
    /// Segments as point-index pairs, including the closing one.
    fn n_segments(&self) -> usize {
        if self.closed {
            self.pts.len()
        } else {
            self.pts.len() - 1
        }
    }

    fn segment(&self, k: usize) -> (Pt, Pt) {
        (self.pts[k], self.pts[(k + 1) % self.pts.len()])
    }
}

/// The solution path first, then every sealed loop in raster order of its
/// first cell.
pub fn extract_strands(grid: &Grid, path: &[Cell]) -> Vec<Strand> {
    let centre = |(r, c): Cell| [c as f64 + 0.5, r as f64 + 0.5];
    let (h, w) = (grid.rows, grid.cols);
    let mut seen = vec![false; h * w];
    for &(r, c) in path {
        seen[r * w + c] = true;
    }
    let mut out = vec![Strand {
        pts: path.iter().map(|&p| centre(p)).collect(),
        closed: false,
        solution: true,
    }];
    for r0 in 0..h {
        for c0 in 0..w {
            if seen[r0 * w + c0] {
                continue;
            }
            let start = (r0, c0);
            let mut pts = vec![];
            let (mut prev, mut cur) = (None, start);
            loop {
                pts.push(centre(cur));
                seen[cur.0 * w + cur.1] = true;
                let next = grid
                    .open_nbrs(cur)
                    .find(|&q| Some(q) != prev)
                    .expect("labyrinth cells have two openings");
                if next == start {
                    break;
                }
                prev = Some(cur);
                cur = next;
            }
            out.push(Strand {
                pts,
                closed: true,
                solution: false,
            });
        }
    }
    out
}

fn dist(a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    (dx * dx + dy * dy).sqrt()
}

/// `n` values from `start` to `stop` (`np.linspace`).
fn linspace(start: f64, stop: f64, n: usize, endpoint: bool) -> Vec<f64> {
    let div = if endpoint { n.saturating_sub(1) } else { n };
    let step = if div > 0 {
        (stop - start) / div as f64
    } else {
        0.0
    };
    let mut v: Vec<f64> = (0..n).map(|k| start + k as f64 * step).collect();
    if endpoint && n > 1 {
        v[n - 1] = stop;
    }
    v
}

/// Resample a polyline at `n` points evenly spaced by arc length (endpoints
/// kept for open strands), as `np.interp` on the cumulative length.
fn resample_n(p: &[Pt], closed: bool, n_of: impl Fn(f64) -> usize) -> Vec<Pt> {
    let mut q = p.to_vec();
    if closed {
        q.push(p[0]);
    }
    let mut d = vec![0.0];
    for k in 1..q.len() {
        d.push(d[k - 1] + dist(q[k - 1], q[k]));
    }
    let total = *d.last().unwrap();
    let n = n_of(total);
    let t = linspace(0.0, total, n + usize::from(!closed), !closed);
    let mut j = 0;
    t.iter()
        .map(|&t| {
            while j + 2 < d.len() && d[j + 1] < t {
                j += 1;
            }
            let span = d[j + 1] - d[j];
            let f = if span > 0.0 {
                ((t - d[j]) / span).clamp(0.0, 1.0)
            } else {
                0.0
            };
            [
                q[j][0] + (q[j + 1][0] - q[j][0]) * f,
                q[j][1] + (q[j + 1][1] - q[j][1]) * f,
            ]
        })
        .collect()
}

/// Uniform spacing `seg` (at least 3 samples).
fn resample_uniform(p: &[Pt], closed: bool, seg: f64) -> Vec<Pt> {
    resample_n(p, closed, |len| {
        ((len / seg).round_ties_even() as usize).max(3)
    })
}

/// Spacing at most `h` (at least 3 samples).
pub fn densify(p: &[Pt], closed: bool, h: f64) -> Vec<Pt> {
    resample_n(p, closed, |len| ((len / h).ceil() as usize).max(3))
}

/// Relaxation settings. Defaults are the prototype's `relax()` defaults; the
/// generator sets iters, length, bend, tol, pic, noise and seed from Params.
#[derive(Clone, Debug, PartialEq)]
pub struct FlowParams {
    pub iters: u32,
    /// Repulsion radius (corridor spacing units).
    pub r: f64,
    /// Sample spacing.
    pub seg: f64,
    /// Rest length factor reached at 70 % of the iterations (<1 opens voids).
    pub length: f64,
    pub spring: f64,
    pub bend: f64,
    pub rep: f64,
    /// Strength of the picture force.
    pub pic: f64,
    /// How far (in corridors) the picture's edge may drift.
    pub tol: f64,
    pub noise: f64,
    /// Maximum displacement per iteration.
    pub step: f64,
    pub seed: u64,
}

impl Default for FlowParams {
    fn default() -> Self {
        FlowParams {
            iters: 600,
            r: 1.0,
            seg: 0.5,
            length: 1.0,
            spring: 0.5,
            bend: 0.15,
            rep: 0.5,
            pic: 0.3,
            tol: 1.0,
            noise: 0.01,
            step: 0.04,
            seed: 0,
        }
    }
}

/// A steppable, deterministic relaxation (the app can show progress and stop
/// between steps).
pub struct Relaxation {
    p: Vec<Pt>,
    prv: Vec<usize>,
    nxt: Vec<usize>,
    sid: Vec<u32>,
    loc: Vec<u32>,
    nlen: Vec<u32>,
    closed: Vec<bool>,
    solution: Vec<bool>,
    endpt: Vec<bool>,
    offsets: Vec<usize>,
    strand_closed: Vec<bool>,
    strand_solution: Vec<bool>,
    phi: Field,
    gx: Field,
    gy: Field,
    rows: usize,
    cols: usize,
    params: FlowParams,
    it: u32,
}

impl Relaxation {
    pub fn new(strands: &[Strand], fine_fg: &Mask, params: FlowParams) -> Self {
        let phi = signed_distance(fine_fg);
        let (gy, gx) = phi.gradient();
        let mut r = Relaxation {
            p: vec![],
            prv: vec![],
            nxt: vec![],
            sid: vec![],
            loc: vec![],
            nlen: vec![],
            closed: vec![],
            solution: vec![],
            endpt: vec![],
            offsets: vec![0],
            strand_closed: vec![],
            strand_solution: vec![],
            phi,
            gx,
            gy,
            rows: fine_fg.rows,
            cols: fine_fg.cols,
            params,
            it: 0,
        };
        for (k, s) in strands.iter().enumerate() {
            let pts = resample_uniform(&s.pts, s.closed, r.params.seg);
            let (n, base) = (pts.len(), r.p.len());
            for (i, q) in pts.into_iter().enumerate() {
                r.p.push(q);
                r.sid.push(k as u32);
                r.loc.push(i as u32);
                r.nlen.push(n as u32);
                r.closed.push(s.closed);
                r.solution.push(s.solution);
                let (first, last) = (base, base + n - 1);
                let g = base + i;
                r.prv.push(if i > 0 {
                    g - 1
                } else if s.closed {
                    last
                } else {
                    first
                });
                r.nxt.push(if i + 1 < n {
                    g + 1
                } else if s.closed {
                    first
                } else {
                    last
                });
                r.endpt.push(!s.closed && (i == 0 || i == n - 1));
            }
            r.offsets.push(r.p.len());
            r.strand_closed.push(s.closed);
            r.strand_solution.push(s.solution);
        }
        r
    }

    pub fn iteration(&self) -> u32 {
        self.it
    }

    pub fn is_done(&self) -> bool {
        self.it >= self.params.iters
    }

    /// Fraction of the iterations done, 0..=1.
    pub fn progress(&self) -> f64 {
        if self.params.iters == 0 {
            1.0
        } else {
            self.it as f64 / self.params.iters as f64
        }
    }

    /// Run all remaining iterations.
    pub fn run(&mut self) {
        while !self.is_done() {
            self.step();
        }
    }

    /// The strands at their current positions.
    pub fn strands(&self) -> Vec<Strand> {
        (0..self.strand_closed.len())
            .map(|k| Strand {
                pts: self.p[self.offsets[k]..self.offsets[k + 1]].to_vec(),
                closed: self.strand_closed[k],
                solution: self.strand_solution[k],
            })
            .collect()
    }

    /// One iteration: every sample's displacement from the current positions.
    pub fn step(&mut self) {
        let pr = &self.params;
        let ramp = (self.it as f64 / (0.7 * pr.iters as f64)).min(1.0);
        let rest = pr.seg * (1.0 + (pr.length - 1.0) * ramp);
        let n = self.p.len();
        // discrete Laplacian per sample (zero at open ends)
        let lp: Vec<Pt> = (0..n)
            .map(|i| {
                if self.endpt[i] {
                    [0.0, 0.0]
                } else {
                    let (a, b, c) = (self.p[self.prv[i]], self.p[self.nxt[i]], self.p[i]);
                    [0.5 * (a[0] + b[0]) - c[0], 0.5 * (a[1] + b[1]) - c[1]]
                }
            })
            .collect();
        let hash = SpatialHash::new(&self.p, pr.r, self.cols as f64, self.rows as f64);
        let ctx = StepCtx {
            r: self,
            lp: &lp,
            hash: &hash,
            rest,
        };
        let moves: Vec<Pt> = map_indices(n, |i| ctx.displacement(i));
        let (w, h) = (self.cols as f64, self.rows as f64);
        for (p, m) in self.p.iter_mut().zip(moves) {
            p[0] = (p[0] + m[0]).clamp(0.5, w - 0.5);
            p[1] = (p[1] + m[1]).clamp(0.5, h - 0.5);
        }
        self.it += 1;
    }
}

struct StepCtx<'a> {
    r: &'a Relaxation,
    lp: &'a [Pt],
    hash: &'a SpatialHash,
    rest: f64,
}

impl StepCtx<'_> {
    fn spring(&self, a: usize, b: usize) -> Pt {
        let (p, q) = (self.r.p[a], self.r.p[b]);
        let e = [q[0] - p[0], q[1] - p[1]];
        let len = (e[0] * e[0] + e[1] * e[1]).sqrt() + 1e-12;
        let k = self.r.params.spring * (len - self.rest) / len;
        [k * e[0], k * e[1]]
    }

    fn displacement(&self, i: usize) -> Pt {
        let r = self.r;
        let pr = &r.params;
        let pi = r.p[i];
        let mut f = [0.0, 0.0];
        // springs: segment (i, nxt) pulls i forward, (prv, i) pulls it back
        if r.nxt[i] != i {
            let s = self.spring(i, r.nxt[i]);
            f = [f[0] + s[0], f[1] + s[1]];
        }
        if r.prv[i] != i {
            let s = self.spring(r.prv[i], i);
            f = [f[0] - s[0], f[1] - s[1]];
        }
        // bending
        let (a, b, c) = (self.lp[r.prv[i]], self.lp[r.nxt[i]], self.lp[i]);
        f[0] -= pr.bend * (0.5 * (a[0] + b[0]) - c[0]);
        f[1] -= pr.bend * (0.5 * (a[1] + b[1]) - c[1]);
        // repulsion from samples within r, except near neighbours on the strand
        let r2 = pr.r * pr.r;
        self.hash.for_each_near(pi, |j| {
            if j == i {
                return;
            }
            let pj = r.p[j];
            let d = [pi[0] - pj[0], pi[1] - pj[1]];
            let dd = d[0] * d[0] + d[1] * d[1];
            if dd > r2 {
                return;
            }
            if r.sid[i] == r.sid[j] {
                let mut di = r.loc[i].abs_diff(r.loc[j]);
                if r.closed[i] {
                    di = di.min(r.nlen[i] - di);
                }
                if di <= 2 {
                    return;
                }
            }
            let dist = dd.sqrt() + 1e-9;
            let k = pr.rep * (pr.r - dist) / pr.r / dist;
            f[0] += k * d[0];
            f[1] += k * d[1];
        });
        // picture: solution back inside, decoys back outside
        let ph = r.phi.at_xy(pi[0], pi[1]);
        let mut g = [r.gx.at_xy(pi[0], pi[1]), r.gy.at_xy(pi[0], pi[1])];
        let gl = (g[0] * g[0] + g[1] * g[1]).sqrt() + 1e-9;
        g = [g[0] / gl, g[1] / gl];
        let (push, sign) = if r.solution[i] {
            ((-pr.tol - ph).max(0.0), 1.0)
        } else {
            ((ph - pr.tol).max(0.0), -1.0)
        };
        f[0] += pr.pic * push * sign * g[0];
        f[1] += pr.pic * push * sign * g[1];
        // noise
        let (n0, n1) = gauss_pair(pr.seed, r.it, i as u32);
        f[0] += pr.noise * n0;
        f[1] += pr.noise * n1;
        // cap the step; open ends stay put
        if r.endpt[i] {
            return [0.0, 0.0];
        }
        let m = (f[0] * f[0] + f[1] * f[1]).sqrt();
        let k = (pr.step / (m + 1e-12)).min(1.0);
        [f[0] * k, f[1] * k]
    }
}

/// `(0..n).map(f).collect()`, in parallel with the `parallel` feature. Each
/// element depends only on its index, so the result is the same either way.
pub(crate) fn map_indices<T: Send>(n: usize, f: impl Fn(usize) -> T + Sync + Send) -> Vec<T> {
    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;
        (0..n).into_par_iter().map(f).collect()
    }
    #[cfg(not(feature = "parallel"))]
    {
        (0..n).map(f).collect()
    }
}

fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Two independent standard normals for sample `i` at iteration `it`
/// (SplitMix64 counter hash + Box–Muller, all via libm). A pure function, so
/// noise doesn't depend on evaluation order.
pub fn gauss_pair(seed: u64, it: u32, i: u32) -> (f64, f64) {
    let key = splitmix64(seed ^ splitmix64(((it as u64) << 32) | i as u64));
    let a = splitmix64(key);
    let b = splitmix64(a);
    let scale = 1.0 / (1u64 << 53) as f64;
    let u1 = ((a >> 11) + 1) as f64 * scale; // (0, 1]
    let u2 = (b >> 11) as f64 * scale; // [0, 1)
    let rad = (-2.0 * libm::log(u1)).sqrt();
    let th = 2.0 * std::f64::consts::PI * u2;
    (rad * libm::cos(th), rad * libm::sin(th))
}

/// Points bucketed on a uniform grid of `cell`-sized squares, for fixed-order
/// neighbour queries within `cell` of a point.
pub struct SpatialHash {
    cell: f64,
    nx: usize,
    ny: usize,
    start: Vec<usize>,
    items: Vec<usize>,
}

impl SpatialHash {
    pub fn new(pts: &[Pt], cell: f64, width: f64, height: f64) -> Self {
        let nx = ((width / cell).ceil() as usize).max(1);
        let ny = ((height / cell).ceil() as usize).max(1);
        let bucket = |p: Pt| -> usize {
            let bx = ((p[0] / cell).floor().max(0.0) as usize).min(nx - 1);
            let by = ((p[1] / cell).floor().max(0.0) as usize).min(ny - 1);
            by * nx + bx
        };
        let mut count = vec![0usize; nx * ny + 1];
        for &p in pts {
            count[bucket(p) + 1] += 1;
        }
        for k in 1..count.len() {
            count[k] += count[k - 1];
        }
        let start = count.clone();
        let mut fill = count;
        let mut items = vec![0; pts.len()];
        for (i, &p) in pts.iter().enumerate() {
            let b = bucket(p);
            items[fill[b]] = i;
            fill[b] += 1;
        }
        SpatialHash {
            cell,
            nx,
            ny,
            start,
            items,
        }
    }

    /// Visit every point in the 3×3 buckets around `p` (a superset of those
    /// within `cell`), bucket by bucket, in index order within each.
    pub fn for_each_near(&self, p: Pt, mut f: impl FnMut(usize)) {
        let bx = (p[0] / self.cell).floor() as isize;
        let by = (p[1] / self.cell).floor() as isize;
        for y in by - 1..=by + 1 {
            if y < 0 || y as usize >= self.ny {
                continue;
            }
            for x in bx - 1..=bx + 1 {
                if x < 0 || x as usize >= self.nx {
                    continue;
                }
                let b = y as usize * self.nx + x as usize;
                for &i in &self.items[self.start[b]..self.start[b + 1]] {
                    f(i);
                }
            }
        }
    }
}

/// Strict crossings between non-adjacent segments of all strands.
pub fn count_crossings(strands: &[Strand]) -> usize {
    struct Seg {
        a: Pt,
        b: Pt,
        sid: usize,
        loc: usize,
        n: usize,
        closed: bool,
    }
    let mut segs = vec![];
    for (k, s) in strands.iter().enumerate() {
        let n = s.n_segments();
        for i in 0..n {
            let (a, b) = s.segment(i);
            segs.push(Seg {
                a,
                b,
                sid: k,
                loc: i,
                n,
                closed: s.closed,
            });
        }
    }
    if segs.is_empty() {
        return 0;
    }
    let longest = segs.iter().map(|s| dist(s.a, s.b)).fold(0.0, f64::max) + 1e-6;
    let mids: Vec<Pt> = segs
        .iter()
        .map(|s| [0.5 * (s.a[0] + s.b[0]), 0.5 * (s.a[1] + s.b[1])])
        .collect();
    let (w, h) = mids
        .iter()
        .fold((0.0f64, 0.0f64), |(w, h), m| (w.max(m[0]), h.max(m[1])));
    let hash = SpatialHash::new(&mids, longest, w + longest, h + longest);
    let orient = |p: Pt, q: Pt, r: Pt| -> f64 {
        let v = (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
        if v > 0.0 {
            1.0
        } else if v < 0.0 {
            -1.0
        } else {
            0.0
        }
    };
    let mut count = 0;
    for (i, s) in segs.iter().enumerate() {
        hash.for_each_near(mids[i], |j| {
            if j <= i {
                return;
            }
            let t = &segs[j];
            if s.sid == t.sid {
                let mut di = s.loc.abs_diff(t.loc);
                if s.closed {
                    di = di.min(s.n - di);
                }
                if di <= 1 {
                    return;
                }
            }
            if orient(s.a, s.b, t.a) * orient(s.a, s.b, t.b) < 0.0
                && orient(t.a, t.b, s.a) * orient(t.a, t.b, s.b) < 0.0
            {
                count += 1;
            }
        });
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linspace_matches_numpy() {
        assert_eq!(linspace(0.0, 1.0, 5, true), [0.0, 0.25, 0.5, 0.75, 1.0]);
        assert_eq!(linspace(0.0, 1.0, 4, false), [0.0, 0.25, 0.5, 0.75]);
    }

    #[test]
    fn uniform_resampling_keeps_ends_and_spacing() {
        let p = vec![[0.0, 0.0], [2.0, 0.0], [2.0, 1.0]];
        let q = resample_uniform(&p, false, 0.5);
        assert_eq!(q.len(), 7);
        assert_eq!(q[0], [0.0, 0.0]);
        assert_eq!(q[6], [2.0, 1.0]);
        for k in 1..q.len() {
            assert!((dist(q[k - 1], q[k]) - 0.5).abs() < 1e-12);
        }
        let sq = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let c = resample_uniform(&sq, true, 0.5);
        assert_eq!(c.len(), 8);
        assert_eq!(c[2], [1.0, 0.0]);
    }

    #[test]
    fn gaussian_noise_is_standard_normal() {
        let (mut s, mut s2, n) = (0.0, 0.0, 20000);
        for i in 0..n / 2 {
            let (a, b) = gauss_pair(7, 3, i);
            s += a + b;
            s2 += a * a + b * b;
        }
        let mean = s / n as f64;
        let var = s2 / n as f64 - mean * mean;
        assert!(mean.abs() < 0.03, "{mean}");
        assert!((var - 1.0).abs() < 0.05, "{var}");
        assert_eq!(gauss_pair(1, 2, 3), gauss_pair(1, 2, 3));
        assert_ne!(gauss_pair(1, 2, 3), gauss_pair(1, 2, 4));
    }

    #[test]
    fn crossings_are_counted_once_and_neighbours_ignored() {
        let x = |pts: Vec<Pt>, closed| Strand {
            pts,
            closed,
            solution: false,
        };
        // an X: two separate strands crossing once
        let s = vec![
            x(vec![[0.0, 0.0], [2.0, 2.0]], false),
            x(vec![[0.0, 2.0], [2.0, 0.0]], false),
        ];
        assert_eq!(count_crossings(&s), 1);
        // a bow tie: one closed strand crossing itself
        let bow = vec![x(
            vec![[0.0, 0.0], [2.0, 2.0], [2.0, 0.0], [0.0, 2.0]],
            true,
        )];
        assert_eq!(count_crossings(&bow), 1);
        // a square: adjacent segments share corners, no crossing
        let sq = vec![x(
            vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            true,
        )];
        assert_eq!(count_crossings(&sq), 0);
    }

    #[test]
    fn spatial_hash_finds_all_points_within_a_cell() {
        let pts: Vec<Pt> = (0..200)
            .map(|k| [(k * 37 % 101) as f64 / 10.0, (k * 53 % 97) as f64 / 10.0])
            .collect();
        let h = SpatialHash::new(&pts, 1.0, 10.1, 9.7);
        for (i, &p) in pts.iter().enumerate() {
            let mut got = vec![];
            h.for_each_near(p, |j| {
                if dist(p, pts[j]) <= 1.0 {
                    got.push(j)
                }
            });
            got.sort();
            let want: Vec<usize> = (0..pts.len()).filter(|&j| dist(p, pts[j]) <= 1.0).collect();
            assert_eq!(got, want, "{i}");
        }
    }
}
