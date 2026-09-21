//! Bakes every pets/**/*.json into the binary, so `cargo install --path .`
//! produces a single self-contained executable.  Pets are organised one
//! directory per species (pets/cat/black.json, pets/fox/red.json, ...);
//! dropping a new file anywhere under pets/ is picked up automatically --
//! no code change needed.

use std::fmt::Write as _;
use std::{env, fs, path::Path, path::PathBuf};

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("pets");
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut paths = Vec::new();
    collect_json(&dir, &mut paths);
    paths.sort();

    let mut out = String::from("pub static EMBEDDED: &[(&str, &str)] = &[\n");
    for path in &paths {
        println!("cargo:rerun-if-changed={}", path.display());
        // Label with the path relative to pets/ (e.g. "cat/black.json"), so
        // two species can each have a same-named variant without colliding.
        let label = path.strip_prefix(&dir).unwrap_or(path).display().to_string();
        writeln!(out, "    ({label:?}, include_str!({:?})),", path.display()).unwrap();
    }
    out.push_str("];\n");

    let dest = Path::new(&env::var("OUT_DIR").unwrap()).join("embedded_pets.rs");
    fs::write(dest, out).unwrap();
}

/// Every `*.json` file under `dir`, at any depth, skipping dotfiles.
fn collect_json(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_json(&path, out);
        } else if path.extension().is_some_and(|e| e == "json") {
            out.push(path);
        }
    }
}
