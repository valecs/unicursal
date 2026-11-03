//! unicursal core: a picture becomes a printable labyrinth (or maze) whose
//! solution, filled in, reveals the picture.
//!
//! Pure library: a greyscale buffer and [`Params`] in; geometry, SVG text and
//! PDF bytes out. No file system, no threads required, and deterministic:
//! the same inputs give bit-identical outputs on every target (#5, #10), so
//! a puzzle link can regenerate its puzzle. Anything that changes outputs for
//! an existing seed must bump [`GENERATOR_VERSION`].

pub mod field;
pub mod flow;
pub mod mask;
pub mod maze;
pub mod page;
pub mod params;
pub mod render;
pub mod rng;
pub mod scene;
pub mod walls;

pub use mask::{GreyImage, Mask};
pub use maze::{Built, Grid, Opening, Side};
pub use params::{Mode, Orientation, Params, Preset};
pub use rng::Pcg32;
pub use scene::{Color, Scene};

/// Version of the generator's output for a given input. Bump it whenever a
/// change alters any golden hash (`tests/golden.rs`).
pub const GENERATOR_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("bad image: {0}")]
    BadImage(String),
    #[error("image has no foreground pixels")]
    EmptyPicture,
    #[error("unknown paper '{0}' (try letter, a4, or e.g. 8.5x11in / 200x200mm)")]
    BadPaper(String),
    #[error("bad colour '{0}' (use #rgb or #rrggbb)")]
    BadColor(String),
    #[error("bad option: {0}")]
    BadOption(String),
    #[error("bad preset: {0}")]
    BadPreset(String),
    #[error(
        "{pitch} mm corridors leave only {cols}x{rows} picture cells on this paper; \
         use a smaller corridor or bigger paper"
    )]
    TooSmall {
        pitch: f64,
        cols: usize,
        rows: usize,
    },
    /// A style the port doesn't have yet (#10 milestones).
    #[error("{0} is not in the Rust core yet")]
    NotYetPorted(&'static str),
    /// A broken invariant: a bug.
    #[error("internal error: {0}")]
    Internal(String),
}

/// A generated puzzle: construction, sizes, and the two printable pages.
#[derive(Clone, Debug)]
pub struct Puzzle {
    pub built: Built,
    pub seed: u64,
    /// Paper actually used, (width, height) mm, after orientation.
    pub paper_mm: (f64, f64),
    /// Corridor spacing on paper, mm.
    pub pitch_mm: f64,
    /// The puzzle page.
    pub puzzle: Scene,
    /// The answer key page.
    pub solution: Scene,
    /// The one-line footer printed on both pages.
    pub footer: String,
    /// Notes for the user (bridges added, pieces dropped, ...).
    pub notes: Vec<String>,
    /// Flow style only: the relaxed geometry and its checks.
    pub flow: Option<FlowResult>,
}

/// The relaxed flow geometry and the checks every flow puzzle must pass.
#[derive(Clone, Debug)]
pub struct FlowResult {
    pub strands: Vec<flow::Strand>,
    pub walls: Vec<(flow::Pt, flow::Pt)>,
    /// Strand crossings (must be 0).
    pub crossings: usize,
    /// Open regions inside the frame (must equal the number of strands).
    pub open_regions: usize,
    /// Clear corridor width, mm: narrowest, 1st percentile, median.
    pub width_mm: (f64, f64, f64),
}

impl FlowResult {
    /// No crossings and no leaks.
    pub fn verified(&self) -> bool {
        self.crossings == 0 && self.open_regions == self.strands.len()
    }
}

/// Linear-interpolated percentile of sorted data (`np.percentile`).
fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let x = q / 100.0 * (sorted.len() - 1) as f64;
    let (i, f) = (x.floor() as usize, x - x.floor());
    let j = (i + 1).min(sorted.len() - 1);
    sorted[i] + (sorted[j] - sorted[i]) * f
}

/// Relaxation settings for a puzzle, from its options (as the prototype's
/// CLI sets them).
pub fn flow_params(p: &Params, seed: u64) -> flow::FlowParams {
    flow::FlowParams {
        iters: p.flow_iters,
        length: p.flow_length,
        bend: p.flow_bend,
        tol: p.flow_tol,
        pic: if p.flow_tol >= 1.0 { 0.3 } else { 0.6 },
        noise: 0.02,
        seed,
        ..flow::FlowParams::default()
    }
}

/// Relax a labyrinth into flow strands and check them: crossings, leaks
/// (rasterised at 30 px per corridor) and clear width (in corridors).
pub fn flow_check(built: &Built, strands: Vec<flow::Strand>) -> (FlowResult, Vec<f64>) {
    let (h, w) = (built.grid.rows, built.grid.cols);
    let walls = walls::flow_walls(&strands, h, w);
    let crossings = flow::count_crossings(&strands);
    let sol = render::extended_solution(&strands, built.opening.side);
    let stubs = if built.opening.side.is_some() {
        vec![(sol[1], sol[0]), (sol[sol.len() - 2], sol[sol.len() - 1])]
    } else {
        vec![]
    };
    let space = walls::OpenSpace {
        tubes: strands
            .iter()
            .map(|s| {
                if s.solution {
                    (sol.clone(), false)
                } else {
                    (s.pts.clone(), s.closed)
                }
            })
            .collect(),
        walls: &walls,
        stubs,
    };
    let open_regions = walls::open_regions(&space, h, w, 30);
    let widths = walls::corridor_widths(&strands, &walls, h, w);
    (
        FlowResult {
            strands,
            walls,
            crossings,
            open_regions,
            width_mm: (f64::NAN, f64::NAN, f64::NAN),
        },
        widths,
    )
}

impl Puzzle {
    /// Picture cells (cols, rows).
    pub fn picture_cells(&self) -> (usize, usize) {
        (self.built.grid.cols / 2, self.built.grid.rows / 2)
    }
}

/// Generate a puzzle from a picture.
pub fn generate(img: &GreyImage, p: &Params) -> Result<Puzzle, Error> {
    if p.flow && p.mode != Mode::Labyrinth {
        return Err(Error::BadOption(
            "flow needs mode labyrinth (it relies on there being no junctions)".into(),
        ));
    }
    if p.curvy {
        return Err(Error::NotYetPorted("the curvy style"));
    }
    let fill = Color::parse(&p.fill)?;
    let mut rng = Pcg32::from_seed(p.seed);

    let mut paper = page::parse_paper(&p.paper)?;
    let landscape = match p.orientation {
        Orientation::Auto => img.width > img.height,
        Orientation::Portrait => false,
        Orientation::Landscape => true,
    };
    if landscape != (paper.0 > paper.1) {
        paper = (paper.1, paper.0);
    }
    let avail = (
        paper.0 - 2.0 * p.paper_margin,
        paper.1 - 2.0 * p.paper_margin,
    );
    let (cells, pitch) = match p.cells {
        None => {
            if p.corridor.is_nan() || p.corridor <= 0.0 {
                return Err(Error::BadOption(format!("corridor = {}", p.corridor)));
            }
            let (cols, rows) = page::fit_grid(avail, p.corridor);
            let cells = page::cells_to_fit(img.width, img.height, cols, rows);
            if cells < 4 {
                return Err(Error::TooSmall {
                    pitch: p.corridor,
                    cols,
                    rows,
                });
            }
            (cells, Some(p.corridor))
        }
        Some(0) => return Err(Error::BadOption("cells = 0".into())),
        Some(c) => (c, None),
    };

    let raw = mask::load_mask(img, cells, p.threshold, p.invert);
    let (fg, mut notes) = mask::largest_component(&raw, !p.no_bridges)?;
    let built = match p.mode {
        Mode::Labyrinth => maze::build_labyrinth(&fg, &mut rng),
        Mode::Maze => maze::build_maze(&fg, &mut rng, p.deadend_fix),
    };
    notes.extend(built.notes.iter().cloned());

    // the solution is the open corridor from the entrance: exactly the picture
    let path = built.solution();
    let mut sol = Mask::new(built.grid.rows, built.grid.cols);
    for &(r, c) in &path {
        sol.set(r, c, true);
    }
    if sol != built.fine_fg {
        return Err(Error::Internal(
            "solution does not cover the picture".into(),
        ));
    }

    let (h, w) = (built.grid.rows, built.grid.cols);
    let pitch = pitch.unwrap_or_else(|| (avail.0 / w as f64).min(avail.1 / h as f64));
    let cell = pitch * page::PT_PER_MM;
    let footer = format!(
        "{}x{} cells · {pitch:.1} mm corridors · seed {}",
        w / 2,
        h / 2,
        p.seed
    );
    let footer_top = built.opening.side == Some(Side::S);
    let place = |drawing: &Scene| {
        page::place_on_page(drawing, paper, Some(&footer), footer_top, p.paper_margin)
    };
    let (puzzle, solution, flow) = if p.flow {
        let strands = flow::extract_strands(&built.grid, &path);
        let mut relax =
            flow::Relaxation::new(&strands, &built.fine_fg, flow_params(p, rng.derive_seed()));
        relax.run();
        let (mut fr, mut widths) = flow_check(&built, relax.strands());
        widths.sort_by(f64::total_cmp);
        fr.width_mm = (
            percentile(&widths, 0.0) * pitch,
            percentile(&widths, 1.0) * pitch,
            percentile(&widths, 50.0) * pitch,
        );
        if fr.crossings > 0 {
            notes.push(format!("{} strand crossings", fr.crossings));
        }
        if fr.open_regions != fr.strands.len() {
            notes.push(format!(
                "{} open regions for {} strands: a wall leaks",
                fr.open_regions,
                fr.strands.len()
            ));
        }
        let mut style = render::FlowStyle {
            void: Color::parse(&p.void)?,
            fill,
            show_solution: false,
            outline: p.ink_saver,
        };
        let draw = |style: &render::FlowStyle| {
            render::render_flow(&fr.strands, &fr.walls, &built.opening, h, w, cell, style)
        };
        let puzzle = place(&draw(&style));
        style.show_solution = true;
        let solution = place(&draw(&style));
        (puzzle, solution, Some(fr))
    } else {
        let grid_page = |s: Option<&Mask>| place(&render::render_grid(&built, cell, s, fill));
        (grid_page(None), grid_page(Some(&sol)), None)
    };
    Ok(Puzzle {
        built,
        seed: p.seed,
        paper_mm: paper,
        pitch_mm: pitch,
        puzzle,
        solution,
        footer,
        notes,
        flow,
    })
}

#[cfg(test)]
mod tests {
    use super::Error;

    #[test]
    fn error_messages_tell_the_user_what_to_do() {
        let e = Error::TooSmall {
            pitch: 40.0,
            cols: 2,
            rows: 3,
        };
        assert_eq!(
            e.to_string(),
            "40 mm corridors leave only 2x3 picture cells on this paper; \
             use a smaller corridor or bigger paper"
        );
        assert_eq!(
            Error::BadPaper("napkin".into()).to_string(),
            "unknown paper 'napkin' (try letter, a4, or e.g. 8.5x11in / 200x200mm)"
        );
        assert_eq!(
            Error::NotYetPorted("the curvy style").to_string(),
            "the curvy style is not in the Rust core yet"
        );
        let boxed: Box<dyn std::error::Error> = Box::new(Error::EmptyPicture);
        assert_eq!(boxed.to_string(), "image has no foreground pixels");
    }
}
