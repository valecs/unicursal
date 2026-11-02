//! The CLI end to end: files written, prototype flags and presets honoured.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

fn shape(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../shapes")
        .join(format!("{name}.png"))
}

fn unicursal() -> Command {
    Command::cargo_bin("unicursal").unwrap()
}

/// Run in a fresh temporary directory, so outputs land there.
fn run_in(dir: &TempDir, args: &[&str]) -> std::process::Output {
    unicursal()
        .current_dir(dir.path())
        .args(args)
        .output()
        .unwrap()
}

fn text(o: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

#[test]
fn writes_puzzle_and_solution_svg_and_pdf() {
    let d = TempDir::new().unwrap();
    let img = shape("ghost");
    let o = run_in(
        &d,
        &[
            img.to_str().unwrap(),
            "--preset",
            "medium",
            "--seed",
            "3",
            "--out",
            "ghost",
        ],
    );
    assert!(o.status.success(), "{}", text(&o));
    for f in [
        "ghost.svg",
        "ghost.pdf",
        "ghost_solution.svg",
        "ghost_solution.pdf",
    ] {
        let bytes = std::fs::read(d.path().join(f)).unwrap();
        assert!(!bytes.is_empty(), "{f}");
    }
    assert!(
        std::fs::read(d.path().join("ghost.pdf"))
            .unwrap()
            .starts_with(b"%PDF-")
    );
    let svg = std::fs::read_to_string(d.path().join("ghost.svg")).unwrap();
    assert!(
        svg.contains("13x17 cells · 7.0 mm corridors · seed 3"),
        "{svg}"
    );
    assert!(text(&o).contains("picture 13x17 cells"));
}

#[test]
fn same_seed_same_files() {
    let d = TempDir::new().unwrap();
    let img = shape("cat");
    for name in ["a", "b"] {
        unicursal()
            .current_dir(d.path())
            .args([
                img.to_str().unwrap(),
                "--mode",
                "maze",
                "--seed",
                "11",
                "--out",
                name,
            ])
            .assert()
            .success();
    }
    assert_eq!(
        std::fs::read(d.path().join("a.pdf")).unwrap(),
        std::fs::read(d.path().join("b.pdf")).unwrap()
    );
}

#[test]
fn explicit_flags_override_the_preset() {
    let d = TempDir::new().unwrap();
    let img = shape("ghost");
    let o = run_in(
        &d,
        &[
            img.to_str().unwrap(),
            "--preset",
            "easy",
            "--corridor",
            "5",
            "--seed",
            "1",
        ],
    );
    assert!(o.status.success(), "{}", text(&o));
    assert!(
        text(&o).contains("corridor spacing 5.00 mm"),
        "{}",
        text(&o)
    );
}

#[test]
fn lists_presets_in_file_order() {
    let o = unicursal().arg("--list-presets").output().unwrap();
    assert!(o.status.success());
    let names: Vec<String> = String::from_utf8_lossy(&o.stdout)
        .lines()
        .map(|l| l.split_whitespace().next().unwrap().to_string())
        .collect();
    assert_eq!(names, ["easy", "medium", "hard"]);
}

#[test]
fn unknown_preset_and_unported_styles_fail_cleanly() {
    let d = TempDir::new().unwrap();
    let img = shape("ghost");
    let img = img.to_str().unwrap();
    for (args, msg) in [
        (vec![img, "--preset", "extreme"], "unknown preset 'extreme'"),
        (vec![img, "--flow"], "not in the Rust core yet"),
        (
            vec![img, "--flow", "--mode", "maze"],
            "flow needs mode labyrinth",
        ),
    ] {
        let o = run_in(&d, &args);
        assert!(!o.status.success(), "{args:?}");
        assert!(text(&o).contains(msg), "{args:?}: {}", text(&o));
    }
    assert_eq!(
        std::fs::read_dir(d.path()).unwrap().count(),
        0,
        "no files on failure"
    );
}
