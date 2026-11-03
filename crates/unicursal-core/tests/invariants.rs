//! Port of tests/test_invariants.py: the structural guarantees on fixed
//! pictures and seeds. Properties over arbitrary masks are in properties.rs.

mod common;

use common::*;
use unicursal_core::mask::{components, largest_component, load_mask};
use unicursal_core::maze::{build_labyrinth, build_maze};
use unicursal_core::{Mode, Params, Pcg32, generate, params};

#[test]
fn labyrinth_has_no_junctions_and_solution_is_the_picture() {
    let fg = picture(&ring(), 20);
    for seed in 0..5 {
        check_labyrinth(&build_labyrinth(&fg, &mut Pcg32::from_seed(seed)));
    }
}

#[test]
fn labyrinth_background_regions_are_single_loops() {
    let fg = picture(&ring(), 20);
    let b = build_labyrinth(&fg, &mut Pcg32::from_seed(1));
    assert_eq!(components(&b.fine_fg.not()).1, 2, "outside + the hole");
    check_background_loops(&b);
}

#[test]
fn maze_is_spanning_tree_with_picture_solution() {
    let fg = picture(&ring(), 20);
    for seed in 0..3 {
        check_maze(&build_maze(&fg, &mut Pcg32::from_seed(seed), true));
    }
}

#[test]
fn bridging_joins_pieces() {
    let raw = load_mask(&two_blobs(), 30, 128, false);
    assert_eq!(components(&raw).1, 2);
    let (fg, notes) = largest_component(&raw, true).unwrap();
    assert_eq!(components(&fg).1, 1);
    assert!(
        raw.cells.iter().zip(&fg.cells).all(|(&r, &f)| !r || f),
        "only adds"
    );
    assert!(fg.count() > raw.count());
    assert_eq!(notes.len(), 1);
}

#[test]
fn without_bridges_the_largest_piece_is_kept() {
    let raw = load_mask(&two_blobs(), 30, 128, false);
    let (fg, _) = largest_component(&raw, false).unwrap();
    assert_eq!(components(&fg).1, 1);
    assert!(
        fg.cells.iter().zip(&raw.cells).all(|(&f, &r)| !f || r),
        "only removes"
    );
}

#[test]
fn maze_deadend_fix_keeps_solution_and_tree() {
    // regression for #7: the dead-end walk used to run onto the solution path
    let fg = picture(&shape("ghost"), 15);
    for seed in 0..10 {
        check_maze(&build_maze(&fg, &mut Pcg32::from_seed(seed), true));
        check_maze(&build_maze(&fg, &mut Pcg32::from_seed(seed), false));
    }
}

#[test]
fn deadend_fix_reduces_dead_ends() {
    let fg = picture(&shape("ghost"), 15);
    let dead_ends = |fix: bool| -> usize {
        (0..10)
            .map(|seed| {
                let b = build_maze(&fg, &mut Pcg32::from_seed(seed), fix);
                all_cells(&b.grid)
                    .filter(|&p| b.grid.degree(p) == 1)
                    .count()
            })
            .sum()
    };
    assert!(dead_ends(true) < dead_ends(false));
}

#[test]
fn generate_draws_one_entrance_arrow_on_true_scale_pages() {
    let img = ring();
    for (mode, seed) in [(Mode::Labyrinth, 0), (Mode::Labyrinth, 1), (Mode::Maze, 2)] {
        let p = Params {
            mode,
            seed,
            ..Params::default()
        };
        let pz = generate(&img, &p).unwrap();
        for page in [&pz.puzzle, &pz.solution] {
            let svg = page.to_svg();
            assert_eq!(svg.matches("class=\"entrance\"").count(), 1);
            assert!(svg.contains("width=\"612.00pt\" height=\"792.00pt\""));
            assert!(svg.contains(&pz.footer));
        }
        assert!(pz.solution.to_svg().contains("fill=\"#e0218a\""));
        assert!(!pz.puzzle.to_svg().contains("fill=\"#e0218a\""));
    }
}

#[test]
fn preset_sizes_follow_the_corridor() {
    // DECISIONS.md 14: on Letter the frame fills the printable area; ghost at
    // Easy 7x9 -> 8x11, Medium 12x16 -> 13x17, Hard 30x40 -> 31x41.
    let img = shape("ghost");
    let presets = params::parse_presets(params::DEFAULT_PRESETS).unwrap();
    let want = [(8, 11), (13, 17), (31, 41)];
    for (preset, want) in presets.iter().zip(want) {
        let mut p = Params::default();
        preset.apply(&mut p).unwrap();
        let pz = generate(&img, &p).unwrap();
        assert_eq!(pz.picture_cells(), want, "{}", preset.name);
    }
}

#[test]
fn landscape_pictures_get_landscape_paper() {
    let pz = generate(&two_blobs(), &Params::default()).unwrap();
    assert!(pz.paper_mm.0 > pz.paper_mm.1);
    let p = Params {
        orientation: unicursal_core::Orientation::Portrait,
        ..Params::default()
    };
    assert!(generate(&two_blobs(), &p).unwrap().paper_mm.0 < 279.0);
}

#[test]
fn errors_are_reported_not_panicked() {
    let blank = draw(10, 10, |_, _| None);
    assert!(generate(&blank, &Params::default()).is_err());
    let p = Params {
        corridor: 200.0,
        ..Params::default()
    };
    assert!(generate(&ring(), &p).is_err());
    let p = Params {
        flow: true,
        mode: Mode::Maze,
        ..Params::default()
    };
    assert!(generate(&ring(), &p).is_err());
    let p = Params {
        fill: "pink".into(),
        ..Params::default()
    };
    assert!(generate(&ring(), &p).is_err());
}
