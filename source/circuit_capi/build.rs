// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{env, path::PathBuf};

fn main() {
    let crate_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest path"));
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output path"));
    let config = cbindgen::Config::from_file(crate_dir.join("cbindgen.toml"))
        .expect("valid C header configuration");
    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
        .expect("C header generation succeeds")
        .write_to_file(output.join("qdk_circuit.h"));
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=cbindgen.toml");
}
