//! Embed the catalogs of `locales/<locale>/youcraft.tsv` when the `embed` feature is on.
//!
//! Writes `catalogs.rs` into `OUT_DIR`: one `(locale, "", catalog)` entry per locale directory
//! that holds a `youcraft.tsv`, sorted by locale, each catalog included by its absolute path.
//! Without the feature the list is empty, and nothing is read.

use std::path::PathBuf;

fn main() {
    println!("cargo::rerun-if-env-changed=CARGO_FEATURE_EMBED");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default()).join("catalogs.rs");
    let mut entries = Vec::new();
    if std::env::var_os("CARGO_FEATURE_EMBED").is_some() {
        let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
        let locales = manifest.join("../../locales");
        println!("cargo::rerun-if-changed={}", locales.display());
        if let Ok(dirs) = std::fs::read_dir(&locales) {
            for dir in dirs.flatten() {
                let catalog = dir.path().join("youcraft.tsv");
                if catalog.is_file() {
                    println!("cargo::rerun-if-changed={}", catalog.display());
                    entries.push((dir.file_name().to_string_lossy().into_owned(), catalog));
                }
            }
        }
    }
    entries.sort();
    let mut code =
        String::from("/// Every embedded catalog: (locale, shared catalog, YouCraft's catalog).\n");
    code.push_str("pub static EMBEDDED: &craft_i18n::Embedded = &[\n");
    for (locale, path) in &entries {
        code.push_str(&format!(
            "    ({locale:?}, \"\", include_str!({:?})),\n",
            path.display().to_string()
        ));
    }
    code.push_str("];\n");
    if let Err(error) = std::fs::write(&out, code) {
        println!("cargo::error=cannot write {}: {error}", out.display());
    }
}
