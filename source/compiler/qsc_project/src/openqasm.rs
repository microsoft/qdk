// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod integration_tests;

use super::{FileSystemAsync, Project};
use qsc_data_structures::target::Profile;
use qsc_qasm::parser::ast::{PathKind, Program, StmtKind};
use rustc_hash::FxHashSet;
use std::{path::Path, str::FromStr as _, sync::Arc};

pub async fn load_project<T, P: AsRef<Path>>(
    project_host: &T,
    path: P,
    source: Option<Arc<str>>,
) -> Project
where
    T: FileSystemAsync + ?Sized,
{
    let mut sources = Vec::<(Arc<str>, Arc<str>)>::new();
    let mut loaded_files = FxHashSet::default();
    let mut pending_includes = vec![];
    let mut errors = vec![];
    let mut target_profile = None;

    let path = Arc::from(path.as_ref().to_string_lossy().as_ref());
    match source {
        Some(source) => {
            let (program, _errors) = qsc_qasm::parser::parse(source.as_ref());
            target_profile = get_first_profile_pragma(&program);
            let includes = get_includes(&program, &path);
            pending_includes.extend(includes);
            loaded_files.insert(path.clone());
            sources.push((path.clone(), source.clone()));
        }
        None => {
            match project_host.read_file(Path::new(path.as_ref())).await {
                Ok((file, source)) => {
                    // load the root file
                    let (program, _errors) = qsc_qasm::parser::parse(source.as_ref());
                    target_profile = get_first_profile_pragma(&program);
                    let includes = get_includes(&program, &file);
                    pending_includes.extend(includes);
                    loaded_files.insert(file.clone());
                    sources.push((file, source.clone()));
                }
                Err(e) => {
                    // If we can't read the file, we create a project with an error.
                    // This is a special case where we can't load the project at all.
                    errors.push(super::project::Error::FileSystem {
                        about_path: path.to_string(),
                        error: e.to_string(),
                    });
                    return Project {
                        path: path.clone(),
                        name: get_file_name_from_uri(&path),
                        lints: Vec::default(),
                        errors,
                        project_type: super::ProjectType::OpenQASM(vec![]),
                        target_profile,
                    };
                }
            }
        }
    }

    while let Some((current, include)) = pending_includes.pop() {
        // Resolve relative path, this works for both FS and URI paths.
        let resolved_path = {
            let current_path = Path::new(current.as_ref());
            let parent_dir = current_path.parent().unwrap_or(Path::new("."));
            let target_path = Path::new(include.as_ref());

            match project_host.resolve_path(parent_dir, target_path).await {
                Ok(resolved) => Arc::from(resolved.to_string_lossy().as_ref()),
                Err(_) => include.clone(),
            }
        };

        if loaded_files.contains(&resolved_path) {
            // We've already loaded this include, so skip it.
            // We'll let the source resolver handle any duplicates.
            // and cyclic dependency errors.
            continue;
        }

        // At this point, we have a valid include path that we need to try to load.
        // Any file read errors after the root are ignored,
        // the parser will handle them as part of full parsing.
        if let Ok((file, source)) = project_host
            .read_file(Path::new(resolved_path.as_ref()))
            .await
        {
            let (program, _errors) = qsc_qasm::parser::parse(source.as_ref());
            let includes = get_includes(&program, &file);
            pending_includes.extend(includes);
            loaded_files.insert(file.clone());
            sources.push((file, source.clone()));
        }
    }

    Project {
        path: path.clone(),
        name: get_file_name_from_uri(&path),
        lints: Vec::default(),
        errors,
        project_type: super::ProjectType::OpenQASM(sources),
        target_profile,
    }
}

/// Returns a vector of all includes found in the given `Program`.
/// Each include is represented as a tuple containing:
/// - The parent file path (as an `Arc<str>`)
/// - The filename of the included file (as an `Arc<str>`)
fn get_includes(program: &Program, parent: &Arc<str>) -> Vec<(Arc<str>, Arc<str>)> {
    program
        .statements
        .iter()
        .filter_map(|stmt| {
            if let StmtKind::Include(include) = &*stmt.kind {
                if matches!(
                    include.filename.to_lowercase().as_ref(),
                    "stdgates.inc" | "qelib1.inc" | "qdk.inc"
                ) {
                    return None;
                }
                Some((parent.clone(), include.filename.clone()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
}

fn get_file_name_from_uri(uri: &Arc<str>) -> Arc<str> {
    let path = Path::new(uri.as_ref());

    // Extract the file name or return the original URI if it fails
    path.file_name()
        .and_then(|name| name.to_str().map(|s| s.to_string().into()))
        .map_or_else(|| uri.clone(), |f| f)
}

pub fn get_first_profile_pragma(program: &Program) -> Option<Profile> {
    for stmt in &program.statements {
        if let StmtKind::Pragma(pragma) = &*stmt.kind {
            let name_str = pragma
                .identifier
                .as_ref()
                .map_or_else(String::new, PathKind::as_string);

            if name_str.to_lowercase() == "qdk.qir.profile"
                && let Some(ref value) = pragma.value
            {
                return Profile::from_str(value.as_ref()).ok();
            }
        }
    }
    None
}
