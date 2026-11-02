//! unicursal core: a picture becomes a printable labyrinth (or maze) whose
//! solution, filled in, reveals the picture.
//!
//! Pure library: a greyscale buffer and [`Params`] in; geometry, SVG text and
//! PDF bytes out. No file system, no threads required, and deterministic:
//! the same inputs give bit-identical outputs on every target (#5, #10), so
//! a puzzle link can regenerate its puzzle. Anything that changes outputs for
//! an existing seed must bump [`GENERATOR_VERSION`].

pub mod mask;
pub mod maze;
pub mod page;
pub mod params;
pub mod render;
pub mod rng;
pub mod scene;

use std::fmt;

pub use mask::{GreyImage, Mask};
pub use maze::{Built, Grid, Opening, Side};
pub use params::{Mode, Orientation, Params, Preset};
pub use rng::Pcg32;
pub use scene::{Color, Scene};

/// Version of the generator's output for a given input. Bump it whenever a
/// change alters any golden hash (`tests/golden.rs`).
pub const GENERATOR_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    BadImage(String),
    EmptyPicture,
    BadPaper(String),
    BadColor(String),
    BadOption(String),
    BadPreset(String),
    TooSmall {
        pitch: f64,
        cols: usize,
        rows: usize,
    },
    /// A style the port doesn't have yet (#10 milestones).
    NotYetPorted(&'static str),
    /// A broken invariant: a bug.
    Internal(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadImage(m) => write!(f, "bad image: {m}"),
            Error::EmptyPicture => write!(f, "image has no foreground pixels"),
            Error::BadPaper(s) => write!(
                f,
                "unknown paper '{s}' (try letter, a4, or e.g. 8.5x11in / 200x200mm)"
            ),
            Error::BadColor(s) => write!(f, "bad colour '{s}' (use #rgb or #rrggbb)"),
            Error::BadOption(m) => write!(f, "bad option: {m}"),
            Error::BadPreset(m) => write!(f, "bad preset: {m}"),
            Error::TooSmall { pitch, cols, rows } => write!(
                f,
                "{pitch} mm corridors leave only {cols}x{rows} picture cells on this paper; \
                 use a smaller corridor or bigger paper"
            ),
            Error::NotYetPorted(s) => write!(f, "{s} is not in the Rust core yet"),
            Error::Internal(m) => write!(f, "internal error: {m}"),
        }
    }
}

impl std::error::Error for Error {}

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
    if p.flow {
        return Err(Error::NotYetPorted("the flow style"));
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
    let page_of = |solution: Option<&Mask>| {
        let drawing = render::render_grid(&built, cell, solution, fill);
        page::place_on_page(&drawing, paper, Some(&footer), footer_top, p.paper_margin)
    };
    let puzzle = page_of(None);
    let solution = page_of(Some(&sol));
    Ok(Puzzle {
        built,
        seed: p.seed,
        paper_mm: paper,
        pitch_mm: pitch,
        puzzle,
        solution,
        footer,
        notes,
    })
}
