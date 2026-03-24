// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An in-memory file system implementation for the unit tests.
//! Method signatures and behaviors are somewhat specialized to the way the
//! language service expects the fs to behave; if we want to reuse this in other
//! tests, it could use some work to make methods a little more general.

use async_trait::async_trait;
use qsc_project::{EntryType, FileSystem, JSFileEntry, JSProjectHost};
use rustc_hash::FxHashMap;
use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::Arc};

pub(crate) enum FsNode {
    Dir(FxHashMap<Arc<str>, FsNode>),
    File(Arc<str>),
}

/// A file system operation error.
#[derive(Debug)]
pub(crate) enum FsError {
    NotFound,
}

impl FsNode {
    pub fn read_file(&self, file: String) -> miette::Result<(Arc<str>, Arc<str>)> {
        let mut curr = Some(self);

        for part in file.split('/') {
            curr = curr.and_then(|node| match node {
                FsNode::Dir(dir) => dir.get(part),
                FsNode::File(_) => None,
            });
        }

        match curr {
            Some(FsNode::File(contents)) => Ok((file.into(), contents.clone())),
            Some(FsNode::Dir(_)) | None => Err(miette::Error::msg("file not found")),
        }
    }

    pub fn write_file(&mut self, file: &str, contents: &str) -> Result<(), FsError> {
        let mut curr = Some(self);

        for part in file.split('/') {
            curr = curr.and_then(|node| match node {
                FsNode::Dir(dir) => dir.get_mut(part),
                FsNode::File(_) => None,
            });
        }

        if let Some(FsNode::File(curr_contents)) = curr {
            *curr_contents = contents.into();
            Ok(())
        } else {
            Err(FsError::NotFound)
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn list_directory(&self, dir_name: String) -> Vec<JSFileEntry> {
        let mut curr = Some(self);

        for part in dir_name.split('/') {
            curr = curr.and_then(|node| match node {
                FsNode::Dir(dir) => dir.get(part),
                FsNode::File(_) => None,
            });
        }

        match curr {
            Some(FsNode::Dir(dir)) => dir
                .iter()
                .map(|(name, node)| JSFileEntry {
                    name: format!("{dir_name}/{name}"),
                    r#type: match node {
                        FsNode::Dir(_) => EntryType::Folder,
                        FsNode::File(_) => EntryType::File,
                    },
                })
                .collect(),
            Some(FsNode::File(_)) | None => Vec::default(),
        }
    }

    pub fn resolve_path(base: &str, path: &str) -> String {
        let mut parts = base.split('/').collect::<Vec<_>>();

        for part in path.split('/') {
            if part == ".." {
                match parts.pop() {
                    Some(_) => continue,
                    None => panic!("path traversal outside of root"),
                }
            }
            parts.push(part);
        }

        parts.join("/")
    }

    pub fn find_manifest_directory(&self, file: &str) -> Option<PathBuf> {
        let mut curr = Some(self);
        let mut curr_path = String::new();
        let mut last_manifest_dir = None;

        // Traverse the path, keeping track of the last directory containing a
        // manifest file. If we find a directory containing a manifest file and
        // the next part of the path is "src", that's our manifest directory.
        for part in file.split('/') {
            curr = curr.and_then(|curr| match curr {
                FsNode::Dir(dir) => {
                    if let Some(FsNode::File(_)) = dir.get("qsharp.json")
                        && part == "src"
                    {
                        last_manifest_dir = Some(curr_path.trim_end_matches('/').to_string());
                    }
                    curr_path = format!("{curr_path}{part}/");
                    dir.get(part)
                }
                FsNode::File(_) => None,
            });
        }

        match curr {
            Some(FsNode::Dir(_)) | None => None,
            Some(FsNode::File(_)) => last_manifest_dir.map(Into::into),
        }
    }

    pub fn remove(&mut self, path: &str) {
        let mut curr_parent = Some(self);
        let mut curr_name = None;

        for part in path.split('/') {
            if let Some(name) = curr_name
                && let Some(FsNode::Dir(dir)) = curr_parent
            {
                curr_parent = dir.get_mut(name);
            }

            curr_name = Some(part);
        }

        let name = curr_name.expect("file name should have been set");

        match curr_parent {
            Some(FsNode::Dir(dir)) => dir.remove(name),
            Some(FsNode::File(_)) | None => panic!("path {path} does not exist"),
        };
    }
}

impl FileSystem for FsNode {
    type Entry = JSFileEntry;

    fn read_file(&self, path: &std::path::Path) -> miette::Result<(Arc<str>, Arc<str>)> {
        self.read_file(path.to_string_lossy().into())
    }

    fn list_directory(&self, path: &std::path::Path) -> miette::Result<Vec<Self::Entry>> {
        Ok(self.list_directory(path.to_string_lossy().into()))
    }

    fn resolve_path(
        &self,
        base: &std::path::Path,
        path: &std::path::Path,
    ) -> miette::Result<std::path::PathBuf> {
        Ok(Self::resolve_path(&base.to_string_lossy(), &path.to_string_lossy()).into())
    }

    fn fetch_github(
        &self,
        _owner: &str,
        _repo: &str,
        _ref: &str,
        _path: &str,
    ) -> miette::Result<Arc<str>> {
        Err(miette::Error::msg(
            "github references not supported for this file system",
        ))
    }
}

pub(crate) fn dir<const COUNT: usize>(
    name: &str,
    contents: [(Arc<str>, FsNode); COUNT],
) -> (Arc<str>, FsNode) {
    (name.into(), FsNode::Dir(contents.into_iter().collect()))
}

pub(crate) fn file(name: &str, contents: &str) -> (Arc<str>, FsNode) {
    (name.into(), FsNode::File(Arc::from(contents)))
}

pub(crate) struct TestProjectHost {
    pub fs: Rc<RefCell<FsNode>>,
}

#[async_trait(?Send)]
impl JSProjectHost for TestProjectHost {
    async fn read_file(&self, uri: &str) -> miette::Result<(Arc<str>, Arc<str>)> {
        self.fs.borrow().read_file(uri.to_string())
    }

    async fn list_directory(&self, uri: &str) -> Vec<JSFileEntry> {
        self.fs.borrow().list_directory(uri.to_string())
    }

    async fn resolve_path(&self, base: &str, path: &str) -> miette::Result<Arc<str>> {
        self.fs
            .borrow()
            .resolve_path(PathBuf::from(base).as_path(), PathBuf::from(path).as_path())
            .map(|p| p.to_string_lossy().into())
    }

    async fn find_manifest_directory(&self, doc_uri: &str) -> Option<Arc<str>> {
        self.fs
            .borrow()
            .find_manifest_directory(doc_uri)
            .map(|p| p.to_string_lossy().into())
    }

    async fn fetch_github(
        &self,
        _owner: &str,
        _repo: &str,
        _ref: &str,
        _path: &str,
    ) -> miette::Result<Arc<str>> {
        unimplemented!()
    }
}
