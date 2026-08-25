use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    // The header is generated straight into the Swift package so plain
    // `swift build` always sees the current ABI; it is committed, and CI
    // checks for drift.
    let header = crate_dir.join("../../macos/Sources/CPrchum/include/prchum.h");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=cbindgen.toml");
    cbindgen::generate(&crate_dir)
        .expect("cbindgen failed")
        .write_to_file(header);
}
