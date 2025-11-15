// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    Manifest, PackageRef,
    manifest::{GitHubRef, PackageType},
};
use async_trait::async_trait;
use futures::FutureExt;
use miette::Diagnostic;
use qsc_data_structures::{language_features::LanguageFeatures, target::Profile};
use qsc_frontend::compile::get_target_profile_from_entry_point;
use qsc_linter::LintOrGroupConfig;
use rustc_hash::FxHashMap;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};
use thiserror::Error;

pub const GITHUB_SCHEME: &str = "qsharp-github-source";

#[derive(Debug, Clone)]
pub enum ProjectType {
    /// A Q# project. Described by a `qsharp.json` manifest or a single Q# file
    /// Value is the package graph, including all sources and per-package
    /// configuration settings.
    QSharp(PackageGraphSources),
    /// A QASM project. Described by an `OpenQASM` source file and its includes
    /// Value is the collection of sources and all includes.
    OpenQASM(Vec<(Arc<str>, Arc<str>)>),
}

/// Describes a Q# project with all its sources and dependencies resolved.
#[derive(Debug, Clone)]
pub struct Project {
    /// Friendly name, typically based on project directory name
    /// Not guaranteed to be unique. Don't use as a key.
    pub name: Arc<str>,
    /// A path that represents the whole project.
    /// Typically the `qsharp.json` path for projects, or the document path for single files.
    pub path: Arc<str>,
    /// Lint configuration for the project, typically comes from the root `qsharp.json`.
    pub lints: Vec<LintOrGroupConfig>,
    /// Any errors encountered while loading the project.
    pub errors: Vec<Error>,
    /// The type of project. This is used to determine how to load the project.
    pub project_type: ProjectType,
    /// QIR target profile for this project from user source (from manifest or entry point attribute)
    /// Is `None` if the project does not specify a profile.
    pub target_profile: Option<Profile>,
}

impl Project {
    #[must_use]
    /// Given a source file, creates a project that contains just that file and
    /// default configuration options.
    pub fn from_single_file(name: Arc<str>, contents: Arc<str>) -> Self {
        let display_name = PathBuf::from(name.as_ref())
            .file_name()
            .map_or_else(|| name.clone(), |f| f.to_string_lossy().into());
        let source = PackageGraphSources {
            root: PackageInfo {
                sources: vec![(name.clone(), contents.clone())],
                language_features: LanguageFeatures::default(),
                dependencies: FxHashMap::default(),
                package_type: None,
            },
            packages: FxHashMap::default(),
            has_manifest: false,
        };

        let target_profile =
            get_target_profile_from_entry_point(&[(name.clone(), contents)]).map(|(p, _)| p);

        Self {
            path: name,
            name: display_name,
            lints: Vec::default(),
            errors: Vec::default(),
            project_type: ProjectType::QSharp(source),
            target_profile,
        }
    }

    #[must_use]
    /// Returns true if the project is a Q# project with a manifest.
    pub fn has_manifest(&self) -> bool {
        matches!(self.project_type, ProjectType::QSharp(ref sources) if sources.has_manifest)
    }
}

/// This enum represents a filesystem object type. It is analogous to [`std::fs::FileType`].
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum EntryType {
    File,
    Folder,
    Symlink,
    Unknown,
}

/// This trait represents a filesystem object. It is analogous to [`std::fs::DirEntry`].
pub trait DirEntry {
    type Error: Send + Sync;
    fn entry_type(&self) -> Result<EntryType, Self::Error>;
    fn path(&self) -> PathBuf;
    fn entry_extension(&self) -> String {
        self.path()
            .extension()
            .map(|x| x.to_string_lossy().to_string())
            .unwrap_or_default()
    }
    fn entry_name(&self) -> String {
        self.path()
            .file_name()
            .expect("canonicalized symlink cannot end in relative path")
            .to_string_lossy()
            .to_string()
    }
}

/// Errors that can occur during project loading and dependency resolution.
#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error("No `src` directory found for project")]
    #[diagnostic(code("Qsc.Project.NoSrcDir"))]
    NoSrcDir { path: String },

    #[error("Failed to parse manifest: {error}")]
    #[diagnostic(code("Qsc.Project.ManifestParse"))]
    ManifestParse { path: String, error: String },

    #[error("Failed to parse manifest for GitHub dependency {repo}/{owner} : {error}")]
    #[diagnostic(code("Qsc.Project.GitHubManifestParse"))]
    GitHubManifestParse {
        path: String,
        owner: String,
        repo: String,
        error: String,
    },

    #[error("Circular dependency detected between {0} and {1}")]
    #[diagnostic(code("Qsc.Project.CircularDependency"))]
    Circular(String, String),

    #[error("GitHub dependency {0} contains a local dependency {1}, which is not supported")]
    #[diagnostic(code("Qsc.Project.GitHubToLocal"))]
    GitHubToLocal(String, String),

    #[error("File system error: {about_path}: {error}")]
    #[diagnostic(code("Qsc.Project.FileSystem"))]
    FileSystem { about_path: String, error: String },

    #[error("Error reading circuit file: {path}: {error}")]
    #[diagnostic(code("Qsc.Project.Circuit"))]
    Circuit { path: String, error: String },

    #[error("Error fetching from GitHub: {0}")]
    #[diagnostic(code("Qsc.Project.GitHub"))]
    GitHub(String),

    #[error("File {relative_path} is not listed in the `files` field of the manifest")]
    #[diagnostic(help(
        "To avoid unexpected behavior, add this file to the `files` field in the `qsharp.json` manifest"
    ))]
    #[diagnostic(code("Qsc.Project.DocumentNotInProject"))]
    DocumentNotInProject { path: String, relative_path: String },
}

impl Error {
    /// Returns the document path that the error should be associated with when reporting.
    #[must_use]
    pub fn path(&self) -> Option<&String> {
        match self {
            Error::GitHubManifestParse { path, .. }
            | Error::DocumentNotInProject { path, .. }
            | Error::NoSrcDir { path }
            | Error::Circuit { path, .. }
            | Error::ManifestParse { path, .. } => Some(path),
            // Note we don't return the path for `FileSystem` errors,
            // since for most errors such as "file not found", it's more meaningful
            // to report the error for the manifest that was *referencing* the file,
            // rather than for the file itself that provoked the I/O error.
            Error::FileSystem { .. }
            | Error::GitHubToLocal(_, _)
            | Error::Circular(_, _)
            | Error::GitHub(_) => None,
        }
    }
}

type ProjectResult<T> = Result<T, Error>;

/// This trait is used to abstract filesystem logic with regards to Q# projects.
/// A Q# project requires some multi-file structure, but that may not actually be
/// an OS filesystem. It could be a virtual filesystem on vscode.dev, or perhaps a
/// cached implementation. This interface defines the minimal filesystem requirements
/// for the Q# project system to function correctly.
#[async_trait(?Send)]
pub trait FileSystemAsync {
    type Entry: DirEntry;
    /// Given a path, parse its contents and return a tuple representing (`FileName`, `FileContents`).
    async fn read_file(&self, path: &Path) -> miette::Result<(Arc<str>, Arc<str>)>;

    /// Given a path, list its directory contents (if any).
    async fn list_directory(&self, path: &Path) -> miette::Result<Vec<Self::Entry>>;

    /// Given a base path and a relative path, join the segments and normalize
    /// the path, i.e. replace '..', '.', and redundant separators.
    async fn resolve_path(&self, base: &Path, path: &Path) -> miette::Result<PathBuf>;

    /// Given a path to a file hosted on GitHub, fetches it from GitHub and returns the contents.
    async fn fetch_github(
        &self,
        owner: &str,
        repo: &str,
        r#ref: &str,
        path: &str,
    ) -> miette::Result<Arc<str>>;

    /// Given an initial path, fetch files matching <initial_path>/**/*.qs or <initial_path>/**/*.qsc
    async fn collect_project_sources(&self, initial_path: &Path) -> ProjectResult<Vec<PathBuf>> {
        let listing = self
            .list_directory(initial_path)
            .await
            .map_err(|e| Error::FileSystem {
                about_path: initial_path.to_string_lossy().to_string(),
                error: e.to_string(),
            })?;
        if let Some(src_dir) = listing.into_iter().find(|x| {
            let Ok(entry_type) = x.entry_type() else {
                return false;
            };
            entry_type == EntryType::Folder && x.entry_name() == "src"
        }) {
            let paths = self.collect_project_sources_inner(&src_dir.path()).await?;
            let mut resolved_paths = vec![];
            for p in paths {
                // The paths that come back from `list_directory` contain the project
                // directory, but are not normalized (i.e. they may contain extra '..' or '.' components)

                // Strip the project directory prefix
                let relative_to_project = p
                    .strip_prefix(initial_path)
                    .expect("path should be under initial path");

                // Normalize
                resolved_paths.push(
                    self.resolve_path(initial_path, relative_to_project)
                        .await
                        .map_err(|e| Error::FileSystem {
                            about_path: p.to_string_lossy().to_string(),
                            error: e.to_string(),
                        })?,
                );
            }
            return Ok(resolved_paths);
        }
        Err(Error::NoSrcDir {
            path: initial_path.to_string_lossy().to_string(),
        })
    }

    async fn collect_project_sources_inner(
        &self,
        initial_path: &Path,
    ) -> ProjectResult<Vec<PathBuf>> {
        let listing = self
            .list_directory(initial_path)
            .await
            .map_err(|e| Error::FileSystem {
                about_path: initial_path.to_string_lossy().to_string(),
                error: e.to_string(),
            })?;
        let mut files = vec![];
        for item in filter_hidden_files(listing.into_iter()) {
            let extension = item.entry_extension();
            match item.entry_type() {
                Ok(EntryType::File) if extension == "qs" || extension == "qsc" => {
                    files.push(item.path());
                }
                Ok(EntryType::Folder) => {
                    files.append(&mut self.collect_project_sources_inner(&item.path()).await?);
                }
                _ => (),
            }
        }
        Ok(files)
    }

    async fn collect_sources_from_files_field(
        &self,
        project_path: &Path,
        manifest: &Manifest,
    ) -> ProjectResult<Vec<PathBuf>> {
        let mut v = vec![];
        for file in &manifest.files {
            v.push(
                self.resolve_path(project_path, Path::new(&file))
                    .await
                    .map_err(|e| Error::FileSystem {
                        about_path: project_path.to_string_lossy().to_string(),
                        error: e.to_string(),
                    })?,
            );
        }
        Ok(v)
    }

    /// Compares the list of files in the `src` directory with the list of files
    /// in the `files` field, and adds errors if any are missing.
    fn validate_files_list(
        project_path: &Path,
        qs_files: &mut Vec<PathBuf>,
        listed_files: &mut Vec<PathBuf>,
        errors: &mut Vec<Error>,
    ) {
        qs_files.sort();
        listed_files.sort();

        let mut listed_files = listed_files.iter().peekable();

        for item in qs_files.iter() {
            while let Some(&next) = listed_files.peek() {
                if next < item {
                    listed_files.next();
                } else {
                    break;
                }
            }
            if listed_files.peek() != Some(&item) {
                errors.push(Error::DocumentNotInProject {
                    path: item.to_string_lossy().to_string(),
                    relative_path: item
                        .strip_prefix(project_path)
                        .unwrap_or(item)
                        .to_string_lossy()
                        .to_string(),
                });
            }
        }
    }

    /// Given a directory, loads the project sources
    /// and the sources for all its dependencies.
    ///
    /// Any errors that didn't block project load are contained in the
    /// `errors` field of the returned `Project`.
    async fn load_project(
        &self,
        directory: &Path,
        global_cache: Option<&RefCell<PackageCache>>,
    ) -> Result<Project, Vec<Error>> {
        let manifest = self
            .parse_manifest_in_dir(directory)
            .await
            .map_err(|e| vec![e])?;

        let mut errors = vec![];
        let mut packages = FxHashMap::default();
        let mut stack = vec![];

        let root = self
            .read_local_manifest_and_sources(directory, &mut errors)
            .await
            .map_err(|e| vec![e])?;

        let root_path = directory.to_string_lossy().to_string();
        let root_ref = PackageRef::Path { path: root_path };

        self.collect_deps(
            key_for_package_ref(&root_ref),
            &root,
            global_cache.unwrap_or(&RefCell::new(FxHashMap::default())),
            &mut stack,
            &mut packages,
            &mut errors,
            &root_ref,
        )
        .await;

        let name = directory
            .file_name()
            .map(|f| f.to_string_lossy().into())
            .unwrap_or(format!("Q# project at {}", directory.display()))
            .into();

        let manifest_path = self
            .resolve_path(directory, Path::new("qsharp.json"))
            .await
            .map_err(|e| {
                vec![Error::FileSystem {
                    about_path: directory.to_string_lossy().to_string(),
                    error: e.to_string(),
                }]
            })?
            .to_string_lossy()
            .into();

        Ok(Project {
            lints: manifest.lints,
            errors,
            name,
            path: manifest_path,
            project_type: ProjectType::QSharp(PackageGraphSources {
                root,
                packages,
                has_manifest: true,
            }),
            target_profile: manifest
                .target_profile
                .as_deref()
                .and_then(|s| Profile::from_str(s).ok()),
        })
    }

    /// Given an `OpenQASM` file, loads the project sources
    /// and the sources for all its dependencies.
    ///
    /// Any errors that didn't block project load are contained in the
    /// `errors` field of the returned `Project`.
    async fn load_openqasm_project(&self, path: &Path, source: Option<Arc<str>>) -> Project {
        crate::openqasm::load_project(self, path, source).await
    }

    /// Given a directory, attempts to parse a `qsharp.json` in that directory
    /// according to the manifest schema.
    async fn parse_manifest_in_dir(&self, directory: &Path) -> ProjectResult<Manifest> {
        let manifest_path = self
            .resolve_path(directory, Path::new("qsharp.json"))
            .await
            .map_err(|e| Error::FileSystem {
                about_path: directory.to_string_lossy().to_string(),
                error: e.to_string(),
            })?;
        let (_, manifest_content) =
            self.read_file(&manifest_path)
                .await
                .map_err(|e| Error::FileSystem {
                    about_path: manifest_path.to_string_lossy().to_string(),
                    error: e.to_string(),
                })?;
        let manifest = serde_json::from_str::<Manifest>(&manifest_content).map_err(|e| {
            Error::ManifestParse {
                path: manifest_path.to_string_lossy().to_string(),
                error: e.to_string(),
            }
        })?;
        Ok(manifest)
    }

    /// Load the sources for a single package at the given directory. Also load its
    /// dependency information but don't recurse into dependencies yet.
    ///
    /// Any errors that didn't block project load are accumulated into the `errors` vector.
    async fn read_local_manifest_and_sources(
        &self,
        directory: &Path,
        errors: &mut Vec<Error>,
    ) -> ProjectResult<PackageInfo> {
        let manifest = self.parse_manifest_in_dir(directory).await?;

        // For local packages, we include all source files under the `src/`
        // directory, even if a `files` field is present.
        //
        // If there are files under `src/` that are missing from the `files` field,
        // we assume that's user error, and we report it.
        //
        // Since the omission is already reported as an error here, we go ahead and include
        // all the found files in the package sources. This way compilation
        // can continue as the user probably intended, without compounding errors.

        let mut all_project_files = self.collect_project_sources(directory).await?;

        let mut listed_files = self
            .collect_sources_from_files_field(directory, &manifest)
            .await?;

        if !listed_files.is_empty() {
            Self::validate_files_list(directory, &mut all_project_files, &mut listed_files, errors);
        }

        let mut sources = Vec::with_capacity(all_project_files.len());
        for path in all_project_files {
            sources.push(self.read_file(&path).await.map_err(|e| Error::FileSystem {
                about_path: path.to_string_lossy().to_string(),
                error: e.to_string(),
            })?);
        }

        let mut dependencies = FxHashMap::default();

        // For any local dependencies, convert relative paths to absolute,
        // so that multiple references to the same package, from different packages,
        // get merged correctly.
        for (alias, mut dep) in manifest.dependencies {
            if let PackageRef::Path { path: dep_path } = &mut dep {
                *dep_path = self
                    .resolve_path(directory, &PathBuf::from(dep_path.clone()))
                    .await
                    .map_err(|e| Error::FileSystem {
                        about_path: directory.to_string_lossy().to_string(),
                        error: e.to_string(),
                    })?
                    .to_string_lossy()
                    .into();
            }
            dependencies.insert(alias.into(), key_for_package_ref(&dep));
        }

        Ok(PackageInfo {
            sources,
            language_features: LanguageFeatures::from_iter(manifest.language_features),
            dependencies,
            package_type: manifest.package_type,
        })
    }

    /// Load the sources for a single package at the given GitHub ref. Also load its
    /// dependency information but don't recurse into dependencies yet.
    async fn read_github_manifest_and_sources(
        &self,
        dep: &GitHubRef,
    ) -> ProjectResult<PackageInfo> {
        let path_trimmed_seps = dep
            .path
            .as_ref()
            .map(|p| {
                p.strip_suffix('/')
                    .unwrap_or(p)
                    .strip_prefix('/')
                    .unwrap_or(p)
            })
            .unwrap_or_default();

        let manifest_path = format!("{path_trimmed_seps}/qsharp.json",);
        let manifest_content = self
            .fetch_github(&dep.owner, &dep.repo, &dep.r#ref, &manifest_path)
            .await
            .map_err(|e| Error::GitHub(e.to_string()))?;

        let manifest = serde_json::from_str::<Manifest>(&manifest_content).map_err(|e| {
            Error::GitHubManifestParse {
                path: format!(
                    "{GITHUB_SCHEME}:{}/{}/{}{}/qsharp.json",
                    dep.owner,
                    dep.repo,
                    dep.r#ref,
                    if path_trimmed_seps.is_empty() {
                        String::new()
                    } else {
                        format!("/{path_trimmed_seps}")
                    }
                ),
                owner: dep.owner.clone(),
                repo: dep.repo.clone(),
                error: e.to_string(),
            }
        })?;

        // Look at the files field in the manifest to find the sources.
        let mut sources = vec![];
        for file in &manifest.files {
            let path = if path_trimmed_seps.is_empty() {
                format!("/{file}")
            } else {
                format!("/{path_trimmed_seps}/{file}")
            };
            let contents = self
                .fetch_github(&dep.owner, &dep.repo, &dep.r#ref, &path)
                .await
                .map_err(|e| Error::GitHub(e.to_string()))?;

            // Use the well-known URI scheme for the generated GitHub source paths.
            // This will allow the editor to recognize these URIs as GitHub sources
            // and open them using cached contents.
            sources.push((
                format!(
                    "{GITHUB_SCHEME}:{}/{}/{}{path}",
                    dep.owner, dep.repo, dep.r#ref
                )
                .into(),
                contents,
            ));
        }

        Ok(PackageInfo {
            sources,
            language_features: LanguageFeatures::from_iter(&manifest.language_features),
            dependencies: manifest
                .dependencies
                .into_iter()
                .map(|(k, v)| (k.into(), key_for_package_ref(&v)))
                .collect(),
            package_type: manifest.package_type,
        })
    }

    /// Load the sources and dependency information for a single package,
    /// using a previously cached version if available.
    async fn read_manifest_and_sources(
        &self,
        global_cache: &RefCell<PackageCache>,
        key: PackageKey,
        this_pkg: &PackageRef,
        errors: &mut Vec<Error>,
    ) -> ProjectResult<PackageInfo> {
        match this_pkg {
            PackageRef::GitHub { github } => {
                {
                    let cache = global_cache.borrow();
                    if let Some(cached) = cache.get(&key) {
                        return cached.clone();
                    }
                }

                let result = self.read_github_manifest_and_sources(github).await;

                let mut cache = global_cache.borrow_mut();
                cache.insert(key, result.clone());

                result
            }
            PackageRef::Path { path } => {
                // Local dependencies are not cached at the moment, to make the multi-project
                // editing experience as intuitive as possible. This may change if we start
                // hitting perf issues, but careful consideration is needed into when to
                // invalidate the cache.
                self.read_local_manifest_and_sources(PathBuf::from(path.clone()).as_path(), errors)
                    .await
            }
        }
    }

    /// Recursive method to load sources for all dependencies and their
    /// dependencies, etc.
    #[allow(clippy::too_many_arguments)]
    async fn collect_deps(
        &self,
        key: Arc<str>,
        pkg: &PackageInfo,
        global_cache: &RefCell<PackageCache>,
        stack: &mut Vec<PackageKey>,
        packages: &mut FxHashMap<PackageKey, PackageInfo>,
        errors: &mut Vec<Error>,
        this_pkg: &PackageRef,
    ) {
        stack.push(key.clone());

        for (alias, dep_key) in &pkg.dependencies {
            if stack.contains(dep_key) {
                errors.push(Error::Circular(key.to_string(), dep_key.to_string()));
                continue;
            }

            let dependency = package_ref_from_key(dep_key);
            if matches!(dependency, PackageRef::Path { .. })
                && matches!(this_pkg, PackageRef::GitHub { .. })
            {
                errors.push(Error::GitHubToLocal(key.to_string(), alias.to_string()));
                continue;
            }

            let dep_result = self
                .read_manifest_and_sources(global_cache, dep_key.clone(), &dependency, errors)
                .await;

            match dep_result {
                Ok(pkg) => {
                    self.collect_deps(
                        dep_key.clone(),
                        &pkg,
                        global_cache,
                        stack,
                        packages,
                        errors,
                        &dependency,
                    )
                    .await;
                    packages.insert(dep_key.clone(), pkg);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }

        stack.pop();
    }
}

/// Filters out any hidden files (files that start with '.')
fn filter_hidden_files<Entry: DirEntry>(
    listing: impl Iterator<Item = Entry>,
) -> impl Iterator<Item = Entry> {
    listing.filter(|x| !x.entry_name().starts_with('.'))
}

/// We're using JSON to generate a key for the packages,
/// but something more readable would also be okay as long as we can
/// guarantee uniqueness.
#[must_use]
pub fn key_for_package_ref(dep: &PackageRef) -> PackageKey {
    serde_json::to_string(dep)
        .expect("dependency should be serializable")
        .into()
}

#[must_use]
pub fn package_ref_from_key(key: &PackageKey) -> PackageRef {
    serde_json::from_str(key).expect("dependency should be deserializable")
}

/// A `PackageKey` is a global unique identifier for a package, and it's
/// simply generated from the dependency information in the manifest
/// (local or GitHub path).
type PackageKey = Arc<str>;

/// A `PackageAlias` is the name that a package uses to refer to one of its
/// dependencies. The same package can live under different aliases in the
/// same package graph.
type PackageAlias = Arc<str>;

/// Long-lived cache that can optionally be used for loading packages.
pub type PackageCache = FxHashMap<PackageKey, ProjectResult<PackageInfo>>;

type Sources = Vec<(Arc<str>, Arc<str>)>;

#[derive(Clone, Debug)]
pub struct PackageInfo {
    pub sources: Sources,
    pub language_features: LanguageFeatures,
    pub dependencies: FxHashMap<PackageAlias, PackageKey>,
    pub package_type: Option<PackageType>,
}

#[derive(Clone, Debug)]
pub struct PackageGraphSources {
    pub root: PackageInfo,
    pub packages: FxHashMap<PackageKey, PackageInfo>,
    pub has_manifest: bool,
}

#[derive(Debug)]
pub struct DependencyCycle;

pub type OrderedDependencies = Vec<(Arc<str>, PackageInfo)>;

impl PackageGraphSources {
    /// Produces an ordered vector over the packages in the order they should be compiled
    pub fn compilation_order(self) -> (Result<OrderedDependencies, DependencyCycle>, PackageInfo) {
        // The order is defined by which packages depend on which other packages
        // For example, if A depends on B which depends on C, then we compile C, then B, then A
        // If there are cycles, this is an error, and we will report it as such
        let mut in_degree: FxHashMap<&str, usize> = FxHashMap::default();
        let mut graph: FxHashMap<&str, Vec<&str>> = FxHashMap::default();

        // Initialize the graph and in-degrees
        // This graph contains all direct and transient dependencies
        // and tracks which packages depend on which other packages,
        // as well as the in-degree (quantity of dependents) of each package
        for (key, package_info) in &self.packages {
            in_degree.entry(key).or_insert(0);
            for dep in package_info.dependencies.values() {
                graph.entry(dep).or_default().push(key);
                *in_degree.entry(key).or_insert(0) += 1;
            }
        }

        // this queue contains all packages with in-degree 0
        // these packages are valid starting points for the build order,
        // as they don't depend on any other packages.
        // If there are no dependency cycles, then all packages will be reachable
        // via this queue of build order entry points.
        let mut queue: Vec<&str> = in_degree
            .iter()
            .filter_map(|(key, &deg)| if deg == 0 { Some(*key) } else { None })
            .collect();

        let mut sorted_keys = Vec::new();

        // from all build order entry points (the initial value of `queue`), we
        // can build the build order by visiting each package and decrementing
        // the in-degree of its dependencies. If the in-degree of a dependency
        // reaches 0, then it can be added to the queue of build order entry points,
        // as all of its dependents have been built.
        while let Some(node) = queue.pop() {
            sorted_keys.push(node.to_string());
            if let Some(neighbors) = graph.get(node) {
                for &neighbor in neighbors {
                    let count = in_degree
                        .get_mut(neighbor)
                        .expect("graph pre-calculated this");
                    *count -= 1;
                    if *count == 0 {
                        queue.push(neighbor);
                    }
                }
            }
        }

        let mut sorted_packages = self.packages.into_iter().collect::<Vec<_>>();
        let mut cycle_detected = false;
        sorted_packages.sort_by_key(|(a_key, _pkg)| {
            sorted_keys
                .iter()
                .position(|key| key.as_str() == &**a_key)
                .unwrap_or_else(|| {
                    // The only situation in which a package is not in the build order
                    // is if there is a cycle in the dependency graph.
                    // this is because the build order must start with a package that
                    // has zero dependencies. If all packages have dependencies, then
                    // a cycle must exist.
                    cycle_detected = true;
                    sorted_keys.len()
                })
        });

        if cycle_detected {
            return (Err(DependencyCycle), self.root);
        }

        log::debug!("build plan: {:#?}", sorted_keys);

        (Ok(sorted_packages), self.root)
    }

    #[must_use]
    pub fn with_no_dependencies(
        sources: Vec<(Arc<str>, Arc<str>)>,
        language_features: LanguageFeatures,
        package_type: Option<PackageType>,
    ) -> Self {
        Self {
            root: PackageInfo {
                sources,
                language_features,
                dependencies: FxHashMap::default(),
                package_type,
            },
            packages: FxHashMap::default(),
            has_manifest: false,
        }
    }
}

/// This trait is used to abstract filesystem logic with regards to Q# projects.
/// A Q# project requires some multi-file structure, but that may not actually be
/// an OS filesystem. It could be a virtual filesystem on vscode.dev, or perhaps a
/// cached implementation. This interface defines the minimal filesystem requirements
/// for the Q# project system to function correctly.
pub trait FileSystem {
    type Entry: DirEntry;
    /// Given a path, parse its contents and return a tuple representing (`FileName`, `FileContents`).
    fn read_file(&self, path: &Path) -> miette::Result<(Arc<str>, Arc<str>)>;

    /// Given a path, list its directory contents (if any).
    fn list_directory(&self, path: &Path) -> miette::Result<Vec<Self::Entry>>;

    fn resolve_path(&self, base: &Path, path: &Path) -> miette::Result<PathBuf>;

    fn fetch_github(
        &self,
        owner: &str,
        repo: &str,
        r#ref: &str,
        path: &str,
    ) -> miette::Result<Arc<str>>;

    fn load_project(
        &self,
        directory: &Path,
        global_cache: Option<&RefCell<PackageCache>>,
    ) -> Result<Project, Vec<Error>> {
        // Rather than rewriting all the async code in the project loader,
        // we call the async implementation here, doing some tricks to make it
        // run synchronously.

        let fs = ToFileSystemAsync { fs: self };

        // WARNING: This will panic if there are *any* await points in the
        // load_project implementation. Right now, we know that will never be the case
        // because we just passed in our synchronous FS functions to the project loader.
        // Proceed with caution if you make the `FileSystemAsync` implementation any
        // more complex.
        FutureExt::now_or_never(fs.load_project(directory, global_cache))
            .expect("load_project should never await")
    }

    fn load_openqasm_project(&self, path: &Path, source: Option<Arc<str>>) -> Project {
        // Rather than rewriting all the async code in the project loader,
        // we call the async implementation here, doing some tricks to make it
        // run synchronously.

        let fs = ToFileSystemAsync { fs: self };

        // WARNING: This will panic if there are *any* await points in the
        // load_openqasm_project implementation. Right now, we know that will never be the case
        // because we just passed in our synchronous FS functions to the project loader.
        // Proceed with caution if you make the `FileSystemAsync` implementation any
        // more complex.
        FutureExt::now_or_never(fs.load_openqasm_project(path, source))
            .expect("load_openqasm_project should never await")
    }
}

/// Trivial wrapper to turn a `FileSystem` into a `FileSystemAsync`
struct ToFileSystemAsync<'a, FS>
where
    FS: ?Sized,
{
    fs: &'a FS,
}

#[async_trait(?Send)]
impl<FS, E> FileSystemAsync for ToFileSystemAsync<'_, FS>
where
    E: DirEntry,
    FS: FileSystem<Entry = E> + ?Sized,
{
    type Entry = E;

    async fn read_file(&self, path: &Path) -> miette::Result<(Arc<str>, Arc<str>)> {
        self.fs.read_file(path)
    }

    async fn list_directory(&self, path: &Path) -> miette::Result<Vec<Self::Entry>> {
        self.fs.list_directory(path)
    }

    async fn resolve_path(&self, base: &Path, path: &Path) -> miette::Result<PathBuf> {
        self.fs.resolve_path(base, path)
    }

    async fn fetch_github(
        &self,
        owner: &str,
        repo: &str,
        r#ref: &str,
        path: &str,
    ) -> miette::Result<Arc<str>> {
        self.fs.fetch_github(owner, repo, r#ref, path)
    }
}
