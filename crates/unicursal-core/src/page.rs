//! Paper sizes, fitting the maze to the printable area, and the printed page
//! (maze centred at true scale, 5 cm check bar and footer in the margin).

use crate::Error;
use crate::scene::{Anchor, Cap, Cmd, Color, Item, Join, Scene, Stroke};

pub const PT_PER_MM: f64 = 72.0 / 25.4;

/// Frame-to-edge-of-drawing margin, in corridors. Holds only the entrance
/// arrow, so it may overlap the paper margin: sizing fits the *frame* to the
/// printable area (see [`fit_grid`]).
pub const MARGIN: f64 = 1.5;

/// Printed check line: if it doesn't measure this, the page was scaled.
pub const SCALE_BAR_MM: f64 = 50.0;

pub const PAPERS: [(&str, (f64, f64)); 5] = [
    ("letter", (215.9, 279.4)),
    ("legal", (215.9, 355.6)),
    ("tabloid", (279.4, 431.8)),
    ("a4", (210.0, 297.0)),
    ("a3", (297.0, 420.0)),
];

/// A named paper (`letter`, `a4`, ...) or `<w>x<h>[mm|in]` → (width, height) mm.
pub fn parse_paper(spec: &str) -> Result<(f64, f64), Error> {
    let spec = spec.trim().to_lowercase();
    if let Some((_, size)) = PAPERS.iter().find(|(n, _)| *n == spec) {
        return Ok(*size);
    }
    let bad = || Error::BadPaper(spec.clone());
    let (body, k) = if let Some(b) = spec.strip_suffix("in") {
        (b, 25.4)
    } else if let Some(b) = spec.strip_suffix("mm") {
        (b, 1.0)
    } else {
        (spec.as_str(), 1.0)
    };
    let (w, h) = body.split_once('x').ok_or_else(bad)?;
    let num = |s: &str| -> Result<f64, Error> {
        let s = s.trim();
        if s.is_empty() || !s.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return Err(bad());
        }
        s.parse::<f64>().map_err(|_| bad())
    };
    let (w, h) = (num(w)? * k, num(h)? * k);
    if w <= 0.0 || h <= 0.0 {
        return Err(bad());
    }
    Ok((w, h))
}

/// Largest picture-cell grid (cols, rows) whose frame fits in `avail` mm at
/// corridor spacing `pitch` mm (each picture cell is 2 corridors).
pub fn fit_grid(avail: (f64, f64), pitch: f64) -> (usize, usize) {
    (
        ((avail.0 / pitch) as usize) / 2,
        ((avail.1 / pitch) as usize) / 2,
    )
}

/// Largest cells-on-the-long-side whose grid fits in `ncols × nrows`; 0 if none.
pub fn cells_to_fit(img_w: usize, img_h: usize, ncols: usize, nrows: usize) -> usize {
    (1..=ncols.max(nrows))
        .rev()
        .find(|&c| {
            let (m, n) = crate::mask::grid_for(img_w, img_h, c);
            n <= ncols && m <= nrows
        })
        .unwrap_or(0)
}

/// Centre a drawing on a page of `paper_mm` at true scale. The footer line
/// (a 5 cm check bar at the left, `footer` at the right) sits in the paper
/// margin on the side away from the entrance, and is drawn last: the drawing
/// may extend into the margin.
pub fn place_on_page(
    drawing: &Scene,
    paper_mm: (f64, f64),
    footer: Option<&str>,
    footer_top: bool,
    margin_mm: f64,
) -> Scene {
    let (pw, ph) = (paper_mm.0 * PT_PER_MM, paper_mm.1 * PT_PER_MM);
    let mut page = Scene::new(pw, ph);
    page.place(
        drawing,
        (pw - drawing.width) / 2.0,
        (ph - drawing.height) / 2.0,
    );
    let fy = if footer_top { 19.0 } else { ph - 14.0 }; // text baseline
    let (left, right) = (margin_mm * PT_PER_MM, pw - margin_mm * PT_PER_MM);
    let grey = Color(0x88, 0x88, 0x88);
    let (bar, by) = (SCALE_BAR_MM * PT_PER_MM, fy - 2.5);
    page.items.push(
        Item::stroke(
            vec![
                Cmd::M(left, by - 3.0),
                Cmd::L(left, by + 3.0),
                Cmd::M(left, by),
                Cmd::L(left + bar, by),
                Cmd::M(left + bar, by - 3.0),
                Cmd::L(left + bar, by + 3.0),
            ],
            Stroke {
                color: grey,
                width: 0.8,
                cap: Cap::Butt,
                join: Join::Miter,
            },
        )
        .with_class("scale-bar"),
    );
    let text = |x, anchor, text: &str| Item::Text {
        x,
        y: fy,
        size: 7.0,
        anchor,
        color: grey,
        text: text.to_string(),
    };
    page.items.push(text(
        left + bar + 5.0,
        Anchor::Start,
        "5 cm. If not, print at 100% / actual size",
    ));
    if let Some(f) = footer {
        page.items.push(text(right, Anchor::End, f));
    }
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_sizing() {
        assert_eq!(parse_paper("letter").unwrap(), (215.9, 279.4));
        assert_eq!(parse_paper(" A4 ").unwrap(), (210.0, 297.0));
        let (w, h) = parse_paper("8.5x11in").unwrap();
        assert!((w - 215.9).abs() < 1e-6 && (h - 279.4).abs() < 1e-6);
        assert_eq!(parse_paper("200 x 250 mm").unwrap(), (200.0, 250.0));
        assert!(parse_paper("napkin").is_err());
        assert!(parse_paper("-3x4").is_err());
        // ghost case: the frame fills the printable area of letter at 7 mm
        let avail = (215.9 - 2.0 * 12.7, 279.4 - 2.0 * 12.7);
        let (nmax, mmax) = fit_grid(avail, 7.0);
        assert_eq!((nmax, mmax), (13, 18));
        assert!(2.0 * nmax as f64 * 7.0 <= avail.0 && 2.0 * mmax as f64 * 7.0 <= avail.1);
        assert_eq!(cells_to_fit(240, 320, nmax, mmax), 17);
    }

    #[test]
    fn page_is_true_scale_with_a_5cm_bar_drawn_last() {
        let mut d = Scene::new(100.0, 50.0);
        d.items.push(Item::fill(
            crate::scene::rect(0.0, 0.0, 1.0, 1.0),
            Color::BLACK,
        ));
        let page = place_on_page(&d, (215.9, 279.4), Some("x"), false, 12.7);
        let svg = page.to_svg();
        assert!(
            svg.contains("width=\"612.00pt\" height=\"792.00pt\""),
            "{svg}"
        );
        let bar = page
            .items
            .iter()
            .position(|i| {
                matches!(
                    i,
                    Item::Path {
                        class: Some("scale-bar"),
                        ..
                    }
                )
            })
            .unwrap();
        assert!(bar > 0, "bar drawn after the maze");
        let Item::Path { d, .. } = &page.items[bar] else {
            unreachable!()
        };
        let (Cmd::M(x0, _), Cmd::L(x1, _)) = (d[2], d[3]) else {
            unreachable!()
        };
        assert!((x1 - x0 - 50.0 * PT_PER_MM).abs() < 1e-9);
    }
}
