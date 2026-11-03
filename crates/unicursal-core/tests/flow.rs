//! Flow style: port of test_invariants.py's flow checks, plus properties.
//! Every flow puzzle must have no strand crossings, exactly one open region
//! per strand (no wall leaks), and a minimum clear corridor width.

mod common;

use common::*;
use proptest::prelude::*;
use unicursal_core::flow::{FlowParams, Relaxation, Strand, extract_strands};
use unicursal_core::mask::Mask;
use unicursal_core::maze::{Built, build_labyrinth};
use unicursal_core::{Params, Pcg32, flow_check, generate};

fn relaxed(b: &Built, params: FlowParams) -> Vec<Strand> {
    let strands = extract_strands(&b.grid, &b.solution());
    let mut r = Relaxation::new(&strands, &b.fine_fg, params);
    r.run();
    r.strands()
}

fn prototype_test_params(seed: u64, iters: u32) -> FlowParams {
    FlowParams {
        iters,
        length: 0.85,
        bend: 0.3,
        noise: 0.02,
        seed,
        ..FlowParams::default()
    }
}

#[test]
fn flow_is_sealed_and_wide_enough() {
    let fg = picture(&ring(), 14);
    let b = build_labyrinth(&fg, &mut Pcg32::from_seed(4));
    let strands = relaxed(&b, prototype_test_params(4, 600));
    let (fr, widths) = flow_check(&b, strands);
    assert_eq!(fr.crossings, 0);
    assert_eq!(
        fr.open_regions,
        fr.strands.len(),
        "{} open regions for {} strands (a wall leaks)",
        fr.open_regions,
        fr.strands.len()
    );
    let narrowest = widths.iter().copied().fold(f64::INFINITY, f64::min);
    assert!(
        narrowest > 0.6,
        "narrowest clear width {narrowest} corridors"
    );
}

#[test]
fn strands_are_the_labyrinth() {
    // one open solution strand through exactly the picture's cells, then one
    // closed loop per background region, covering every cell once
    let fg = picture(&ring(), 14);
    let b = build_labyrinth(&fg, &mut Pcg32::from_seed(2));
    let s = extract_strands(&b.grid, &b.solution());
    assert!(s[0].solution && !s[0].closed);
    assert_eq!(s[0].pts.len(), b.fine_fg.count());
    assert!(s[1..].iter().all(|s| s.closed && !s.solution));
    let total: usize = s.iter().map(|s| s.pts.len()).sum();
    assert_eq!(total, b.grid.rows * b.grid.cols);
}

#[test]
fn relaxation_is_steppable() {
    let fg = picture(&ring(), 10);
    let b = build_labyrinth(&fg, &mut Pcg32::from_seed(1));
    let strands = extract_strands(&b.grid, &b.solution());
    let params = prototype_test_params(1, 40);
    let mut a = Relaxation::new(&strands, &b.fine_fg, params.clone());
    a.run();
    let mut c = Relaxation::new(&strands, &b.fine_fg, params);
    while !c.is_done() {
        assert!(c.progress() < 1.0);
        c.step();
    }
    assert_eq!(c.progress(), 1.0);
    assert_eq!(a.strands(), c.strands());
}

#[test]
fn generate_flow_draws_one_arrow_and_a_verified_puzzle() {
    let p = Params {
        flow: true,
        cells: Some(14),
        seed: 4,
        ..Params::default()
    };
    let pz = generate(&ring(), &p).unwrap();
    let fr = pz.flow.as_ref().unwrap();
    assert!(
        fr.verified(),
        "{} crossings, {} regions",
        fr.crossings,
        fr.open_regions
    );
    for page in [&pz.puzzle, &pz.solution] {
        let svg = page.to_svg();
        assert_eq!(svg.matches("class=\"entrance\"").count(), 1);
        assert!(svg.contains("fill=\"#111111\""), "void colour");
    }
    assert!(pz.solution.to_svg().contains("stroke=\"#e0218a\""));
    assert!(!pz.puzzle.to_svg().contains("#e0218a"));
    // ink saver: light voids, corridors outlined, same geometry
    let ink = generate(
        &ring(),
        &Params {
            ink_saver: true,
            void: "#cfcfcf".into(),
            ..p
        },
    )
    .unwrap();
    let svg = ink.puzzle.to_svg();
    assert!(svg.contains("fill=\"#cfcfcf\""));
    assert!(ink.flow.unwrap().verified());
}

#[cfg(feature = "parallel")]
#[test]
fn thread_count_does_not_change_the_result() {
    let fg = picture(&shape("ghost"), 12);
    let b = build_labyrinth(&fg, &mut Pcg32::from_seed(3));
    let run = |threads: usize| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| relaxed(&b, prototype_test_params(9, 300)))
    };
    let one = run(1);
    for n in [2, 3, 8] {
        assert_eq!(run(n), one, "{n} threads");
    }
}

fn small_pictures() -> impl Strategy<Value = (Mask, u64)> {
    (1usize..=6, 1usize..=6)
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

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, failure_persistence: None, ..ProptestConfig::default() })]

    /// The flow invariants on arbitrary pictures and seeds.
    #[test]
    fn flow_invariants((mask, seed) in small_pictures()) {
        let fg = unicursal_core::mask::largest_component(&mask, true).unwrap().0;
        let b = build_labyrinth(&fg, &mut Pcg32::from_seed(seed));
        let strands = relaxed(&b, prototype_test_params(seed, 400));
        let (fr, widths) = flow_check(&b, strands);
        prop_assert_eq!(fr.crossings, 0);
        prop_assert_eq!(fr.open_regions, fr.strands.len(), "a wall leaks");
        let narrowest = widths.iter().copied().fold(f64::INFINITY, f64::min);
        prop_assert!(narrowest > 0.6, "narrowest clear width {}", narrowest);
    }
}
