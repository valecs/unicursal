//! `unicursal`: command-line front end to the core, with the Python
//! prototype's flags (`picmaze.py`). Decodes the image, writes the files.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
use unicursal_core::params::{self, Preset};
use unicursal_core::{GreyImage, Mode, Orientation, Params, generate};

const ABOUT: &str = "Picture labyrinths: a silhouette becomes a printable maze whose solution, \
filled in, reveals the picture.";

const LONG_ABOUT: &str = "\
Picture labyrinths: a silhouette becomes a printable maze whose solution, filled in, reveals \
the picture.

Size: the main knob is --corridor, the spacing of corridors on paper in mm. Together with --paper \
it sets how many corridors fit, and hence how detailed the picture and how hard the puzzle. \
Rough guide:
   4 mm  sharp pencil / fine pen        (very detailed, long solve)
   6 mm  dull pencil, fine marker
   8 mm  classic markers, crayons       (default)
  11 mm  chunky markers, little kids
Output is a PDF at true scale for the chosen paper: print at 100%.

Writes <out>.svg/.pdf (puzzle) and <out>_solution.svg/.pdf (answer key).";

#[derive(Parser, Debug)]
#[command(name = "unicursal", version, about = ABOUT, long_about = LONG_ABOUT)]
struct Cli {
    /// Picture (PNG, JPEG, GIF, BMP or WebP); dark pixels are the picture
    #[arg(required_unless_present = "list_presets")]
    image: Option<PathBuf>,
    /// Difficulty preset (e.g. easy, medium, hard); explicit flags override it
    #[arg(long)]
    preset: Option<String>,
    /// Presets file [default: the built-in presets.toml]
    #[arg(long)]
    presets: Option<PathBuf>,
    /// List presets and exit
    #[arg(long)]
    list_presets: bool,
    /// Corridor spacing on paper, mm (sets size and difficulty) [default: 8]
    #[arg(long)]
    corridor: Option<f64>,
    /// letter, legal, tabloid, a4, a3, or e.g. 8.5x11in, 200x250mm [default: letter]
    #[arg(long)]
    paper: Option<String>,
    /// Page orientation; auto follows the picture [default: auto]
    #[arg(long, value_enum)]
    orientation: Option<OrientationArg>,
    /// Blank margin at the paper edge, mm [default: 12.7]
    #[arg(long)]
    paper_margin: Option<f64>,
    /// Override: fixed picture resolution (long side, before 2x2 doubling);
    /// corridor size then follows from the paper
    #[arg(long)]
    cells: Option<usize>,
    /// [default: labyrinth]
    #[arg(long, value_enum)]
    mode: Option<ModeArg>,
    /// [default: random]
    #[arg(long)]
    seed: Option<u64>,
    /// Pixels darker than this are the picture [default: 128]
    #[arg(long)]
    threshold: Option<u8>,
    /// Treat light pixels as the picture
    #[arg(long)]
    invert: bool,
    /// Keep only the largest piece instead of joining pieces
    #[arg(long)]
    no_bridges: bool,
    /// Maze mode: skip the dead-end pairing heuristic (on by default: fewer,
    /// paired dead ends)
    #[arg(long)]
    no_deadend_fix: bool,
    /// Solution fill colour, #rgb or #rrggbb [default: #e0218a]
    #[arg(long)]
    fill: Option<String>,
    /// Round the corners and bend the sheet so paths curve (not ported yet)
    #[arg(long)]
    curvy: bool,
    /// Curvy: max local stretch/squeeze, 0..<1 [default: 0.55]
    #[arg(long)]
    warp: Option<f64>,
    /// Curvy: typical bend wavelength, in maze cells [default: 14]
    #[arg(long)]
    warp_scale: Option<f64>,
    /// Curvy: corner-rounding passes (0 = sharp) [default: 3]
    #[arg(long)]
    smooth: Option<u32>,
    /// Labyrinth only: relax the corridors into flowing curves
    #[arg(long)]
    flow: bool,
    /// [default: 1500]
    #[arg(long)]
    flow_iters: Option<u32>,
    /// Flow: corridor length kept; <1 opens voids (0.7-1.0) [default: 0.85]
    #[arg(long)]
    flow_length: Option<f64>,
    /// Flow: stiffness; higher = broader, smoother curves [default: 0.3]
    #[arg(long)]
    flow_bend: Option<f64>,
    /// Flow: how far (in corridors) the picture's edge may drift [default: 1]
    #[arg(long)]
    flow_tol: Option<f64>,
    /// Flow: colour of the voids [default: #111]
    #[arg(long)]
    void: Option<String>,
    /// Flow: light grey voids with outlined corridors
    #[arg(long)]
    ink_saver: bool,
    /// Output file stem
    #[arg(long, default_value = "picmaze")]
    out: PathBuf,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OrientationArg {
    Auto,
    Portrait,
    Landscape,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ModeArg {
    Labyrinth,
    Maze,
}

fn load_presets(cli: &Cli) -> Result<Vec<Preset>> {
    let text = match &cli.presets {
        Some(path) => {
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?
        }
        None => params::DEFAULT_PRESETS.to_string(),
    };
    Ok(params::parse_presets(&text)?)
}

/// Defaults, then the preset, then explicit flags.
fn params(cli: &Cli) -> Result<Params> {
    let mut p = Params::default();
    let mut preset_seed = false;
    if let Some(name) = &cli.preset {
        let presets = load_presets(cli)?;
        let Some(preset) = presets.iter().find(|p| &p.name == name) else {
            let have: Vec<_> = presets.iter().map(|p| p.name.as_str()).collect();
            bail!("unknown preset '{name}' (have: {})", have.join(", "));
        };
        preset.apply(&mut p)?;
        preset_seed = preset.values.iter().any(|(k, _)| k == "seed");
    }
    macro_rules! flag {
        ($($f:ident),*) => { $(if let Some(v) = cli.$f.clone() { p.$f = v; })* };
    }
    flag!(
        corridor,
        paper,
        paper_margin,
        threshold,
        fill,
        warp,
        warp_scale,
        smooth
    );
    flag!(flow_iters, flow_length, flow_bend, flow_tol, void);
    if let Some(c) = cli.cells {
        p.cells = Some(c);
    }
    if let Some(s) = cli.seed {
        p.seed = s;
    } else if !preset_seed {
        p.seed = random_seed();
    }
    if let Some(o) = cli.orientation {
        p.orientation = match o {
            OrientationArg::Auto => Orientation::Auto,
            OrientationArg::Portrait => Orientation::Portrait,
            OrientationArg::Landscape => Orientation::Landscape,
        };
    }
    if let Some(m) = cli.mode {
        p.mode = match m {
            ModeArg::Labyrinth => Mode::Labyrinth,
            ModeArg::Maze => Mode::Maze,
        };
    }
    p.invert |= cli.invert;
    p.no_bridges |= cli.no_bridges;
    p.deadend_fix &= !cli.no_deadend_fix;
    p.curvy |= cli.curvy;
    p.flow |= cli.flow;
    p.ink_saver |= cli.ink_saver;
    if p.ink_saver && cli.void.is_none() && p.void == Params::default().void {
        p.void = "#cfcfcf".into();
    }
    Ok(p)
}

/// A fresh seed below 10^6 (short enough to read off the footer).
fn random_seed() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    h.finish() % 1_000_000
}

fn decode(path: &PathBuf) -> Result<GreyImage> {
    let img = image::open(path)
        .with_context(|| format!("reading {}", path.display()))?
        .to_rgba8();
    let (w, h) = img.dimensions();
    Ok(GreyImage::from_rgba(w as usize, h as usize, img.as_raw())?)
}

fn run(cli: Cli) -> Result<()> {
    if cli.list_presets {
        for p in load_presets(&cli)? {
            let opts: Vec<String> = p.values.iter().map(|(k, v)| format!("{k}={v}")).collect();
            println!(
                "{:10} {:8} {:26} {}",
                p.name,
                p.label.as_deref().unwrap_or(""),
                p.tool.as_deref().unwrap_or(""),
                opts.join(", ")
            );
        }
        return Ok(());
    }
    let params = params(&cli)?;
    let image = cli.image.as_ref().expect("clap requires an image");
    let pz = generate(&decode(image)?, &params)?;
    for note in &pz.notes {
        eprintln!("note: {note}");
    }
    let (cols, rows) = pz.picture_cells();
    eprintln!(
        "paper {:.0}x{:.0} mm, picture {cols}x{rows} cells, corridor spacing {:.2} mm, seed {}",
        pz.paper_mm.0, pz.paper_mm.1, pz.pitch_mm, pz.seed
    );
    let stem = cli.out.as_os_str().to_string_lossy().into_owned();
    for (name, page) in [
        (stem.clone(), &pz.puzzle),
        (format!("{stem}_solution"), &pz.solution),
    ] {
        std::fs::write(format!("{name}.svg"), page.to_svg())
            .with_context(|| format!("writing {name}.svg"))?;
        std::fs::write(format!("{name}.pdf"), page.to_pdf())
            .with_context(|| format!("writing {name}.pdf"))?;
        println!("wrote {name}.svg and {name}.pdf");
    }
    if let Some(fr) = &pz.flow {
        let (narrowest, p1, median) = fr.width_mm;
        eprintln!(
            "clear corridor width: narrowest {narrowest:.1} mm, 1% narrower than {p1:.1} mm, \
             typical {median:.1} mm"
        );
        eprintln!(
            "{}{} open regions for {} strands, {} crossings",
            if fr.verified() {
                "verified: "
            } else {
                "WARNING: "
            },
            fr.open_regions,
            fr.strands.len(),
            fr.crossings
        );
    } else {
        eprintln!(
            "clear corridor width: {:.1} mm typical",
            pz.pitch_mm * (1.0 - 0.18)
        );
    }
    println!(
        "grid {}x{}, path length {}",
        pz.built.grid.rows,
        pz.built.grid.cols,
        pz.built.fine_fg.count()
    );
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
