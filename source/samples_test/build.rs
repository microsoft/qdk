// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{
    ffi::OsStr,
    fs::{File, read_dir},
    io::Write,
    path::{Path, PathBuf},
};

fn main() {
    create_tests_for_files("algorithms");
    create_tests_for_files("getting_started");
    create_tests_for_files("language");
    create_tests_for_files_compile_only("estimation");
    create_tests_for_qasm_files("OpenQASM");
    create_tests_for_projects();
}

fn create_tests_for_files(folder: &str) {
    println!("cargo::rerun-if-changed=../../samples/{folder}/");
    // Iterate through the folder and create a test for each qs file
    let mut paths =
        read_dir(format!("../../samples/{folder}")).expect("folder should exist and be readable");
    let out_dir = "./src/tests";
    let dest_path = Path::new(&out_dir).join(format!("{folder}_generated.rs"));
    let mut f = File::create(dest_path).expect("files should be creatable in ./src/tests");

    writeln!(
        f,
        r#"
// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This build-generated module contains tests for the samples in the `/samples/{folder}` folder.
//! DO NOT MANUALLY EDIT THIS FILE. To regenerate this file, run `cargo check` or `cargo test` in the `samples_test` directory.

use super::{folder}::*;
use super::{{compile_and_run, compile_and_run_debug}};
use qsc::SourceMap;"#,
    )
    .expect("writing to file should succeed");

    while let Some(Ok(dir_entry)) = paths.next() {
        let path = &dir_entry.path();
        if Some("qs") != path.extension().and_then(OsStr::to_str) {
            continue;
        }
        let file_name = path
            .file_name()
            .expect("file name should be separable")
            .to_str()
            .expect("file name should be valid");
        let file_stem = path
            .file_stem()
            .expect("file name should be separable")
            .to_str()
            .expect("file name should be valid");
        assert!(
            !file_stem.contains(' '),
            "file name `{file_name}` should not contain spaces"
        );
        let file_stem_upper = file_stem.to_uppercase();

        writeln!(
            f,
            r#"
#[allow(non_snake_case)]
fn {file_stem}_src() -> SourceMap {{
    SourceMap::new(
        vec![("{file_name}".into(), include_str!("../../../../samples/{folder}/{file_name}").into())],
        None,
    )
}}

#[allow(non_snake_case)]
#[test]
fn run_{file_stem}() {{
    let output = compile_and_run({file_stem}_src());
    // This constant must be defined in `samples_test/src/tests/{folder}.rs` and
    // must contain the output of the sample {file_name}
    {file_stem_upper}_EXPECT.assert_eq(&output);
}}

#[allow(non_snake_case)]
#[test]
fn debug_{file_stem}() {{
    let output = compile_and_run_debug({file_stem}_src());
    // This constant must be defined in `samples_test/src/tests/{folder}.rs` and
    // must contain the output of the sample {file_name}
    {file_stem_upper}_EXPECT_DEBUG.assert_eq(&output);
}}"#
        )
        .expect("writing to file should succeed");
    }
}

fn create_tests_for_files_compile_only(folder: &str) {
    println!("cargo::rerun-if-changed=../../samples/{folder}/");
    // Iterate through the folder and create a test for each qs file
    let mut paths =
        read_dir(format!("../../samples/{folder}")).expect("folder should exist and be readable");
    let out_dir = "./src/tests";
    let dest_path = Path::new(&out_dir).join(format!("{folder}_generated.rs"));
    let mut f = File::create(dest_path).expect("files should be creatable in ./src/tests");

    writeln!(
        f,
        r#"
// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This build-generated module contains tests for the samples in the /samples/{folder} folder.
//! DO NOT MANUALLY EDIT THIS FILE. To regenerate this file, run `cargo check` or `cargo test` in the `samples_test` directory.

use super::compile;
use qsc::SourceMap;"#,
    )
    .expect("writing to file should succeed");

    while let Some(Ok(dir_entry)) = paths.next() {
        let path = &dir_entry.path();
        if Some("qs") != path.extension().and_then(OsStr::to_str) {
            continue;
        }
        let file_name = path
            .file_name()
            .expect("file name should be separable")
            .to_str()
            .expect("file name should be valid");
        let file_stem = path
            .file_stem()
            .expect("file name should be separable")
            .to_str()
            .expect("file name should be valid");
        assert!(
            !file_stem.contains(' '),
            "file name `{file_name}` should not contain spaces"
        );

        writeln!(
            f,
            r#"
#[allow(non_snake_case)]
#[test]
fn compile_{file_stem}() {{
    compile(
        SourceMap::new(
            vec![("{file_name}".into(), include_str!("../../../../samples/{folder}/{file_name}").into())],
            None,
        )
    );
}}"#
        )
        .expect("writing to file should succeed");
    }
}

fn create_tests_for_projects() {
    let mut paths = Vec::new();
    for entry in read_dir(Path::new("../../samples")).expect("folder should exist and be readable")
    {
        let entry = entry.expect("directory entries should be readable");
        let path = entry.path();
        if path.is_dir() {
            paths.append(&mut collect_qsharp_project_folders(&path));
        }
    }

    let out_dir = "./src/tests";
    let dest_path = Path::new(&out_dir).join("project_generated.rs");
    let mut f = File::create(dest_path).expect("files should be creatable in ./src/tests");

    writeln!(
        f,
        r#"
// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This build-generated module contains tests for the projects found in the samples folder.
//! DO NOT MANUALLY EDIT THIS FILE. To regenerate this file, run `cargo check` or `cargo test` in the `samples_test` directory.

use super::compile_project;"#,
    )
    .expect("writing to file should succeed");

    for path in paths {
        println!(
            "cargo::rerun-if-changed={}",
            path.to_str().expect("should have a valid path")
        );
        let file_stem = path
            .file_stem()
            .expect("file name should be separable")
            .to_str()
            .expect("file name should be valid");
        assert!(
            !file_stem.contains(' '),
            "folder name `{file_stem}` should not contain spaces"
        );
        let file_stem_cleaned = file_stem.replace('-', "_");

        writeln!(
            f,
            r#"
#[allow(non_snake_case)]
#[test]
fn compile_{file_stem_cleaned}() {{
    compile_project(r"{full_path}");
}}
"#,
            full_path = path.to_str().expect("should have a valid path"),
        )
        .expect("writing to file should succeed");
    }
}

fn collect_qsharp_project_folders(path: &Path) -> Vec<PathBuf> {
    if is_project_excluded(path) {
        return Vec::new();
    }

    // Recursively search for all qsharp.json projects in the samples directory and return
    // a list of their containing folders
    let mut projects = Vec::new();
    let mut paths = read_dir(path).expect("folder should exist and be readable");
    while let Some(Ok(dir_entry)) = paths.next() {
        let entry = &dir_entry.path();
        if entry.is_dir() {
            projects.append(&mut collect_qsharp_project_folders(entry));
        } else if Some("qsharp.json") == entry.file_name().and_then(OsStr::to_str) {
            projects.push(
                path.canonicalize()
                    .expect("path should resolve to a canonical path"),
            );
        }
    }
    projects
}

fn is_project_excluded(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(OsStr::to_str) else {
        return false;
    };

    // Skip projects that rely on GitHub-hosted dependencies that the test filesystem cannot resolve.
    name == "scratch" || name == "SPSA" || name == "HypercubeLookup"
}

fn create_tests_for_qasm_files(folder: &str) {
    println!("cargo::rerun-if-changed=../../samples/{folder}/");
    // Iterate through the folder and create a test for each qs file
    let mut paths =
        read_dir(format!("../../samples/{folder}")).expect("folder should exist and be readable");
    let out_dir = "./src/tests";
    let dest_path = Path::new(&out_dir).join(format!("{folder}_generated.rs"));
    let mut f = File::create(dest_path).expect("files should be creatable in ./src/tests");

    writeln!(
        f,
        r#"
// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This build-generated module contains tests for the samples in the `/samples/{folder}` folder.
//! DO NOT MANUALLY EDIT THIS FILE. To regenerate this file, run `cargo check` or `cargo test` in the `samples_test` directory.

use super::{folder}::*;
use super::{{compile_and_run_qasm, compile_and_run_debug_qasm}};"#,
    )
    .expect("writing to file should succeed");

    while let Some(Ok(dir_entry)) = paths.next() {
        let path = &dir_entry.path();
        if Some("qasm") != path.extension().and_then(OsStr::to_str) {
            continue;
        }
        let file_name = path
            .file_name()
            .expect("file name should be separable")
            .to_str()
            .expect("file name should be valid");
        let file_stem = path
            .file_stem()
            .expect("file name should be separable")
            .to_str()
            .expect("file name should be valid");
        assert!(
            !file_stem.contains(' '),
            "file name `{file_name}` should not contain spaces"
        );
        let file_stem_upper = file_stem.to_uppercase();

        writeln!(
            f,
            r#"
#[allow(non_snake_case)]
fn {file_stem}_src() -> &'static str {{
    include_str!("../../../../samples/{folder}/{file_name}")
}}

#[allow(non_snake_case)]
#[test]
fn run_{file_stem}() {{
    let output = compile_and_run_qasm({file_stem}_src());
    // This constant must be defined in `samples_test/src/tests/{folder}.rs` and
    // must contain the output of the sample {file_name}
    {file_stem_upper}_EXPECT.assert_eq(&output);
}}

#[allow(non_snake_case)]
#[test]
fn debug_{file_stem}() {{
    let output = compile_and_run_debug_qasm({file_stem}_src());
    // This constant must be defined in `samples_test/src/tests/{folder}.rs` and
    // must contain the output of the sample {file_name}
    {file_stem_upper}_EXPECT_DEBUG.assert_eq(&output);
}}"#
        )
        .expect("writing to file should succeed");
    }
}
