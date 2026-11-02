//! Generator options (named as the prototype's CLI flags) and difficulty
//! presets, which are data: `presets.toml`, shared with the prototype.

use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    Labyrinth,
    Maze,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Orientation {
    Auto,
    Portrait,
    Landscape,
}

/// Every generator option. Field names are the prototype's option names
/// (`--flow-length` → `flow_length`), which presets use as keys.
#[derive(Clone, Debug, PartialEq)]
pub struct Params {
    /// Corridor spacing on paper, mm.
    pub corridor: f64,
    pub paper: String,
    pub orientation: Orientation,
    /// Blank margin at the paper edge, mm.
    pub paper_margin: f64,
    /// Fixed picture resolution (long side); overrides `corridor`.
    pub cells: Option<usize>,
    pub mode: Mode,
    pub seed: u64,
    pub threshold: u8,
    pub invert: bool,
    pub no_bridges: bool,
    pub deadend_fix: bool,
    pub fill: String,
    pub curvy: bool,
    pub warp: f64,
    pub warp_scale: f64,
    pub smooth: u32,
    pub flow: bool,
    pub flow_iters: u32,
    pub flow_length: f64,
    pub flow_bend: f64,
    pub flow_tol: f64,
    pub void: String,
    pub ink_saver: bool,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            corridor: 8.0,
            paper: "letter".into(),
            orientation: Orientation::Auto,
            paper_margin: 12.7,
            cells: None,
            mode: Mode::Labyrinth,
            seed: 0,
            threshold: 128,
            invert: false,
            no_bridges: false,
            deadend_fix: true,
            fill: "#e0218a".into(),
            curvy: false,
            warp: 0.55,
            warp_scale: 14.0,
            smooth: 3,
            flow: false,
            flow_iters: 1500,
            flow_length: 0.85,
            flow_bend: 0.3,
            flow_tol: 1.0,
            void: "#111".into(),
            ink_saver: false,
        }
    }
}

impl Params {
    /// Option names a preset may set.
    pub const KEYS: [&'static str; 23] = [
        "corridor",
        "paper",
        "orientation",
        "paper_margin",
        "cells",
        "mode",
        "seed",
        "threshold",
        "invert",
        "no_bridges",
        "deadend_fix",
        "fill",
        "curvy",
        "warp",
        "warp_scale",
        "smooth",
        "flow",
        "flow_iters",
        "flow_length",
        "flow_bend",
        "flow_tol",
        "void",
        "ink_saver",
    ];

    /// Set one option by name from a TOML value.
    pub fn set(&mut self, key: &str, v: &toml::Value) -> Result<(), Error> {
        let bad = || Error::BadOption(format!("{key} = {v:?}"));
        let float = || {
            v.as_float()
                .or_else(|| v.as_integer().map(|i| i as f64))
                .ok_or_else(bad)
        };
        let int = || v.as_integer().ok_or_else(bad);
        let uint = || int().and_then(|i| u64::try_from(i).map_err(|_| bad()));
        let boolean = || v.as_bool().ok_or_else(bad);
        let string = || v.as_str().map(str::to_string).ok_or_else(bad);
        match key {
            "corridor" => self.corridor = float()?,
            "paper" => self.paper = string()?,
            "orientation" => {
                self.orientation = match string()?.as_str() {
                    "auto" => Orientation::Auto,
                    "portrait" => Orientation::Portrait,
                    "landscape" => Orientation::Landscape,
                    _ => return Err(bad()),
                }
            }
            "paper_margin" => self.paper_margin = float()?,
            "cells" => self.cells = Some(uint()? as usize),
            "mode" => {
                self.mode = match string()?.as_str() {
                    "labyrinth" => Mode::Labyrinth,
                    "maze" => Mode::Maze,
                    _ => return Err(bad()),
                }
            }
            "seed" => self.seed = uint()?,
            "threshold" => self.threshold = u8::try_from(int()?).map_err(|_| bad())?,
            "invert" => self.invert = boolean()?,
            "no_bridges" => self.no_bridges = boolean()?,
            "deadend_fix" => self.deadend_fix = boolean()?,
            "fill" => self.fill = string()?,
            "curvy" => self.curvy = boolean()?,
            "warp" => self.warp = float()?,
            "warp_scale" => self.warp_scale = float()?,
            "smooth" => self.smooth = u32::try_from(int()?).map_err(|_| bad())?,
            "flow" => self.flow = boolean()?,
            "flow_iters" => self.flow_iters = u32::try_from(int()?).map_err(|_| bad())?,
            "flow_length" => self.flow_length = float()?,
            "flow_bend" => self.flow_bend = float()?,
            "flow_tol" => self.flow_tol = float()?,
            "void" => self.void = string()?,
            "ink_saver" => self.ink_saver = boolean()?,
            _ => return Err(Error::BadOption(format!("unknown option '{key}'"))),
        }
        Ok(())
    }
}

/// A difficulty preset: display metadata plus option values.
#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    pub name: String,
    pub label: Option<String>,
    pub tool: Option<String>,
    /// Option name → value, in file order.
    pub values: Vec<(String, toml::Value)>,
}

impl Preset {
    pub fn apply(&self, p: &mut Params) -> Result<(), Error> {
        for (k, v) in &self.values {
            p.set(k, v)
                .map_err(|e| Error::BadPreset(format!("{}: {e}", self.name)))?;
        }
        Ok(())
    }
}

/// The repository's `presets.toml`, built in.
pub const DEFAULT_PRESETS: &str = include_str!("../../../presets.toml");

/// Parse a presets file (`[preset.<name>]` tables), in file order. Every
/// preset is checked: only known options, with values of the right type.
pub fn parse_presets(text: &str) -> Result<Vec<Preset>, Error> {
    let doc: toml::Table = text
        .parse()
        .map_err(|e: toml::de::Error| Error::BadPreset(e.to_string()))?;
    let Some(table) = doc.get("preset") else {
        return Ok(vec![]);
    };
    let table = table
        .as_table()
        .ok_or_else(|| Error::BadPreset("`preset` is not a table".into()))?;
    let mut out = vec![];
    for (name, body) in table {
        let body = body
            .as_table()
            .ok_or_else(|| Error::BadPreset(format!("preset.{name} is not a table")))?;
        let meta = |k: &str| body.get(k).and_then(|v| v.as_str()).map(str::to_string);
        let preset = Preset {
            name: name.clone(),
            label: meta("label"),
            tool: meta("tool"),
            values: body
                .iter()
                .filter(|(k, _)| !matches!(k.as_str(), "label" | "tool"))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        };
        preset.apply(&mut Params::default())?;
        out.push(preset);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_presets_parse_in_file_order_and_set_only_known_options() {
        let p = parse_presets(DEFAULT_PRESETS).unwrap();
        let names: Vec<_> = p.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["easy", "medium", "hard"]);
        for preset in &p {
            for (k, _) in &preset.values {
                assert!(Params::KEYS.contains(&k.as_str()), "{}: {k}", preset.name);
            }
        }
    }

    #[test]
    fn preset_sets_values() {
        let p = parse_presets(DEFAULT_PRESETS).unwrap();
        let mut params = Params::default();
        p[0].apply(&mut params).unwrap();
        assert_eq!(params.corridor, 11.0);
    }

    #[test]
    fn bad_presets_are_rejected() {
        assert!(parse_presets("[preset.x]\nwiggle = 3\n").is_err());
        assert!(parse_presets("[preset.x]\ncorridor = \"big\"\n").is_err());
        assert!(parse_presets("[preset.x]\nmode = \"spiral\"\n").is_err());
    }

    #[test]
    fn every_key_is_settable() {
        let mut p = Params::default();
        for k in Params::KEYS {
            // any of these types must be accepted by exactly the right keys;
            // here we only check that every listed key is known
            let r = p.set(k, &toml::Value::Boolean(true));
            assert!(
                !matches!(r, Err(Error::BadOption(ref m)) if m.starts_with("unknown")),
                "{k}"
            );
        }
    }

    #[test]
    fn deadend_fix_is_on_by_default() {
        // #7: the owner chose the dead-end heuristic as the maze default
        assert!(Params::default().deadend_fix);
    }
}
