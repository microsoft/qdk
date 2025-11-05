// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use miette::miette;
use pyo3::{
    exceptions::PyException,
    prelude::*,
    types::{PyDict, PyList, PyString, PyTuple},
};
use qsc::project::{DirEntry, EntryType, FileSystem};

pub(crate) fn file_system(
    py: Python,
    read_file: PyObject,
    list_directory: PyObject,
    resolve_path: PyObject,
    fetch_github: PyObject,
) -> impl FileSystem {
    Py {
        py,
        fs_hooks: FsHooks {
            read_file,
            list_directory,
            resolve_path,
            fetch_github,
        },
    }
}

struct FsHooks {
    read_file: PyObject,
    list_directory: PyObject,
    resolve_path: PyObject,
    fetch_github: PyObject,
}

#[derive(Debug)]
struct Entry {
    ty: EntryType,
    path: String,
    name: String,
}

impl DirEntry for Entry {
    type Error = pyo3::PyErr;

    fn entry_type(&self) -> Result<EntryType, Self::Error> {
        Ok(self.ty)
    }

    fn entry_name(&self) -> String {
        self.name.clone()
    }

    fn path(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }
}

struct Py<'a> {
    pub py: Python<'a>,
    fs_hooks: FsHooks,
}

impl FileSystem for Py<'_> {
    type Entry = Entry;

    fn read_file(&self, path: &Path) -> miette::Result<(Arc<str>, Arc<str>)> {
        read_file(self.py, &self.fs_hooks.read_file, path).map_err(|e| diagnostic_from(self.py, &e))
    }

    fn list_directory(&self, path: &Path) -> miette::Result<Vec<Self::Entry>> {
        list_directory(self.py, &self.fs_hooks.list_directory, path)
            .map_err(|e| diagnostic_from(self.py, &e))
    }

    fn resolve_path(&self, base: &Path, path: &Path) -> miette::Result<PathBuf> {
        resolve_path(self.py, &self.fs_hooks.resolve_path, base, path)
            .map_err(|e| diagnostic_from(self.py, &e))
    }

    fn fetch_github(
        &self,
        owner: &str,
        repo: &str,
        r#ref: &str,
        path: &str,
    ) -> miette::Result<Arc<str>> {
        fetch_github(
            self.py,
            &self.fs_hooks.fetch_github,
            owner,
            repo,
            r#ref,
            path,
        )
        .map_err(|e| diagnostic_from(self.py, &e))
    }
}

fn read_file(py: Python, read_file: &PyObject, path: &Path) -> PyResult<(Arc<str>, Arc<str>)> {
    let read_file_result = read_file.call1(py, PyTuple::new(py, &[path.to_string_lossy()])?)?;

    let tuple = read_file_result.downcast_bound::<PyTuple>(py)?;

    Ok((get_tuple_string(tuple, 0)?, get_tuple_string(tuple, 1)?))
}

fn list_directory(py: Python, list_directory: &PyObject, path: &Path) -> PyResult<Vec<Entry>> {
    let list_directory_result =
        list_directory.call1(py, PyTuple::new(py, &[path.to_string_lossy()])?)?;

    list_directory_result
        .downcast_bound::<PyList>(py)?
        .into_iter()
        .map(|e| {
            let dict = e.downcast::<PyDict>()?;
            let entry_type = match get_dict_string(dict, "type")?.to_string().as_str() {
                "file" => EntryType::File,
                "folder" => EntryType::Folder,
                "symlink" => EntryType::Symlink,
                _ => Err(PyException::new_err(
                    "expected valid value for `type` in list_directory result",
                ))?,
            };

            Ok(Entry {
                ty: entry_type,
                path: get_dict_string(dict, "path")?.to_string(),
                name: get_dict_string(dict, "entry_name")?.to_string(),
            })
        })
        .collect() // Returns all values if all Ok, or first Err
}

fn resolve_path(
    py: Python,
    resolve_path: &PyObject,
    base: &Path,
    path: &Path,
) -> PyResult<PathBuf> {
    let resolve_path_result = resolve_path.call1(
        py,
        PyTuple::new(py, &[base.to_string_lossy(), path.to_string_lossy()])?,
    )?;

    Ok(PathBuf::from(
        resolve_path_result
            .downcast_bound::<PyString>(py)?
            .str()?
            .to_string(),
    ))
}

fn fetch_github(
    py: Python,
    fetch_github: &PyObject,
    owner: &str,
    repo: &str,
    r#ref: &str,
    path: &str,
) -> PyResult<Arc<str>> {
    let fetch_github_result =
        fetch_github.call1(py, PyTuple::new(py, [owner, repo, r#ref, path])?)?;

    Ok(fetch_github_result
        .downcast_bound::<PyString>(py)?
        .to_string()
        .into())
}

fn get_tuple_string(tuple: &Bound<'_, PyTuple>, index: usize) -> PyResult<Arc<str>> {
    Ok(tuple
        .get_item(index)?
        .downcast::<PyString>()?
        .to_string()
        .into())
}

fn get_dict_string<'a>(dict: &Bound<'a, PyDict>, key: &'a str) -> PyResult<Bound<'a, PyString>> {
    match dict.get_item(key)? {
        Some(item) => Ok(item.downcast::<PyString>()?.str()?),
        None => Err(PyException::new_err(format!("missing key `{key}` in dict"))),
    }
}

fn diagnostic_from(py: Python<'_>, err: &PyErr) -> miette::Report {
    if let Some(traceback) = err.traceback(py) {
        match traceback.format() {
            Ok(traceback) => miette!(format!("{err}\n{traceback}",)),
            Err(traceback_err) => {
                miette!(format!("{err}\nerror getting traceback: {traceback_err}",))
            }
        }
    } else {
        miette!(err.to_string())
    }
}
