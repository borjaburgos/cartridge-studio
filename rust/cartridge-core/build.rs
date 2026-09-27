use std::{env, path::PathBuf};
fn main() {
    println!("cargo:rerun-if-env-changed=CARTRIDGE_STUDIO_BUILD_CATALOG");
    let source = env::var_os("CARTRIDGE_STUDIO_BUILD_CATALOG")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
                .join("../../tmp/game-catalog/games.sqlite3")
        });
    println!("cargo:rerun-if-changed={}", source.display());
    let source = source
        .canonicalize()
        .expect("Build the pinned game catalog first: python3 scripts/build_game_catalog.py");
    println!(
        "cargo:rustc-env=CARTRIDGE_STUDIO_EMBEDDED_CATALOG={}",
        source.display()
    );
    let helper = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("../../tmp/studio-build/helper.bin");
    println!("cargo:rerun-if-changed={}", helper.display());
    let helper = helper
        .canonicalize()
        .expect("Build the qualified embedded helper first: python3 scripts/build_rust.py");
    println!(
        "cargo:rustc-env=CARTRIDGE_STUDIO_EMBEDDED_HELPER={}",
        helper.display()
    );
}
