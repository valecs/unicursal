//! Shared test helpers: synthetic pictures, the repository's shapes, and the
//! invariant checks every construction must pass.
#![allow(dead_code)]

use unicursal_core::mask::{self, GreyImage, Mask};
use unicursal_core::maze::{Built, Cell, Grid};

/// White canvas; `paint(x, y)` says which pixels are black.
pub fn draw(w: usize, h: usize, paint: impl Fn(f64, f64) -> Option<bool>) -> GreyImage {
    let mut px = vec![255u8; w * h];
    for y in 0..h {
        for x in 0..w {
            if let Some(black) = paint(x as f64, y as f64) {
                px[y * w + x] = if black { 0 } else { 255 };
            }
        }
    }
    GreyImage::new(w, h, px).unwrap()
}

/// Inside the ellipse with Pillow-style inclusive bounding box.
pub fn in_ellipse(x: f64, y: f64, x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
    let (cx, cy, rx, ry) = (
        (x0 + x1) / 2.0,
        (y0 + y1) / 2.0,
        (x1 - x0) / 2.0,
        (y1 - y0) / 2.0,
    );
    let (dx, dy) = ((x - cx) / rx, (y - cy) / ry);
    dx * dx + dy * dy <= 1.0
}

/// Donut touching the bottom edge: one hole, edge entrance.
pub fn ring() -> GreyImage {
    draw(300, 300, |x, y| {
        let mut v = None;
        if in_ellipse(x, y, 40.0, 40.0, 260.0, 260.0) {
            v = Some(true);
        }
        if in_ellipse(x, y, 110.0, 110.0, 190.0, 190.0) {
            v = Some(false);
        }
        if (130.0..=170.0).contains(&x) && y >= 250.0 {
            v = Some(true);
        }
        v
    })
}

/// Two separate pieces: exercises bridging.
pub fn two_blobs() -> GreyImage {
    draw(300, 200, |x, y| {
        (in_ellipse(x, y, 20.0, 40.0, 120.0, 160.0) || in_ellipse(x, y, 180.0, 40.0, 280.0, 160.0))
            .then_some(true)
    })
}

/// A PNG from `shapes/`, as greyscale.
pub fn shape(name: &str) -> GreyImage {
    let bytes: &[u8] = match name {
        "cat" => include_bytes!("../../../../shapes/cat.png"),
        "ghost" => include_bytes!("../../../../shapes/ghost.png"),
        "heart" => include_bytes!("../../../../shapes/heart.png"),
        "hi" => include_bytes!("../../../../shapes/hi.png"),
        "jack" => include_bytes!("../../../../shapes/jack.png"),
        "skull" => include_bytes!("../../../../shapes/skull.png"),
        "witch" => include_bytes!("../../../../shapes/witch.png"),
        _ => panic!("no shape {name}"),
    };
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    let (w, h) = (info.width as usize, info.height as usize);
    let px = &buf[..info.buffer_size()];
    match info.color_type {
        png::ColorType::Grayscale => GreyImage::new(w, h, px.to_vec()).unwrap(),
        png::ColorType::GrayscaleAlpha => {
            let rgba: Vec<u8> = px
                .chunks(2)
                .flat_map(|p| [p[0], p[0], p[0], p[1]])
                .collect();
            GreyImage::from_rgba(w, h, &rgba).unwrap()
        }
        png::ColorType::Rgb => {
            let rgba: Vec<u8> = px.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
            GreyImage::from_rgba(w, h, &rgba).unwrap()
        }
        png::ColorType::Rgba => GreyImage::from_rgba(w, h, px).unwrap(),
        t => panic!("unexpected colour type {t:?}"),
    }
}

/// The picture at `cells`, as one piece (bridged).
pub fn picture(img: &GreyImage, cells: usize) -> Mask {
    mask::largest_component(&mask::load_mask(img, cells, 128, false), true)
        .unwrap()
        .0
}

pub fn all_cells(g: &Grid) -> impl Iterator<Item = Cell> + '_ {
    (0..g.rows).flat_map(move |r| (0..g.cols).map(move |c| (r, c)))
}

/// Cells on the open corridor from the entrance.
pub fn solution_mask(b: &Built) -> Mask {
    let mut m = Mask::new(b.grid.rows, b.grid.cols);
    for (r, c) in b.solution() {
        m.set(r, c, true);
    }
    m
}

/// Every cell has exactly two openings except the two solution ends, which
/// have one; disjoint cycles + one path means passages = cells - 1; the
/// solution is exactly the picture.
pub fn check_labyrinth(b: &Built) {
    let g = &b.grid;
    let o = b.opening;
    for p in all_cells(g) {
        let want = if p == o.entrance || p == o.exit { 1 } else { 2 };
        assert_eq!(g.degree(p), want, "degree at {p:?}");
    }
    assert_eq!(g.n_passages(), g.rows * g.cols - 1);
    assert_eq!(solution_mask(b), b.fine_fg, "solution is the picture");
}

/// Each background region of the fine grid is one sealed loop.
pub fn check_background_loops(b: &Built) {
    let g = &b.grid;
    let (lab, k) = mask::components(&b.fine_fg.not());
    for t in 1..=k {
        let cells: Vec<Cell> = (0..lab.len())
            .filter(|&i| lab[i] == t)
            .map(|i| (i / g.cols, i % g.cols))
            .collect();
        let start = cells[0];
        let (mut prev, mut cur, mut n) = (None, start, 0);
        loop {
            assert_eq!(g.degree(cur), 2);
            let next = g
                .open_nbrs(cur)
                .find(|&q| Some(q) != prev && lab[q.0 * g.cols + q.1] == t)
                .expect("loop continues inside its region");
            prev = Some(cur);
            cur = next;
            n += 1;
            if cur == start || n > cells.len() {
                break;
            }
        }
        assert!(
            cur == start && n == cells.len(),
            "region {t}: one cycle covers it"
        );
    }
}

/// Open passages form a spanning tree and the solution is exactly the picture.
pub fn check_maze(b: &Built) {
    let g = &b.grid;
    assert_eq!(g.n_passages(), g.rows * g.cols - 1);
    let mut seen = vec![false; g.rows * g.cols];
    let mut stack = vec![(0, 0)];
    seen[0] = true;
    let mut n = 1;
    while let Some(p) = stack.pop() {
        for q in g.open_nbrs(p) {
            if !seen[q.0 * g.cols + q.1] {
                seen[q.0 * g.cols + q.1] = true;
                n += 1;
                stack.push(q);
            }
        }
    }
    assert_eq!(n, g.rows * g.cols, "connected");
    assert_eq!(solution_mask(b), b.fine_fg, "solution is the picture");
}
