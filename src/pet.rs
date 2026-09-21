//! The data-driven pet format: everything about a pet lives in its JSON file.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/embedded_pets.rs"));

/// Every pet must provide at least these; the rest are optional extras.
pub const REQUIRED: &[&str] = &["sit", "walk", "sleep"];
pub const MAX_COLORS: usize = 8;

pub type Rgb = (u8, u8, u8);

#[derive(Deserialize)]
pub struct FrameSize {
    pub width: usize,
    pub height: usize,
}

/// Which way an animation is drawn.  Only a side view means anything when
/// flipped: mirroring a front view just moves the tail to the other side, and
/// mirroring `look_left` would turn it into `look_right`.
#[derive(Deserialize, Clone, Copy, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum View {
    #[default]
    Front,
    Side,
}

#[derive(Deserialize)]
pub struct Animation {
    pub fps: f32,
    #[serde(default)]
    pub view: View,
    #[serde(rename = "loop", default = "yes")]
    pub looping: bool,
    pub frames: Vec<Vec<String>>,
}

fn yes() -> bool {
    true
}

impl Animation {
    /// True when facing left should flip this animation.
    pub fn mirrors(&self) -> bool {
        self.view == View::Side
    }
}

fn dot() -> char {
    '.'
}

#[derive(Deserialize)]
pub struct Pet {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub frame: FrameSize,
    #[serde(default = "dot")]
    pub transparent: char,
    pub palette: BTreeMap<String, String>,
    pub animations: BTreeMap<String, Animation>,

    /// palette resolved to rgb, indexed by byte, built after loading
    #[serde(skip)]
    lut: Vec<Option<Rgb>>,
}

impl Pet {
    pub fn parse(json: &str) -> Result<Pet, String> {
        let mut pet: Pet = serde_json::from_str(json).map_err(|e| e.to_string())?;
        pet.lut = vec![None; 256];
        for (key, hex) in &pet.palette {
            let ch = one_char(key)?;
            pet.lut[ch as usize] = Some(parse_hex(hex)?);
        }
        Ok(pet)
    }

    /// Colour of a palette character, or None when it is transparent.
    #[inline]
    pub fn color(&self, ch: char) -> Option<Rgb> {
        if ch == self.transparent || ch as usize >= 256 {
            None
        } else {
            self.lut[ch as usize]
        }
    }

    pub fn animation(&self, name: &str) -> Option<&Animation> {
        self.animations.get(name)
    }

    /// Falls back to `sit` so a pet missing an optional extra still animates.
    pub fn animation_or_sit(&self, name: &str) -> &Animation {
        self.animations
            .get(name)
            .or_else(|| self.animations.get("sit"))
            .expect("validated pets always have sit")
    }

    pub fn has(&self, name: &str) -> bool {
        self.animations.contains_key(name)
    }

    /// Returns every problem with this pet, empty when it is well formed.
    pub fn validate(&self) -> Vec<String> {
        let mut errs = Vec::new();
        let (w, h) = (self.frame.width, self.frame.height);

        if w == 0 || h == 0 {
            errs.push(format!("frame size {w}x{h} is degenerate"));
        }
        if self.palette.len() > MAX_COLORS {
            errs.push(format!(
                "palette has {} colours, the limit is {MAX_COLORS}",
                self.palette.len()
            ));
        }
        for key in self.palette.keys() {
            if one_char(key).is_err() {
                errs.push(format!("palette key {key:?} is not a single character"));
            }
            if key.chars().next() == Some(self.transparent) {
                errs.push(format!("palette key {key:?} collides with the transparent char"));
            }
        }
        for name in REQUIRED {
            if !self.animations.contains_key(*name) {
                errs.push(format!("missing required animation {name:?}"));
            }
        }
        for (name, anim) in &self.animations {
            if anim.frames.is_empty() {
                errs.push(format!("{name}: has no frames"));
            }
            if !(anim.fps > 0.0 && anim.fps <= 60.0) {
                errs.push(format!("{name}: fps {} is out of range", anim.fps));
            }
            for (i, frame) in anim.frames.iter().enumerate() {
                if frame.len() != h {
                    errs.push(format!(
                        "{name}[{i}]: has {} rows, expected {h}",
                        frame.len()
                    ));
                }
                for (y, row) in frame.iter().enumerate() {
                    if row.chars().count() != w {
                        errs.push(format!(
                            "{name}[{i}] row {y}: has {} chars, expected {w}",
                            row.chars().count()
                        ));
                    }
                    for ch in row.chars() {
                        if ch != self.transparent && self.color(ch).is_none() {
                            errs.push(format!(
                                "{name}[{i}] row {y}: char {ch:?} is not in the palette"
                            ));
                        }
                    }
                }
            }
        }
        errs.dedup();
        errs
    }
}

fn one_char(key: &str) -> Result<char, String> {
    let mut it = key.chars();
    match (it.next(), it.next()) {
        (Some(c), None) => Ok(c),
        _ => Err(format!("palette key {key:?} must be exactly one character")),
    }
}

fn parse_hex(hex: &str) -> Result<Rgb, String> {
    let h = hex.strip_prefix('#').unwrap_or(hex);
    if h.len() != 6 {
        return Err(format!("colour {hex:?} is not #rrggbb"));
    }
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).map_err(|_| format!("bad colour {hex:?}"));
    Ok((byte(0)?, byte(2)?, byte(4)?))
}

/// Directories searched at runtime, so a user can drop in their own pets
/// without rebuilding.  Later entries win over the ones baked into the binary.
pub fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(dir) = std::env::var("PET_DIR") {
        dirs.push(PathBuf::from(dir));
    }
    if let Ok(home) = std::env::var("XDG_DATA_HOME") {
        dirs.push(Path::new(&home).join("pet/pets"));
    } else if let Ok(home) = std::env::var("HOME") {
        dirs.push(Path::new(&home).join(".local/share/pet/pets"));
    }
    dirs.push(PathBuf::from("pets"));
    dirs
}

/// Every `*.json` file under `dir`, at any depth (pets are grouped one
/// directory per species: `cat/black.json`, `fox/red.json`, ...), skipping
/// dotfiles so an editor swapfile or a `.gitkeep` is never mistaken for a pet.
pub fn collect_json(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_json(&path, out);
        } else if path.extension().is_some_and(|e| e == "json") {
            out.push(path);
        }
    }
}

/// All pets, keyed by id: the embedded ones first, then anything on disk.
pub fn load_all() -> (BTreeMap<String, Pet>, Vec<String>) {
    let mut pets = BTreeMap::new();
    let mut errs = Vec::new();
    let mut add = |source: String, json: &str, errs: &mut Vec<String>| match Pet::parse(json) {
        Ok(pet) => {
            pets.insert(pet.id.clone(), pet);
        }
        Err(e) => errs.push(format!("{source}: {e}")),
    };

    for (name, json) in EMBEDDED {
        add(format!("<builtin>/{name}"), json, &mut errs);
    }
    for dir in search_dirs() {
        let mut paths = Vec::new();
        collect_json(&dir, &mut paths);
        paths.sort();
        for path in paths {
            match std::fs::read_to_string(&path) {
                Ok(json) => add(path.display().to_string(), &json, &mut errs),
                Err(e) => errs.push(format!("{}: {e}", path.display())),
            }
        }
    }
    (pets, errs)
}
