//! Picture → boolean mask of picture cells: greyscale conversion, Lanczos
//! downsampling (as Pillow does it), threshold, 4-connected components, and
//! joining separate pieces with thin bridges.

use std::collections::VecDeque;

use crate::Error;

/// An 8-bit greyscale image, row-major. 0 is black.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreyImage {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

impl GreyImage {
    pub fn new(width: usize, height: usize, pixels: Vec<u8>) -> Result<Self, Error> {
        if width == 0 || height == 0 || pixels.len() != width * height {
            return Err(Error::BadImage(format!(
                "{width}x{height} image with {} pixels",
                pixels.len()
            )));
        }
        Ok(GreyImage {
            width,
            height,
            pixels,
        })
    }

    /// From 8-bit RGBA (as a browser canvas gives it). Luma by ITU-R 601-2,
    /// as Pillow's `convert("L")`, after compositing over white, so that
    /// transparent pixels count as background.
    pub fn from_rgba(width: usize, height: usize, rgba: &[u8]) -> Result<Self, Error> {
        if rgba.len() != width * height * 4 {
            return Err(Error::BadImage(format!(
                "{width}x{height} RGBA image with {} bytes",
                rgba.len()
            )));
        }
        let pixels = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                let a = p[3] as u32;
                let over = |c: u8| (c as u32 * a + 255 * (255 - a) + 127) / 255;
                let (r, g, b) = (over(p[0]), over(p[1]), over(p[2]));
                // Pillow: L = (R*19595 + G*38470 + B*7471 + 0x8000) >> 16
                ((r * 19595 + g * 38470 + b * 7471 + 0x8000) >> 16) as u8
            })
            .collect();
        GreyImage::new(width, height, pixels)
    }

    /// Picture-cell grid size for `cells` cells on the long side, following
    /// the image's aspect ratio: (rows, cols).
    pub fn grid_for(&self, cells: usize) -> (usize, usize) {
        grid_for(self.width, self.height, cells)
    }
}

/// (rows, cols) with `cells` on the long side. Ties round to even, like
/// Python's `round`.
pub fn grid_for(width: usize, height: usize, cells: usize) -> (usize, usize) {
    let short = |long: usize, other: usize| {
        (((cells * other) as f64 / long as f64).round_ties_even() as usize).max(1)
    };
    if width >= height {
        (short(width, height), cells)
    } else {
        (cells, short(height, width))
    }
}

/// A boolean grid, row-major. `true` = picture (foreground).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Mask {
    pub rows: usize,
    pub cols: usize,
    pub cells: Vec<bool>,
}

impl Mask {
    pub fn new(rows: usize, cols: usize) -> Self {
        Mask {
            rows,
            cols,
            cells: vec![false; rows * cols],
        }
    }

    /// From rows of booleans (all the same length).
    pub fn from_rows(rows: &[Vec<bool>]) -> Self {
        let cols = rows.first().map_or(0, Vec::len);
        assert!(rows.iter().all(|r| r.len() == cols), "ragged mask");
        Mask {
            rows: rows.len(),
            cols,
            cells: rows.concat(),
        }
    }

    #[inline]
    pub fn get(&self, r: usize, c: usize) -> bool {
        self.cells[r * self.cols + c]
    }

    #[inline]
    pub fn set(&mut self, r: usize, c: usize, v: bool) {
        self.cells[r * self.cols + c] = v;
    }

    pub fn count(&self) -> usize {
        self.cells.iter().filter(|&&b| b).count()
    }

    pub fn not(&self) -> Mask {
        Mask {
            cells: self.cells.iter().map(|b| !b).collect(),
            ..*self
        }
    }

    /// Set cells as (row, col), in raster order.
    pub fn positions(&self) -> Vec<(usize, usize)> {
        (0..self.rows)
            .flat_map(|r| (0..self.cols).map(move |c| (r, c)))
            .filter(|&(r, c)| self.get(r, c))
            .collect()
    }

    /// Each cell doubled into a 2×2 block.
    pub fn doubled(&self) -> Mask {
        let mut out = Mask::new(2 * self.rows, 2 * self.cols);
        for r in 0..out.rows {
            for c in 0..out.cols {
                out.set(r, c, self.get(r / 2, c / 2));
            }
        }
        out
    }

    /// 4-neighbours inside the grid, in the order E, S, W, N.
    pub fn nbrs(&self, r: usize, c: usize) -> impl Iterator<Item = (usize, usize)> + use<> {
        nbrs(self.rows, self.cols, r, c)
    }
}

/// Neighbour offsets in the prototype's order (`NBRS`): E, S, W, N.
pub const NBRS: [(isize, isize); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];

pub fn nbrs(
    rows: usize,
    cols: usize,
    r: usize,
    c: usize,
) -> impl Iterator<Item = (usize, usize)> + use<> {
    NBRS.into_iter().filter_map(move |(dr, dc)| {
        let (nr, nc) = (r.checked_add_signed(dr)?, c.checked_add_signed(dc)?);
        (nr < rows && nc < cols).then_some((nr, nc))
    })
}

/// Lanczos-3 resampling to `rows × cols`, the way Pillow's `resize(...,
/// LANCZOS)` does it for 8-bit images (antialiased support, 22-bit fixed-point
/// weights, horizontal pass then vertical, each clipped to 0..=255).
pub fn resample(img: &GreyImage, rows: usize, cols: usize) -> GreyImage {
    let mut cur = img.clone();
    if cols != cur.width {
        let k = Coeffs::new(cur.width, cols);
        let mut px = vec![0u8; cur.height * cols];
        for y in 0..cur.height {
            let row = &cur.pixels[y * cur.width..(y + 1) * cur.width];
            for x in 0..cols {
                px[y * cols + x] = k.apply(x, |i| row[i]);
            }
        }
        cur = GreyImage {
            width: cols,
            height: cur.height,
            pixels: px,
        };
    }
    if rows != cur.height {
        let k = Coeffs::new(cur.height, rows);
        let mut px = vec![0u8; rows * cur.width];
        for y in 0..rows {
            for x in 0..cur.width {
                px[y * cur.width + x] = k.apply(y, |i| cur.pixels[i * cur.width + x]);
            }
        }
        cur = GreyImage {
            width: cur.width,
            height: rows,
            pixels: px,
        };
    }
    cur
}

const PRECISION_BITS: u32 = 32 - 8 - 2;

struct Coeffs {
    /// Per output pixel: first input index and fixed-point weights.
    taps: Vec<(usize, Vec<i64>)>,
}

impl Coeffs {
    fn new(in_size: usize, out_size: usize) -> Self {
        let scale = in_size as f64 / out_size as f64;
        let filterscale = scale.max(1.0);
        let support = 3.0 * filterscale;
        let taps = (0..out_size)
            .map(|xx| {
                let center = (xx as f64 + 0.5) * scale;
                let xmin = ((center - support + 0.5) as i64).max(0) as usize;
                let xmax = ((center + support + 0.5) as i64).min(in_size as i64) as usize;
                let w: Vec<f64> = (xmin..xmax)
                    .map(|x| lanczos((x as f64 - center + 0.5) / filterscale))
                    .collect();
                let total: f64 = w.iter().sum();
                let fixed = w
                    .iter()
                    .map(|&k| {
                        let k = if total != 0.0 { k / total } else { k };
                        let f = k * (1u64 << PRECISION_BITS) as f64;
                        if k < 0.0 {
                            (f - 0.5) as i64
                        } else {
                            (f + 0.5) as i64
                        }
                    })
                    .collect();
                (xmin, fixed)
            })
            .collect();
        Coeffs { taps }
    }

    fn apply(&self, out: usize, px: impl Fn(usize) -> u8) -> u8 {
        let (xmin, w) = &self.taps[out];
        let mut ss: i64 = 1 << (PRECISION_BITS - 1);
        for (i, &k) in w.iter().enumerate() {
            ss += px(xmin + i) as i64 * k;
        }
        (ss >> PRECISION_BITS).clamp(0, 255) as u8
    }
}

fn sinc(x: f64) -> f64 {
    if x == 0.0 {
        1.0
    } else {
        let px = x * std::f64::consts::PI;
        libm::sin(px) / px
    }
}

fn lanczos(x: f64) -> f64 {
    if (-3.0..3.0).contains(&x) {
        sinc(x) * sinc(x / 3.0)
    } else {
        0.0
    }
}

/// Downsample to `cells` on the long side and threshold: dark pixels (below
/// `threshold`) are the picture, or light ones with `invert`.
pub fn load_mask(img: &GreyImage, cells: usize, threshold: u8, invert: bool) -> Mask {
    let (rows, cols) = img.grid_for(cells);
    let small = resample(img, rows, cols);
    Mask {
        rows,
        cols,
        cells: small
            .pixels
            .iter()
            .map(|&p| (p < threshold) != invert)
            .collect(),
    }
}

/// 4-connected components: (labels, count). Labels are 1..=count, numbered in
/// raster order of each component's first cell; 0 = not in the mask.
pub fn components(mask: &Mask) -> (Vec<u32>, u32) {
    let mut lab = vec![0u32; mask.cells.len()];
    let mut k = 0;
    let mut stack = Vec::new();
    for i in 0..mask.cells.len() {
        if mask.cells[i] && lab[i] == 0 {
            k += 1;
            lab[i] = k;
            stack.push(i);
            while let Some(j) = stack.pop() {
                for (r, c) in mask.nbrs(j / mask.cols, j % mask.cols) {
                    let q = r * mask.cols + c;
                    if mask.cells[q] && lab[q] == 0 {
                        lab[q] = k;
                        stack.push(q);
                    }
                }
            }
        }
    }
    (lab, k)
}

/// Sizes of components 1..=count (index 0 is component 1).
fn sizes(lab: &[u32], k: u32) -> Vec<usize> {
    let mut s = vec![0usize; k as usize];
    for &l in lab {
        if l > 0 {
            s[l as usize - 1] += 1;
        }
    }
    s
}

/// Label of the largest component (the first one on ties).
fn largest(lab: &[u32], k: u32) -> u32 {
    let s = sizes(lab, k);
    let mut best = 0;
    for (i, &n) in s.iter().enumerate() {
        if n > s[best] {
            best = i;
        }
    }
    best as u32 + 1
}

/// Join all foreground pieces with 1-cell-wide bridges, each the shortest
/// background path from the growing main piece to the nearest other piece.
/// Returns (joined mask, number of bridges).
pub fn bridge_components(mask: &Mask) -> (Mask, usize) {
    let mut mask = mask.clone();
    let n = mask.cells.len();
    let mut bridges = 0;
    loop {
        let (lab, k) = components(&mask);
        if k <= 1 {
            return (mask, bridges);
        }
        let main = largest(&lab, k);
        // BFS from every cell of the main piece; prev[x] = Some(p) once seen.
        const SOURCE: usize = usize::MAX;
        let mut prev: Vec<Option<usize>> = vec![None; n];
        let mut q = VecDeque::new();
        for i in 0..n {
            if lab[i] == main {
                prev[i] = Some(SOURCE);
                q.push_back(i);
            }
        }
        let mut hit = None;
        'bfs: while let Some(p) = q.pop_front() {
            for (r, c) in mask.nbrs(p / mask.cols, p % mask.cols) {
                let x = r * mask.cols + c;
                if prev[x].is_none() {
                    prev[x] = Some(p);
                    if mask.cells[x] {
                        hit = Some(x);
                        break 'bfs;
                    }
                    q.push_back(x);
                }
            }
        }
        let hit = hit.expect("another piece is reachable");
        let mut p = prev[hit].unwrap();
        while prev[p] != Some(SOURCE) {
            mask.cells[p] = true;
            p = prev[p].unwrap();
        }
        bridges += 1;
    }
}

/// The picture as one 4-connected piece: other pieces joined by bridges, or
/// (without `bridge`) dropped. Also returns notes for the user.
pub fn largest_component(mask: &Mask, bridge: bool) -> Result<(Mask, Vec<String>), Error> {
    let (lab, k) = components(mask);
    if k == 0 {
        return Err(Error::EmptyPicture);
    }
    if k == 1 {
        return Ok((mask.clone(), vec![]));
    }
    if bridge {
        let (joined, nb) = bridge_components(mask);
        return Ok((
            joined,
            vec![format!(
                "joined {k} foreground pieces with {nb} thin bridges"
            )],
        ));
    }
    let keep = largest(&lab, k);
    let s = sizes(&lab, k);
    let dropped: usize = s.iter().sum::<usize>() - s[keep as usize - 1];
    let kept = Mask {
        cells: lab.iter().map(|&l| l == keep).collect(),
        ..*mask
    };
    Ok((
        kept,
        vec![format!(
            "foreground has {k} pieces; kept the largest, dropped {dropped} cells. \
             Thicken thin parts or join pieces in the source image to keep them."
        )],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_follows_aspect_and_rounds_ties_to_even() {
        assert_eq!(grid_for(240, 320, 16), (16, 12));
        assert_eq!(grid_for(300, 200, 30), (20, 30));
        // 5 * 1 / 2 = 2.5 rounds to 2, like Python
        assert_eq!(grid_for(2, 1, 5), (2, 5));
        assert_eq!(grid_for(100, 1, 5), (1, 5));
    }

    #[test]
    fn resample_keeps_flat_images_flat() {
        for v in [0u8, 77, 255] {
            let img = GreyImage::new(37, 23, vec![v; 37 * 23]).unwrap();
            let s = resample(&img, 5, 8);
            assert!(s.pixels.iter().all(|&p| p == v), "{v}: {:?}", s.pixels);
        }
    }

    #[test]
    fn rgba_transparent_is_background() {
        let g = GreyImage::from_rgba(2, 1, &[0, 0, 0, 0, 0, 0, 0, 255]).unwrap();
        assert_eq!(g.pixels, [255, 0]);
    }

    #[test]
    fn components_are_numbered_in_raster_order() {
        let m = Mask::from_rows(&[
            vec![false, true, false],
            vec![true, false, true],
            vec![true, false, true],
        ]);
        let (lab, k) = components(&m);
        assert_eq!(k, 3);
        assert_eq!(lab, [0, 1, 0, 2, 0, 3, 2, 0, 3]);
    }
}
