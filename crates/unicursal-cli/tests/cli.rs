//! The CLI end to end: files written, prototype flags and presets honoured.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn shapes() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shapes")
}

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("unicursal-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_unicursal"))
        .args(args)
        .output()
        .unwrap()
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

#[test]
fn writes_puzzle_and_solution_svg_and_pdf() {
    let d = tmpdir("files");
    let out = d.join("ghost");
    let img = shapes().join("ghost.png");
    let o = run(&[
        img.to_str().unwrap(),
        "--preset",
        "medium",
        "--seed",
        "3",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", text(&o));
    for f in [
        "ghost.svg",
        "ghost.pdf",
        "ghost_solution.svg",
        "ghost_solution.pdf",
    ] {
        let bytes = std::fs::read(d.join(f)).unwrap();
        assert!(!bytes.is_empty(), "{f}");
    }
    assert!(
        std::fs::read(d.join("ghost.pdf"))
            .unwrap()
            .starts_with(b"%PDF-")
    );
    let svg = std::fs::read_to_string(d.join("ghost.svg")).unwrap();
    assert!(
        svg.contains("13x17 cells · 7.0 mm corridors · seed 3"),
        "{svg}"
    );
    assert!(text(&o).contains("picture 13x17 cells"));
}

#[test]
fn same_seed_same_files() {
    let d = tmpdir("seed");
    let img = shapes().join("cat.png");
    for name in ["a", "b"] {
        let out = d.join(name);
        let o = run(&[
            img.to_str().unwrap(),
            "--mode",
            "maze",
            "--seed",
            "11",
            "--out",
            out.to_str().unwrap(),
        ]);
        assert!(o.status.success(), "{}", text(&o));
    }
    assert_eq!(
        std::fs::read(d.join("a.pdf")).unwrap(),
        std::fs::read(d.join("b.pdf")).unwrap()
    );
}

#[test]
fn explicit_flags_override_the_preset() {
    let d = tmpdir("override");
    let out = d.join("g");
    let img = shapes().join("ghost.png");
    let o = run(&[
        img.to_str().unwrap(),
        "--preset",
        "easy",
        "--corridor",
        "5",
        "--seed",
        "1",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(
        text(&o).contains("corridor spacing 5.00 mm"),
        "{}",
        text(&o)
    );
}

#[test]
fn lists_presets_in_file_order() {
    let o = run(&["--list-presets"]);
    assert!(o.status.success());
    let names: Vec<String> = String::from_utf8_lossy(&o.stdout)
        .lines()
        .map(|l| l.split_whitespace().next().unwrap().to_string())
        .collect();
    assert_eq!(names, ["easy", "medium", "hard"]);
}

#[test]
fn unknown_preset_and_unported_styles_fail_cleanly() {
    let img = shapes().join("ghost.png");
    let img = img.to_str().unwrap();
    let o = run(&[img, "--preset", "extreme"]);
    assert!(!o.status.success());
    assert!(
        text(&o).contains("unknown preset 'extreme'"),
        "{}",
        text(&o)
    );
    let o = run(&[img, "--flow"]);
    assert!(!o.status.success());
    assert!(
        text(&o).contains("not in the Rust core yet"),
        "{}",
        text(&o)
    );
    let o = run(&[img, "--flow", "--mode", "maze"]);
    assert!(
        text(&o).contains("flow needs mode labyrinth"),
        "{}",
        text(&o)
    );
}
