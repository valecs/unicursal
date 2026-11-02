//! Port of tests/test_properties.py: the core invariants over arbitrary small
//! masks and seeds. proptest shrinks any failure to a minimal picture; when it
//! finds one, add it to invariants.rs as a named regression.

mod common;

use common::*;
use proptest::prelude::*;
use unicursal_core::mask::{GreyImage, Mask, components, largest_component};
use unicursal_core::maze::{build_labyrinth, build_maze};
use unicursal_core::{Mode, Params, Pcg32, generate};

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 150,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

/// A picture-cell mask with at least one foreground cell (any shape, possibly
/// in several pieces), plus a seed.
fn pictures(max_side: usize) -> impl Strategy<Value = (Mask, u64)> {
    (1..=max_side, 1..=max_side)
        .prop_flat_map(|(h, w)| {
            (
                Just(h),
                Just(w),
                proptest::collection::vec(any::<bool>(), h * w),
                0..h * w,
                any::<u64>(),
            )
        })
        .prop_map(|(h, w, mut cells, fallback, seed)| {
            if !cells.iter().any(|&b| b) {
                cells[fallback] = true;
            }
            (
                Mask {
                    rows: h,
                    cols: w,
                    cells,
                },
                seed,
            )
        })
}

fn joined(mask: &Mask) -> Mask {
    largest_component(mask, true).unwrap().0
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn bridging_yields_one_piece_containing_the_original((mask, _) in pictures(9)) {
        let fg = joined(&mask);
        prop_assert_eq!(components(&fg).1, 1);
        prop_assert!(mask.cells.iter().zip(&fg.cells).all(|(&m, &f)| !m || f));
    }

    #[test]
    fn labyrinth_invariants((mask, seed) in pictures(9)) {
        let b = build_labyrinth(&joined(&mask), &mut Pcg32::from_seed(seed));
        check_labyrinth(&b);
        check_background_loops(&b);
    }

    /// Default maze (dead-end heuristic on). In Python this property found #7
    /// in seconds, shrunk to a one-cell picture.
    #[test]
    fn maze_invariants((mask, seed) in pictures(9)) {
        check_maze(&build_maze(&joined(&mask), &mut Pcg32::from_seed(seed), true));
    }

    #[test]
    fn maze_without_deadend_fix_invariants((mask, seed) in pictures(9)) {
        check_maze(&build_maze(&joined(&mask), &mut Pcg32::from_seed(seed), false));
    }

    /// Puzzle links (#5): the same inputs give the same puzzle.
    #[test]
    fn construction_is_deterministic((mask, seed) in pictures(9), maze in any::<bool>()) {
        let fg = joined(&mask);
        let build = || if maze {
            build_maze(&fg, &mut Pcg32::from_seed(seed), true)
        } else {
            build_labyrinth(&fg, &mut Pcg32::from_seed(seed))
        };
        prop_assert_eq!(build(), build());
    }

    /// The whole pipeline, from pixels and arbitrary paper, never panics and
    /// always yields a valid puzzle or a reported error.
    #[test]
    fn generate_succeeds_or_reports(
        (w, h) in (1usize..60, 1usize..60),
        dark in proptest::collection::vec(any::<bool>(), 64),
        seed in any::<u64>(),
        corridor in 2.0f64..40.0,
        paper in prop::sample::select(vec!["letter", "a4", "a3", "100x150mm", "4x6in"]),
        maze in any::<bool>(),
    ) {
        // an 8x8 pattern of blocks, stretched to w x h pixels
        let px = (0..w * h)
            .map(|i| if dark[(i / w) * 8 / h * 8 + (i % w) * 8 / w] { 0 } else { 255 })
            .collect();
        let img = GreyImage::new(w, h, px).unwrap();
        let p = Params {
            seed,
            corridor,
            paper: paper.into(),
            mode: if maze { Mode::Maze } else { Mode::Labyrinth },
            ..Params::default()
        };
        if let Ok(pz) = generate(&img, &p) {
            let (cols, rows) = pz.picture_cells();
            prop_assert!(cols >= 1 && rows >= 1);
            if maze { check_maze(&pz.built) } else { check_labyrinth(&pz.built) }
            // the frame fits the printable area
            let frame = (2.0 * cols as f64 * pz.pitch_mm, 2.0 * rows as f64 * pz.pitch_mm);
            prop_assert!(frame.0 <= pz.paper_mm.0 - 2.0 * 12.7 + 1e-9);
            prop_assert!(frame.1 <= pz.paper_mm.1 - 2.0 * 12.7 + 1e-9);
        }
    }
}
