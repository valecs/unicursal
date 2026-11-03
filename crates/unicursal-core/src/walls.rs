//! Flow walls and checks: walls as Voronoi ridges between strands (clipped to
//! the corridor tubes), the clear corridor width, and the leak check (open
//! regions must equal strands). Port of `flow.py`'s `flow_walls`,
//! `corridor_widths` and `verify_regions`.

use delaunator::{EMPTY, Point, triangulate};

use crate::flow::{Pt, SpatialHash, Strand, densify, map_indices};

/// Tube radius: half the corridor drawn around each strand (lattice units).
pub const RMAX: f64 = 0.55;
/// Wall thickness.
pub const WALL: f64 = 0.14;
/// Samples closer than this along the same strand get no wall between them.
pub const GAP_ARC: f64 = 1.05;

fn sub(a: Pt, b: Pt) -> Pt {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: Pt, b: Pt) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn norm(a: Pt) -> f64 {
    dot(a, a).sqrt()
}

fn circumcenter(a: Pt, b: Pt, c: Pt) -> Pt {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let (ex, ey) = (c[0] - a[0], c[1] - a[1]);
    let (bl, cl) = (dx * dx + dy * dy, ex * ex + ey * ey);
    let d = 0.5 / (dx * ey - dy * ex);
    [
        a[0] + (ey * bl - dy * cl) * d,
        a[1] + (dx * cl - ex * bl) * d,
    ]
}

/// Wall segments (lattice coordinates): Voronoi ridges between samples that
/// aren't arc-neighbours on the same strand, clipped to the tubes.
pub fn flow_walls(strands: &[Strand], rows: usize, cols: usize) -> Vec<(Pt, Pt)> {
    let dense: Vec<(Vec<Pt>, bool)> = strands
        .iter()
        .map(|s| (densify(&s.pts, s.closed, 0.2), s.closed))
        .collect();
    let (mut pts, mut sid, mut arc, mut alen, mut cl) = (vec![], vec![], vec![], vec![], vec![]);
    let mut maxseg: f64 = 0.0;
    for (k, (p, closed)) in dense.iter().enumerate() {
        let mut s = 0.0;
        let n = p.len();
        let mut arcs = Vec::with_capacity(n);
        for i in 0..n {
            arcs.push(s);
            if i + 1 < n || *closed {
                let seg = norm(sub(p[(i + 1) % n], p[i]));
                maxseg = maxseg.max(seg);
                s += seg;
            }
        }
        for (i, &q) in p.iter().enumerate() {
            pts.push(q);
            sid.push(k);
            arc.push(arcs[i]);
            alen.push(s);
            cl.push(*closed);
        }
    }
    // a ridge point is inside a tube if within RMAX of the strand's polyline;
    // samples sit up to maxseg/2 off it, so widen the clip, and overshoot into
    // the void so walls close cusps
    let rclip = (RMAX * RMAX + (maxseg / 2.0) * (maxseg / 2.0)).sqrt() + 0.15;
    let n = pts.len();
    // frame points, so that outer ridges exist
    let mut all = pts.clone();
    for k in 0..=2 * cols {
        let x = k as f64 * 0.5;
        all.push([x, -0.5]);
        all.push([x, rows as f64 + 0.5]);
    }
    for k in 0..=2 * rows {
        let y = k as f64 * 0.5;
        all.push([-0.5, y]);
        all.push([cols as f64 + 0.5, y]);
    }
    let input: Vec<Point> = all.iter().map(|p| Point { x: p[0], y: p[1] }).collect();
    let tri = triangulate(&input);
    let next = |e: usize| if e % 3 == 2 { e - 2 } else { e + 1 };
    let corner = |e: usize| all[tri.triangles[e]];
    let centre = |e: usize| {
        let t = e - e % 3;
        circumcenter(corner(t), corner(t + 1), corner(t + 2))
    };
    let mut segs = vec![];
    for e in 0..tri.triangles.len() {
        let twin = tri.halfedges[e];
        if twin == EMPTY || twin < e {
            continue; // hull edge (infinite ridge), or seen from the other side
        }
        let (i, j) = (tri.triangles[e], tri.triangles[next(e)]);
        if i >= n || j >= n {
            continue;
        }
        if sid[i] == sid[j] {
            let mut da = (arc[i] - arc[j]).abs();
            if cl[i] {
                da = da.min(alen[i] - da);
            }
            if da < GAP_ARC {
                continue;
            }
        }
        let (a, b) = (centre(e), centre(twin));
        // clip the ridge to within rclip of its generators
        let (pi, pj) = (pts[i], pts[j]);
        let mid = [0.5 * (pi[0] + pj[0]), 0.5 * (pi[1] + pj[1])];
        let h2 = rclip * rclip - dot(sub(pi, mid), sub(pi, mid));
        if h2 <= 0.0 {
            continue;
        }
        let h = h2.sqrt();
        let d = sub(pj, pi);
        let len = norm(d) + 1e-12;
        let u = [-d[1] / len, d[0] / len];
        let (ta, tb) = (dot(sub(a, mid), u), dot(sub(b, mid), u));
        let (lo, hi) = (ta.min(tb).max(-h), ta.max(tb).min(h));
        if hi <= lo {
            continue;
        }
        segs.push((
            [mid[0] + lo * u[0], mid[1] + lo * u[1]],
            [mid[0] + hi * u[0], mid[1] + hi * u[1]],
        ));
    }
    segs
}

/// Squared distance from `p` to segment `ab`.
fn seg_dist2(p: Pt, a: Pt, b: Pt) -> f64 {
    let ab = sub(b, a);
    let ap = sub(p, a);
    let l2 = dot(ab, ab);
    let t = if l2 > 0.0 {
        (dot(ap, ab) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let q = sub(ap, [ab[0] * t, ab[1] * t]);
    dot(q, q)
}

/// Clear corridor width (wall edge to wall edge, lattice units) at points
/// every 0.1 along every strand: the distance to the nearest wall on the left
/// plus on the right, minus the wall thickness. A side with no wall nearby
/// (void) counts as the tube edge; the frame is a wall too.
pub fn corridor_widths(
    strands: &[Strand],
    walls: &[(Pt, Pt)],
    rows: usize,
    cols: usize,
) -> Vec<f64> {
    let mut wp = vec![];
    for &(a, b) in walls {
        let n = (((norm(sub(b, a)) / 0.04).ceil() as usize) + 1).max(2);
        for k in 0..n {
            let t = k as f64 / (n - 1) as f64;
            wp.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
        }
    }
    let cap = RMAX + WALL / 2.0;
    // only walls nearer than `cap` can lower a side below its default
    let hash = SpatialHash::new(&wp, cap, cols as f64 + 1.0, rows as f64 + 1.0);
    let (w, h) = (cols as f64, rows as f64);
    let mut out = vec![];
    for s in strands {
        let mut q = densify(&s.pts, s.closed, 0.1);
        if !s.closed {
            // skip the open ends, which run through the frame gap
            q = if q.len() > 12 {
                q[6..q.len() - 6].to_vec()
            } else {
                vec![]
            };
        }
        let m = q.len();
        if m < 3 {
            continue;
        }
        for i in 0..m {
            // np.gradient along the polyline
            let t = if i == 0 {
                sub(q[1], q[0])
            } else if i == m - 1 {
                sub(q[m - 1], q[m - 2])
            } else {
                let d = sub(q[i + 1], q[i - 1]);
                [d[0] / 2.0, d[1] / 2.0]
            };
            let tl = norm(t) + 1e-12;
            let t = [t[0] / tl, t[1] / tl];
            let (mut left, mut right) = (cap, cap);
            hash.for_each_near(q[i], |j| {
                let v = sub(wp[j], q[i]);
                let d = norm(v);
                if t[0] * v[1] - t[1] * v[0] > 0.0 {
                    left = left.min(d);
                } else {
                    right = right.min(d);
                }
            });
            let fr = q[i][0].min(w - q[i][0]).min(q[i][1]).min(h - q[i][1]);
            if fr < cap {
                if left >= right {
                    left = left.min(fr);
                } else {
                    right = right.min(fr);
                }
            }
            out.push(left + right - WALL);
        }
    }
    out
}

/// What the leak check sees of the puzzle: the corridor tubes, the walls, and
/// the white stubs that carry the solution out through the frame.
pub struct OpenSpace<'a> {
    pub tubes: Vec<(Vec<Pt>, bool)>,
    pub walls: &'a [(Pt, Pt)],
    pub stubs: Vec<(Pt, Pt)>,
}

/// Count open (white) regions inside the frame by rasterising the puzzle's
/// geometry exactly as drawn (round-capped tubes of radius RMAX, round-capped
/// walls of width WALL on top, butt-capped stubs of width 2·RMAX − WALL on
/// top of those) at `px` pixels per corridor. A sealed labyrinth has one
/// region per strand. Specks of ≤ 20 pixels are ignored, as in the prototype.
///
/// Pure f64 geometry, no anti-aliasing: the count is the same on every target.
pub fn open_regions(space: &OpenSpace, rows: usize, cols: usize, px: usize) -> usize {
    let (pw, ph) = (cols * px, rows * px);
    let pad = 4;
    // bucket segments by the unit cells their RMAX-neighbourhood touches
    let (bw, bh) = (cols + 2, rows + 2);
    let mut tube_b: Vec<Vec<(Pt, Pt)>> = vec![vec![]; bw * bh];
    let mut wall_b: Vec<Vec<(Pt, Pt)>> = vec![vec![]; bw * bh];
    let add = |buckets: &mut Vec<Vec<(Pt, Pt)>>, a: Pt, b: Pt, r: f64| {
        let x0 = ((a[0].min(b[0]) - r).floor().max(-1.0) + 1.0) as usize;
        let x1 = ((a[0].max(b[0]) + r).floor().min(cols as f64) + 1.0) as usize;
        let y0 = ((a[1].min(b[1]) - r).floor().max(-1.0) + 1.0) as usize;
        let y1 = ((a[1].max(b[1]) + r).floor().min(rows as f64) + 1.0) as usize;
        for y in y0..=y1.min(bh - 1) {
            for x in x0..=x1.min(bw - 1) {
                buckets[y * bw + x].push((a, b));
            }
        }
    };
    for (p, closed) in &space.tubes {
        let n = p.len();
        let segs = if *closed { n } else { n - 1 };
        for k in 0..segs {
            add(&mut tube_b, p[k], p[(k + 1) % n], RMAX);
        }
        if n == 1 {
            add(&mut tube_b, p[0], p[0], RMAX);
        }
    }
    for &(a, b) in space.walls {
        add(&mut wall_b, a, b, WALL / 2.0);
    }
    let stub_hw = RMAX - WALL / 2.0;
    let in_stub = |p: Pt| {
        space.stubs.iter().any(|&(a, b)| {
            let ab = sub(b, a);
            let l2 = dot(ab, ab);
            let t = dot(sub(p, a), ab) / l2;
            if !(0.0..=1.0).contains(&t) {
                return false;
            }
            let q = sub(sub(p, a), [ab[0] * t, ab[1] * t]);
            dot(q, q) <= stub_hw * stub_hw
        })
    };
    let (r2, w2) = (RMAX * RMAX, (WALL / 2.0) * (WALL / 2.0));
    let rows_white: Vec<Vec<bool>> = map_indices(ph, |y| {
        (0..pw)
            .map(|x| {
                if x < pad || y < pad || x >= pw - pad || y >= ph - pad {
                    return false;
                }
                let p = [(x as f64 + 0.5) / px as f64, (y as f64 + 0.5) / px as f64];
                let b = (p[1].floor() as usize + 1) * bw + p[0].floor() as usize + 1;
                if in_stub(p) {
                    return true;
                }
                tube_b[b].iter().any(|&(a, c)| seg_dist2(p, a, c) <= r2)
                    && !wall_b[b].iter().any(|&(a, c)| seg_dist2(p, a, c) <= w2)
            })
            .collect()
    });
    let white: Vec<bool> = rows_white.concat();
    // 4-connected components, ignoring specks
    let mut seen = vec![false; white.len()];
    let mut count = 0;
    let mut stack = vec![];
    for s in 0..white.len() {
        if !white[s] || seen[s] {
            continue;
        }
        seen[s] = true;
        stack.push(s);
        let mut size = 0;
        while let Some(i) = stack.pop() {
            size += 1;
            let (y, x) = (i / pw, i % pw);
            let mut visit = |j: usize| {
                if white[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < pw {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - pw);
            }
            if y + 1 < ph {
                visit(i + pw);
            }
        }
        if size > 20 {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circumcentre_is_equidistant() {
        let (a, b, c) = ([0.0, 0.0], [4.0, 0.0], [0.0, 2.0]);
        let o = circumcenter(a, b, c);
        assert_eq!(o, [2.0, 1.0]);
    }

    #[test]
    fn segment_distance() {
        assert_eq!(seg_dist2([1.0, 1.0], [0.0, 0.0], [2.0, 0.0]), 1.0);
        assert_eq!(seg_dist2([3.0, 0.0], [0.0, 0.0], [2.0, 0.0]), 1.0);
        assert_eq!(seg_dist2([1.0, 1.0], [0.0, 0.0], [0.0, 0.0]), 2.0);
    }

    fn line(y: f64, closed: bool) -> Strand {
        Strand {
            pts: (0..=8).map(|k| [0.5 + k as f64 * 0.5, y]).collect(),
            closed,
            solution: false,
        }
    }

    #[test]
    fn two_parallel_strands_get_a_wall_between_them() {
        let s = vec![line(1.5, false), line(2.5, false)];
        let walls = flow_walls(&s, 4, 5);
        assert!(!walls.is_empty());
        // every wall between the strands lies near y = 2
        let between: Vec<_> = walls
            .iter()
            .filter(|(a, b)| a[1] > 1.6 && a[1] < 2.4 && b[1] > 1.6 && b[1] < 2.4)
            .collect();
        assert!(!between.is_empty());
        for (a, b) in between {
            assert!(
                (a[1] - 2.0).abs() < 1e-6 && (b[1] - 2.0).abs() < 1e-6,
                "{a:?} {b:?}"
            );
        }
        // and none along the strands themselves
        assert!(
            walls
                .iter()
                .all(|(a, b)| (a[1] - 1.5).abs() > 0.2 || (b[1] - 1.5).abs() > 0.2)
        );
        let w = corridor_widths(&s, &walls, 4, 5);
        assert!(w.iter().all(|&x| x > 0.8 && x < 1.0), "{w:?}");
    }

    #[test]
    fn open_regions_counts_separated_tubes() {
        let s = vec![line(1.5, false), line(2.5, false)];
        let tubes: Vec<_> = s.iter().map(|s| (s.pts.clone(), false)).collect();
        let none = OpenSpace {
            tubes: tubes.clone(),
            walls: &[],
            stubs: vec![],
        };
        assert_eq!(open_regions(&none, 4, 5, 20), 1, "touching tubes merge");
        let walls = flow_walls(&s, 4, 5);
        let walled = OpenSpace {
            tubes,
            walls: &walls,
            stubs: vec![],
        };
        assert_eq!(open_regions(&walled, 4, 5, 20), 2, "a wall separates them");
    }
}
