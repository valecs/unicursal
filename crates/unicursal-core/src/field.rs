//! Scalar fields on the fine grid: Euclidean distance transform, the picture's
//! signed distance, its gradient, and bilinear sampling. These replace
//! scipy's `distance_transform_edt`, `np.gradient` and `map_coordinates`.

use crate::mask::Mask;

/// A row-major `rows × cols` field of f64, one value per fine cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub rows: usize,
    pub cols: usize,
    pub v: Vec<f64>,
}

impl Field {
    #[inline]
    pub fn at(&self, r: usize, c: usize) -> f64 {
        self.v[r * self.cols + c]
    }

    /// Bilinear interpolation at (row, col) in cell-index coordinates,
    /// clamped to the grid (scipy `map_coordinates(order=1, mode="nearest")`).
    pub fn sample(&self, r: f64, c: f64) -> f64 {
        let r = r.clamp(0.0, (self.rows - 1) as f64);
        let c = c.clamp(0.0, (self.cols - 1) as f64);
        let (r0, c0) = (r.floor() as usize, c.floor() as usize);
        let (r1, c1) = ((r0 + 1).min(self.rows - 1), (c0 + 1).min(self.cols - 1));
        let (fr, fc) = (r - r0 as f64, c - c0 as f64);
        let top = self.at(r0, c0) * (1.0 - fc) + self.at(r0, c1) * fc;
        let bot = self.at(r1, c0) * (1.0 - fc) + self.at(r1, c1) * fc;
        top * (1.0 - fr) + bot * fr
    }

    /// Sample at lattice point (x, y), whose cell centres are at +0.5.
    pub fn at_xy(&self, x: f64, y: f64) -> f64 {
        self.sample(y - 0.5, x - 0.5)
    }

    /// Central differences inside, one-sided at the edges (`np.gradient`):
    /// (d/drow, d/dcol).
    pub fn gradient(&self) -> (Field, Field) {
        let (h, w) = (self.rows, self.cols);
        let diff = |n: usize, i: usize, get: &dyn Fn(usize) -> f64| -> f64 {
            if n < 2 {
                0.0
            } else if i == 0 {
                get(1) - get(0)
            } else if i == n - 1 {
                get(n - 1) - get(n - 2)
            } else {
                (get(i + 1) - get(i - 1)) / 2.0
            }
        };
        let mut gr = vec![0.0; h * w];
        let mut gc = vec![0.0; h * w];
        for r in 0..h {
            for c in 0..w {
                gr[r * w + c] = diff(h, r, &|k| self.at(k, c));
                gc[r * w + c] = diff(w, c, &|k| self.at(r, k));
            }
        }
        (
            Field {
                rows: h,
                cols: w,
                v: gr,
            },
            Field {
                rows: h,
                cols: w,
                v: gc,
            },
        )
    }
}

/// Exact Euclidean distance from each `true` cell to the nearest `false` cell
/// (0 on `false` cells), by Felzenszwalb & Huttenlocher's lower envelope of
/// parabolas. Where there is no `false` cell at all, distances are
/// `rows + cols` (farther than anything on the grid).
pub fn edt(mask: &Mask) -> Field {
    let (h, w) = (mask.rows, mask.cols);
    let mut sq: Vec<f64> = mask
        .cells
        .iter()
        .map(|&m| if m { f64::INFINITY } else { 0.0 })
        .collect();
    let n = h.max(w);
    let mut buf = Scratch::new(n);
    let mut out = vec![0.0; n];
    for c in 0..w {
        let col: Vec<f64> = (0..h).map(|r| sq[r * w + c]).collect();
        dt1(&col, &mut out[..h], &mut buf);
        for r in 0..h {
            sq[r * w + c] = out[r];
        }
    }
    for r in 0..h {
        let row = sq[r * w..(r + 1) * w].to_vec();
        dt1(&row, &mut out[..w], &mut buf);
        sq[r * w..(r + 1) * w].copy_from_slice(&out[..w]);
    }
    let far = (h + w) as f64;
    Field {
        rows: h,
        cols: w,
        v: sq
            .into_iter()
            .map(|d| if d.is_finite() { d.sqrt() } else { far })
            .collect(),
    }
}

struct Scratch {
    v: Vec<usize>,
    z: Vec<f64>,
}

impl Scratch {
    fn new(n: usize) -> Self {
        Scratch {
            v: vec![0; n],
            z: vec![0.0; n + 1],
        }
    }
}

/// 1-D squared distance transform of `f` (sample costs; infinite = no site).
fn dt1(f: &[f64], d: &mut [f64], s: &mut Scratch) {
    let n = f.len();
    let mut k: isize = -1;
    for q in 0..n {
        if !f[q].is_finite() {
            continue;
        }
        let fq = f[q] + (q * q) as f64;
        loop {
            if k < 0 {
                break;
            }
            let p = s.v[k as usize];
            let sx = (fq - (f[p] + (p * p) as f64)) / (2.0 * q as f64 - 2.0 * p as f64);
            if sx <= s.z[k as usize] {
                k -= 1;
            } else {
                k += 1;
                s.v[k as usize] = q;
                s.z[k as usize] = sx;
                break;
            }
        }
        if k < 0 {
            k = 0;
            s.v[0] = q;
            s.z[0] = f64::NEG_INFINITY;
        }
        s.z[k as usize + 1] = f64::INFINITY;
    }
    if k < 0 {
        d.fill(f64::INFINITY);
        return;
    }
    let mut j = 0;
    for (q, dq) in d.iter_mut().enumerate() {
        while s.z[j + 1] < q as f64 {
            j += 1;
        }
        let p = s.v[j];
        let dx = q as f64 - p as f64;
        *dq = dx * dx + f[p];
    }
}

/// The picture's signed distance: positive inside (distance to the
/// background), negative outside.
pub fn signed_distance(fine_fg: &Mask) -> Field {
    let inside = edt(fine_fg);
    let outside = edt(&fine_fg.not());
    Field {
        rows: inside.rows,
        cols: inside.cols,
        v: inside
            .v
            .iter()
            .zip(&outside.v)
            .map(|(i, o)| i - o)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn brute(mask: &Mask) -> Vec<f64> {
        let far = (mask.rows + mask.cols) as f64;
        (0..mask.rows * mask.cols)
            .map(|i| {
                let (r, c) = (i / mask.cols, i % mask.cols);
                if !mask.cells[i] {
                    return 0.0;
                }
                let mut best = f64::INFINITY;
                for j in 0..mask.cells.len() {
                    if !mask.cells[j] {
                        let (dr, dc) = (
                            (j / mask.cols) as f64 - r as f64,
                            (j % mask.cols) as f64 - c as f64,
                        );
                        best = best.min((dr * dr + dc * dc).sqrt());
                    }
                }
                if best.is_finite() { best } else { far }
            })
            .collect()
    }

    proptest! {
        #[test]
        fn edt_matches_brute_force(
            (h, w, cells) in (1usize..12, 1usize..12)
                .prop_flat_map(|(h, w)| (Just(h), Just(w), proptest::collection::vec(any::<bool>(), h * w)))
        ) {
            let m = Mask { rows: h, cols: w, cells };
            let got = edt(&m).v;
            let want = brute(&m);
            for (g, w) in got.iter().zip(&want) {
                prop_assert!((g - w).abs() < 1e-9, "{g} vs {w}");
            }
        }
    }

    #[test]
    fn gradient_matches_numpy() {
        // np.gradient([[1, 2, 4], [2, 4, 8]]) = ([[1,2,4],[1,2,4]], [[1,1.5,2],[2,3,4]])
        let f = Field {
            rows: 2,
            cols: 3,
            v: vec![1.0, 2.0, 4.0, 2.0, 4.0, 8.0],
        };
        let (gr, gc) = f.gradient();
        assert_eq!(gr.v, [1.0, 2.0, 4.0, 1.0, 2.0, 4.0]);
        assert_eq!(gc.v, [1.0, 1.5, 2.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn bilinear_sampling_interpolates_and_clamps() {
        let f = Field {
            rows: 2,
            cols: 2,
            v: vec![0.0, 1.0, 2.0, 3.0],
        };
        assert_eq!(f.sample(0.5, 0.5), 1.5);
        assert_eq!(f.sample(0.0, 1.0), 1.0);
        assert_eq!(f.sample(-5.0, 9.0), 1.0);
        assert_eq!(f.at_xy(1.0, 1.0), 1.5);
    }

    #[test]
    fn signed_distance_is_positive_inside() {
        let m = Mask::from_rows(&[
            vec![false, false, false, false],
            vec![false, true, true, false],
            vec![false, false, false, false],
        ]);
        let s = signed_distance(&m);
        assert_eq!(s.at(1, 1), 1.0);
        assert_eq!(s.at(0, 0), -(2.0f64).sqrt());
    }
}
