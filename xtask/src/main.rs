//! YouCraft's build tasks, run as `cargo xtask <task> [--release]`.
//!
//! `site` builds the browser half afresh into `<target>/site/`: the `ui` crate compiled to wasm32
//! with its `hydrate` feature, bound by wasm-bindgen (the library, pinned to the crate's exact
//! version in the workspace, so the bindings always match), shrunk in release by the wasm-opt of
//! binaryen's version [`WASM_OPT`] ([`WASM_OPT_INSTALL`]; any other version, or none, fails the
//! build, so that one commit makes one bundle), the stylesheets of every crate's `style/` concatenated in name order, and the files of
//! `crates/ui/assets/` copied beside them. `serve` builds the site and runs the server on it.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The name of the bundle's files: `pkg/youcraft.js`, `pkg/youcraft_bg.wasm`, `pkg/youcraft.css`.
const BUNDLE: &str = "youcraft";

/// The binaryen version whose wasm-opt shrinks a release bundle, as `wasm-opt --version` names it.
const WASM_OPT: &str = "116";

/// How to install that wasm-opt.
const WASM_OPT_INSTALL: &str = "cargo install wasm-opt --version 0.116.1 --locked";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let release = args.iter().any(|arg| arg == "--release");
    let result = match args.first().map(String::as_str) {
        Some("site") => site(release).map(drop),
        Some("serve") => site(release).and_then(|site| serve(&site, release)),
        _ => Err("usage: cargo xtask site|serve [--release]".to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}

/// The workspace root, which holds this crate.
fn root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().unwrap_or(manifest).to_path_buf()
}

/// Cargo's target directory, as `CARGO_TARGET_DIR` sets it for parallel agents.
fn target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root().join("target"), PathBuf::from)
}

/// Build the browser bundle and return the site directory.
fn site(release: bool) -> Result<PathBuf, String> {
    if release {
        wasm_opt_is_pinned()?;
    }
    let profile = if release { "wasm-release" } else { "dev" };
    run(Command::new(env!("CARGO"))
        .current_dir(root())
        .args([
            "build",
            "--locked",
            "--package",
            "youcraft-ui",
            "--lib",
            "--target",
            "wasm32-unknown-unknown",
        ])
        .args(["--features", "hydrate", "--profile", profile]))?;
    let profile_dir = if release { "wasm-release" } else { "debug" };
    let wasm = target_dir()
        .join("wasm32-unknown-unknown")
        .join(profile_dir)
        .join("youcraft_ui.wasm");
    let site = target_dir().join("site");
    if site.exists() {
        std::fs::remove_dir_all(&site).map_err(|e| format!("{}: {e}", site.display()))?;
    }
    let pkg = site.join("pkg");
    std::fs::create_dir_all(&pkg).map_err(|e| format!("{}: {e}", pkg.display()))?;

    let mut bindgen = wasm_bindgen_cli_support::Bindgen::new();
    bindgen
        .input_path(&wasm)
        .out_name(BUNDLE)
        .typescript(false)
        .debug(!release);
    bindgen
        .web(true)
        .map_err(|e| format!("wasm-bindgen: {e}"))?;
    bindgen
        .generate(&pkg)
        .map_err(|e| format!("wasm-bindgen: {e}"))?;

    let bound = pkg.join(format!("{BUNDLE}_bg.wasm"));
    if release {
        run(Command::new("wasm-opt")
            .arg("-Oz")
            .arg("--all-features")
            .arg(&bound)
            .arg("-o")
            .arg(&bound))?;
    }
    write_stylesheet(&root().join("crates"), &pkg.join(format!("{BUNDLE}.css")))?;
    copy_tree(&root().join("crates/ui/assets"), &site)?;
    Ok(site)
}

/// Run the server on `site`, with its data in `data/` of the workspace.
fn serve(site: &Path, release: bool) -> Result<(), String> {
    let mut command = Command::new(env!("CARGO"));
    command
        .current_dir(root())
        .args(["run", "--locked", "--package", "youcraft-server"]);
    if release {
        command.arg("--release");
    }
    command.args(["--", "serve"]).env("YOUCRAFT_SITE", site);
    run(&mut command)
}

/// Concatenate the `.css` files in the `style/` directory of every crate under `crates`, in the
/// order of their names (`00-tokens.css` first, whichever crate holds it), into `out`.
fn write_stylesheet(crates: &Path, out: &Path) -> Result<(), String> {
    let mut names: Vec<PathBuf> = Vec::new();
    let members = std::fs::read_dir(crates).map_err(|e| format!("{}: {e}", crates.display()))?;
    for member in members.filter_map(Result::ok) {
        let Ok(styles) = std::fs::read_dir(member.path().join("style")) else {
            continue;
        };
        names.extend(
            styles
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| path.extension().is_some_and(|ext| ext == "css")),
        );
    }
    names.sort_by(|a, b| a.file_name().cmp(&b.file_name()).then_with(|| a.cmp(b)));
    let mut css = String::new();
    for name in names {
        let text =
            std::fs::read_to_string(&name).map_err(|e| format!("{}: {e}", name.display()))?;
        css.push_str(&text);
        css.push('\n');
    }
    std::fs::write(out, css).map_err(|e| format!("{}: {e}", out.display()))
}

/// Copy every file under `from` to the same place under `to`; nothing if `from` is absent.
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    let Ok(entries) = std::fs::read_dir(from) else {
        return Ok(());
    };
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|e| format!("{}: {e}", from.display()))?
            .path();
        let Some(name) = path.file_name() else {
            continue;
        };
        if path.is_dir() {
            copy_tree(&path, &to.join(name))?;
        } else {
            std::fs::copy(&path, to.join(name)).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    Ok(())
}

/// Whether the installed wasm-opt is binaryen's [`WASM_OPT`]; why not, if it is not.
fn wasm_opt_is_pinned() -> Result<(), String> {
    let install = format!("a release bundle is shrunk by wasm-opt {WASM_OPT}: {WASM_OPT_INSTALL}");
    let output = Command::new("wasm-opt")
        .arg("--version")
        .output()
        .map_err(|e| format!("wasm-opt: {e}; {install}"))?;
    let version = String::from_utf8_lossy(&output.stdout);
    // `wasm-opt version 116 (version_116)`
    match version.split_whitespace().nth(2) {
        Some(found) if output.status.success() && found == WASM_OPT => Ok(()),
        _ => Err(format!("wasm-opt is {:?}; {install}", version.trim())),
    }
}

fn run(command: &mut Command) -> Result<(), String> {
    let status = command.status().map_err(|e| format!("{command:?}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{command:?}: {status}"))
    }
}
