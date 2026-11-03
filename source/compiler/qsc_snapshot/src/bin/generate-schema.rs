// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

fn main() -> Result<(), capnp::Error> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    capnpc::CompilerCommand::new()
        .src_prefix(root.join("schema"))
        .file(root.join("schema/snapshot.capnp"))
        .output_path(root.join("src"))
        .run()
}
