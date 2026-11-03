//! Classic (grid) style: straight walls, the entrance arrow, and the answer
//! key's filled solution.

use crate::mask::Mask;
use crate::maze::{Built, Opening, Side};
use crate::page::MARGIN;
use crate::scene::{Cap, Cmd, Color, Item, Join, Scene, Stroke, rect};

/// Arrow length cap, pt (~4.6 mm).
pub const ARROW_MAX_PT: f64 = 13.0;
/// Frame-to-tip gap cap, pt (~3.2 mm): arrow and gap stay well inside a
/// 12.7 mm paper margin at any corridor size.
pub const ARROW_GAP_MAX_PT: f64 = 9.0;

/// Polygon (fine-cell units, x/y) for the one arrow that marks the entrance
/// (#2); start and finish are adjacent, so one arrow does for both. With a
/// side it sits outside the frame, tip first, pointing in at the middle of the
/// gap; otherwise it hangs above the entrance cells, pointing down at them.
pub fn entrance_arrow(
    o: &Opening,
    rows: usize,
    cols: usize,
    cell: f64,
    gap: f64,
) -> [(f64, f64); 7] {
    let (a, b) = (o.entrance, o.exit);
    let mid = (
        (a.1 + b.1) as f64 / 2.0 + 0.5,
        (a.0 + b.0) as f64 / 2.0 + 0.5,
    );
    let out = o.side.unwrap_or(Side::N).out();
    let len = (ARROW_MAX_PT / cell).min(1.0);
    let gap = gap.min(ARROW_GAP_MAX_PT / cell);
    let (hw, sw) = (0.4 * len, 0.13 * len); // head, shaft half-widths
    let tip = match o.side {
        Some(Side::N) => (mid.0, -gap),
        Some(Side::S) => (mid.0, rows as f64 + gap),
        Some(Side::W) => (-gap, mid.1),
        Some(Side::E) => (cols as f64 + gap, mid.1),
        None => (mid.0 + out.0 * 0.55, mid.1 + out.1 * 0.55),
    };
    let n = (-out.1, out.0); // across the arrow
    let at = |p: (f64, f64), along: f64, across: f64| {
        (
            p.0 + out.0 * along + n.0 * across,
            p.1 + out.1 * along + n.1 * across,
        )
    };
    let (h, t) = (0.5 * len, len);
    [
        tip,
        at(tip, h, hw),
        at(tip, h, sw),
        at(tip, t, sw),
        at(tip, t, -sw),
        at(tip, h, -sw),
        at(tip, h, -hw),
    ]
}

/// The arrow as a filled scene item; `to_pt` maps cell coordinates to pt.
pub fn arrow_item(poly: &[(f64, f64)], to_pt: impl Fn((f64, f64)) -> (f64, f64)) -> Item {
    let mut d: Vec<Cmd> = poly
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let (x, y) = to_pt(p);
            if i == 0 { Cmd::M(x, y) } else { Cmd::L(x, y) }
        })
        .collect();
    d.push(Cmd::Z);
    Item::fill(d, Color::BLACK).with_class("entrance")
}

/// The gaps in the outer frame at the entrance: (side, index along it).
fn frame_gaps(o: &Opening) -> Vec<(Side, usize)> {
    match o.side {
        Some(s @ (Side::N | Side::S)) => vec![(s, o.entrance.1), (s, o.exit.1)],
        Some(s @ (Side::W | Side::E)) => vec![(s, o.entrance.0), (s, o.exit.0)],
        None => vec![],
    }
}

/// Grid-style drawing, `cell` pt per corridor, with a [`MARGIN`]-corridor
/// border. With `solution`, those cells are filled (the answer key).
pub fn render_grid(built: &Built, cell: f64, solution: Option<&Mask>, fill: Color) -> Scene {
    let g = &built.grid;
    let (h, w) = (g.rows, g.cols);
    let o = MARGIN * cell;
    let mut s = Scene::new(w as f64 * cell + 2.0 * o, h as f64 * cell + 2.0 * o);
    let x = |c: usize| o + c as f64 * cell;
    let y = |r: usize| o + r as f64 * cell;

    if let Some(sol) = solution {
        // one rectangle per run of filled cells in a row
        let mut d = vec![];
        for r in 0..h {
            for (c0, c1) in runs(w, |c| sol.get(r, c)) {
                d.extend(rect(x(c0), y(r), (c1 - c0) as f64 * cell, cell));
            }
        }
        if !d.is_empty() {
            s.items.push(Item::fill(d, fill));
        }
    }

    let mut d = vec![];
    // interior walls, merged into runs
    for c in 0..w.saturating_sub(1) {
        for (r0, r1) in runs(h, |r| !g.h(r, c)) {
            d.extend([Cmd::M(x(c + 1), y(r0)), Cmd::L(x(c + 1), y(r1))]);
        }
    }
    for r in 0..h.saturating_sub(1) {
        for (c0, c1) in runs(w, |c| !g.v(r, c)) {
            d.extend([Cmd::M(x(c0), y(r + 1)), Cmd::L(x(c1), y(r + 1))]);
        }
    }
    // outer frame, open at the entrance
    let gaps = frame_gaps(&built.opening);
    let open = |side: Side, k: usize| gaps.contains(&(side, k));
    for (r0, r1) in runs(w, |c| !open(Side::N, c)) {
        d.extend([Cmd::M(x(r0), y(0)), Cmd::L(x(r1), y(0))]);
    }
    for (c0, c1) in runs(w, |c| !open(Side::S, c)) {
        d.extend([Cmd::M(x(c0), y(h)), Cmd::L(x(c1), y(h))]);
    }
    for (r0, r1) in runs(h, |r| !open(Side::W, r)) {
        d.extend([Cmd::M(x(0), y(r0)), Cmd::L(x(0), y(r1))]);
    }
    for (r0, r1) in runs(h, |r| !open(Side::E, r)) {
        d.extend([Cmd::M(x(w), y(r0)), Cmd::L(x(w), y(r1))]);
    }
    s.items.push(Item::stroke(
        d,
        Stroke {
            color: Color::BLACK,
            width: 0.18 * cell,
            cap: Cap::Square,
            join: Join::Miter,
        },
    ));

    let arrow = entrance_arrow(&built.opening, h, w, cell, 0.25);
    s.items.push(arrow_item(&arrow, |(ax, ay)| {
        (o + ax * cell, o + ay * cell)
    }));
    s
}

/// Maximal runs `[start, end)` of `0..n` where `pred` holds.
fn runs(n: usize, pred: impl Fn(usize) -> bool) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let mut k = 0;
    while k < n {
        if pred(k) {
            let s = k;
            while k < n && pred(k) {
                k += 1;
            }
            out.push((s, k));
        } else {
            k += 1;
        }
    }
    out
}

/// What a flow drawing needs besides the relaxed strands.
pub struct FlowStyle {
    pub void: Color,
    pub fill: Color,
    pub show_solution: bool,
    /// Ink saver: outline each corridor where it meets a void, so voids can
    /// be light without corridors seeming to leak into them.
    pub outline: bool,
}

/// The solution strand with a stub poking 0.3 corridors out through the frame
/// at each end, when the entrance is on the frame.
pub fn extended_solution(strands: &[crate::flow::Strand], side: Option<Side>) -> Vec<[f64; 2]> {
    let sol = &strands
        .iter()
        .find(|s| s.solution)
        .expect("a solution strand")
        .pts;
    match side {
        None => sol.clone(),
        Some(side) => {
            let (ox, oy) = side.out();
            let (a, b) = (sol[0], sol[sol.len() - 1]);
            let mut v = vec![[a[0] + ox * 0.8, a[1] + oy * 0.8]];
            v.extend_from_slice(sol);
            v.push([b[0] + ox * 0.8, b[1] + oy * 0.8]);
            v
        }
    }
}

/// Flow-style drawing, `cell` pt per corridor, with a [`MARGIN`]-corridor
/// border: void-coloured frame, white tubes along the strands, Voronoi walls,
/// the frame, white stubs through the entrance gap, and the arrow.
pub fn render_flow(
    strands: &[crate::flow::Strand],
    walls: &[([f64; 2], [f64; 2])],
    opening: &Opening,
    rows: usize,
    cols: usize,
    cell: f64,
    style: &FlowStyle,
) -> Scene {
    use crate::walls::{RMAX, WALL};
    let o = MARGIN * cell;
    let mut s = Scene::new(cols as f64 * cell + 2.0 * o, rows as f64 * cell + 2.0 * o);
    let t = |p: [f64; 2]| (o + p[0] * cell, o + p[1] * cell);
    let poly = |pts: &[[f64; 2]], closed: bool, d: &mut Vec<Cmd>| {
        for (k, &p) in pts.iter().enumerate() {
            let (x, y) = t(p);
            d.push(if k == 0 { Cmd::M(x, y) } else { Cmd::L(x, y) });
        }
        if closed {
            d.push(Cmd::Z);
        }
    };
    let round = |color: Color, width: f64| Stroke {
        color,
        width,
        cap: Cap::Round,
        join: Join::Round,
    };
    let frame = rect(o, o, cols as f64 * cell, rows as f64 * cell);
    s.items.push(Item::fill(frame.clone(), style.void));

    let sol = extended_solution(strands, opening.side);
    let mut tubes = vec![];
    for st in strands {
        if st.solution {
            poly(&sol, false, &mut tubes);
        } else {
            poly(&st.pts, st.closed, &mut tubes);
        }
    }
    if style.outline {
        // black halo under the tubes; survives only where no tube covers it
        s.items.push(Item::stroke(
            tubes.clone(),
            round(Color::BLACK, (2.0 * RMAX + WALL) * cell),
        ));
    }
    s.items
        .push(Item::stroke(tubes, round(Color::WHITE, 2.0 * RMAX * cell)));
    if style.show_solution {
        let mut d = vec![];
        poly(&sol, false, &mut d);
        s.items.push(Item::stroke(
            d,
            round(style.fill, (2.0 * RMAX - WALL) * 0.9 * cell),
        ));
    }
    let mut d = vec![];
    for &(a, b) in walls {
        poly(&[a, b], false, &mut d);
    }
    s.items
        .push(Item::stroke(d, round(Color::BLACK, WALL * cell)));
    s.items.push(Item::stroke(
        frame,
        Stroke {
            color: Color::BLACK,
            width: WALL * cell,
            cap: Cap::Butt,
            join: Join::Miter,
        },
    ));
    if opening.side.is_some() {
        let n = sol.len();
        let mut d = vec![];
        poly(&[sol[1], sol[0]], false, &mut d);
        poly(&[sol[n - 2], sol[n - 1]], false, &mut d);
        s.items.push(Item::stroke(
            d,
            Stroke {
                color: Color::WHITE,
                width: (2.0 * RMAX - WALL) * cell,
                cap: Cap::Butt,
                join: Join::Miter,
            },
        ));
    }
    // outside the frame: clear of the stub that pokes 0.3 through it
    let gap = if opening.side.is_some() { 0.45 } else { 0.25 };
    let arrow = entrance_arrow(opening, rows, cols, cell, gap);
    s.items
        .push(arrow_item(&arrow, |(x, y)| (o + x * cell, o + y * cell)));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opening(side: Side, a: (usize, usize), b: (usize, usize)) -> Opening {
        Opening {
            entrance: a,
            exit: b,
            side: Some(side),
        }
    }

    #[test]
    fn entrance_arrow_points_in_from_outside_the_frame() {
        let (h, w) = (10, 8);
        for (side, a, b) in [
            (Side::S, (9, 3), (9, 4)),
            (Side::N, (0, 3), (0, 4)),
            (Side::W, (3, 0), (4, 0)),
            (Side::E, (3, 7), (4, 7)),
        ] {
            let p = entrance_arrow(&opening(side, a, b), h, w, 20.0, 0.25);
            let mid = (
                (a.1 + b.1) as f64 / 2.0 + 0.5,
                (a.0 + b.0) as f64 / 2.0 + 0.5,
            );
            let out = side.out();
            let (edge, vertical) = match side {
                Side::N => (0.0, true),
                Side::S => (h as f64, true),
                Side::W => (0.0, false),
                Side::E => (w as f64, false),
            };
            let along = |q: (f64, f64)| if vertical { q.1 } else { q.0 };
            let across = |q: (f64, f64)| if vertical { q.0 } else { q.1 };
            let o = if vertical { out.1 } else { out.0 };
            let dist = |q| (along(q) - edge) * o;
            // every point is outside the frame, on the entrance side
            assert!(p.iter().all(|&q| dist(q) > 0.0), "{side:?}");
            // the tip is closest to the frame, centred on the gap
            assert!((across(p[0]) - across(mid)).abs() < 1e-9);
            assert!(p[1..].iter().all(|&q| dist(q) > dist(p[0])));
            // within the drawing margin
            assert!(p.iter().all(|&q| dist(q) <= MARGIN));
        }
    }

    #[test]
    fn entrance_arrow_has_absolute_size_cap() {
        // at big corridors (Easy) the arrow must not run into the paper edge
        for c in [10.0, 31.2, 200.0] {
            for g in [0.25, 0.45] {
                let p = entrance_arrow(&opening(Side::S, (9, 3), (9, 4)), 10, 8, c, g);
                let ext = (p.iter().map(|q| q.1).fold(f64::MIN, f64::max) - 10.0) * c;
                assert!(ext <= 22.0, "{c} {g}: {ext}");
            }
        }
    }

    #[test]
    fn runs_merge_consecutive_cells() {
        let v = [true, true, false, true, false, false, true];
        assert_eq!(runs(7, |k| v[k]), [(0, 2), (3, 4), (6, 7)]);
    }
}
