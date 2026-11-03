//! Golden hashes: fixed (picture, settings, seed) → SHA-256 of the
//! construction and of the rendered pages (SVG and PDF). Puzzle links
//! regenerate puzzles from these inputs (#5), so the hashes must be identical
//! on every target; CI runs this on x86-64, aarch64 and wasm32.
//!
//! A change that alters any hash changes existing puzzles: bump
//! `GENERATOR_VERSION`, then re-bless with
//! `UNICURSAL_BLESS=1 cargo test -p unicursal-core --test golden`
//! and say so in the PR.

mod common;

use sha2::{Digest, Sha256};
use unicursal_core::{GENERATOR_VERSION, GreyImage, Mode, Orientation, Params, Puzzle, generate};

const GOLDEN: &str = include_str!("golden.txt");

fn cases() -> Vec<(&'static str, GreyImage, Params)> {
    let p = Params::default;
    vec![
        (
            "ghost-medium-labyrinth-3",
            common::shape("ghost"),
            Params {
                corridor: 7.0,
                seed: 3,
                ..p()
            },
        ),
        (
            "ghost-easy-labyrinth-1",
            common::shape("ghost"),
            Params {
                corridor: 11.0,
                seed: 1,
                ..p()
            },
        ),
        (
            "cat-hard-maze-7",
            common::shape("cat"),
            Params {
                corridor: 3.0,
                mode: Mode::Maze,
                seed: 7,
                ..p()
            },
        ),
        (
            "cat-medium-maze-nofix-7",
            common::shape("cat"),
            Params {
                corridor: 7.0,
                mode: Mode::Maze,
                deadend_fix: false,
                seed: 7,
                ..p()
            },
        ),
        (
            "hi-a4-labyrinth-5",
            common::shape("hi"),
            Params {
                paper: "a4".into(),
                seed: 5,
                ..p()
            },
        ),
        (
            "hi-nobridges-maze-5",
            common::shape("hi"),
            Params {
                no_bridges: true,
                mode: Mode::Maze,
                seed: 5,
                ..p()
            },
        ),
        (
            "witch-cells40-labyrinth-19",
            common::shape("witch"),
            Params {
                cells: Some(40),
                seed: 19,
                ..p()
            },
        ),
        (
            "skull-a3-landscape-2",
            common::shape("skull"),
            Params {
                paper: "a3".into(),
                orientation: Orientation::Landscape,
                corridor: 5.0,
                seed: 2,
                ..p()
            },
        ),
        (
            "jack-labyrinth-bigseed",
            common::shape("jack"),
            Params {
                seed: u64::MAX - 12345,
                fill: "#f80".into(),
                ..p()
            },
        ),
        (
            "heart-inverted-maze-0",
            common::shape("heart"),
            Params {
                invert: true,
                mode: Mode::Maze,
                seed: 0,
                ..p()
            },
        ),
        (
            "ring-cells14-labyrinth-4",
            common::ring(),
            Params {
                cells: Some(14),
                seed: 4,
                ..p()
            },
        ),
        (
            "ghost-medium-flow-3",
            common::shape("ghost"),
            Params {
                corridor: 7.0,
                flow: true,
                seed: 3,
                ..p()
            },
        ),
        (
            "ring-cells14-flow-4",
            common::ring(),
            Params {
                cells: Some(14),
                flow: true,
                seed: 4,
                ..p()
            },
        ),
        (
            "witch-cells24-flow-inksaver-19",
            common::shape("witch"),
            Params {
                cells: Some(24),
                flow: true,
                ink_saver: true,
                void: "#cfcfcf".into(),
                flow_tol: 0.4,
                seed: 19,
                ..p()
            },
        ),
        (
            "skull-flow-loopy-7",
            common::shape("skull"),
            Params {
                corridor: 9.0,
                flow: true,
                flow_length: 0.7,
                flow_bend: 0.5,
                flow_iters: 900,
                seed: 7,
                ..p()
            },
        ),
    ]
}

fn hex(d: impl AsRef<[u8]>) -> String {
    d.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

fn hashes(pz: &Puzzle) -> (String, String) {
    let b = &pz.built;
    let mut h = Sha256::new();
    h.update(format!("{}x{} {:?}\n", b.grid.rows, b.grid.cols, b.opening));
    h.update(
        b.grid
            .hp
            .iter()
            .chain(&b.grid.vp)
            .map(|&x| x as u8)
            .collect::<Vec<_>>(),
    );
    if let Some(fr) = &pz.flow {
        // the relaxed geometry to the last bit, not just as printed
        for s in &fr.strands {
            for p in &s.pts {
                h.update(p[0].to_bits().to_le_bytes());
                h.update(p[1].to_bits().to_le_bytes());
            }
        }
        h.update(format!(
            "{} {} {}",
            fr.walls.len(),
            fr.crossings,
            fr.open_regions
        ));
    }
    let construction = hex(h.finalize());
    let mut h = Sha256::new();
    for page in [&pz.puzzle, &pz.solution] {
        h.update(page.to_svg());
        h.update(page.to_pdf());
    }
    (construction, hex(h.finalize()))
}

#[test]
fn golden_hashes() {
    let got: Vec<String> = cases()
        .into_iter()
        .map(|(name, img, params)| {
            let (c, p) = hashes(&generate(&img, &params).unwrap());
            format!("{name} {c} {p}")
        })
        .collect();
    let header = format!("# generator version {GENERATOR_VERSION}: name construction pages");
    let text = format!("{header}\n{}\n", got.join("\n"));
    if std::env::var_os("UNICURSAL_BLESS").is_some() {
        std::fs::write(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden.txt"),
            &text,
        )
        .unwrap();
        return;
    }
    let want: Vec<&str> = GOLDEN.lines().collect();
    assert_eq!(
        want.first().copied(),
        Some(header.as_str()),
        "golden.txt is for another version"
    );
    let mut bad = vec![];
    for (g, w) in got.iter().zip(&want[1..]) {
        if g != w {
            bad.push(format!("  want {w}\n  got  {g}"));
        }
    }
    assert_eq!(got.len(), want.len() - 1, "number of cases");
    assert!(
        bad.is_empty(),
        "outputs changed for existing seeds:\n{}",
        bad.join("\n")
    );
}
