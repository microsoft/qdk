// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{diagnostic::project_errors_into_qsharp_errors, serializable_type};
use async_trait::async_trait;
use miette::Report;
use qsc::{
    LanguageFeatures, TargetCapabilityFlags, linter::LintOrGroupConfig, packages::BuildableProgram,
};
use qsc_project::{EntryType, FileSystemAsync, JSFileEntry, JSProjectHost, PackageCache};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, iter::FromIterator, path::Path, rc::Rc, str::FromStr, sync::Arc};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const IPROJECT_HOST: &'static str = r#"
export interface IProjectHost {
    readFile(uri: string): Promise<string | null>;
    listDirectory(uri: string): Promise<[string, number][]>;
    resolvePath(base: string, path: string): Promise<string | null>;
    fetchGithub(owner: string, repo: string, ref: string, path: string): Promise<string>;
    findManifestDirectory(docUri: string): Promise<string | null>;
}

/**
 * Copy of the ProgramConfig type defined in compiler.ts,
 * but with all the properties required and filled in with defaults where necessary.
 */
export interface IProgramConfig {
    packageGraphSources: IPackageGraphSources;
    profile: TargetProfile;
    projectType: ProjectType;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "IProjectHost")]
    pub type ProjectHost;

    // Methods of `IProjectHost``, expected to be implemented JS-side
    #[wasm_bindgen(method, structural, catch)]
    async fn readFile(this: &ProjectHost, uri: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, structural)]
    async fn listDirectory(this: &ProjectHost, uri: &str) -> JsValue;

    #[wasm_bindgen(method, structural, catch)]
    async fn resolvePath(this: &ProjectHost, base: &str, path: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, structural, catch)]
    async fn fetchGithub(
        this: &ProjectHost,
        owner: &str,
        repo: &str,
        r#ref: &str,
        path: &str,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, structural)]
    async fn findManifestDirectory(this: &ProjectHost, docUri: &str) -> JsValue;

    /// Alias for an array of [sourceName, sourceContents] tuples
    #[wasm_bindgen(typescript_type = "[string, string][]")]
    pub type ProjectSources;

    #[wasm_bindgen(typescript_type = "IProgramConfig")]
    pub type ProgramConfig;

    // Getters for IProgramConfig
    #[wasm_bindgen(method, getter, structural)]
    fn packageGraphSources(this: &ProgramConfig) -> IPackageGraphSources;

    #[wasm_bindgen(method, getter, structural)]
    fn profile(this: &ProgramConfig) -> String;

    #[wasm_bindgen(method, getter, structural)]
    fn projectType(this: &ProgramConfig) -> String;
}

thread_local! { static PACKAGE_CACHE: Rc<RefCell<PackageCache>> = Rc::default(); }

/// a minimal implementation for interacting with async JS filesystem callbacks to
/// load project files
#[wasm_bindgen]
pub struct ProjectLoader(ProjectHost);

#[async_trait(?Send)]
impl JSProjectHost for ProjectHost {
    async fn read_file(&self, uri: &str) -> miette::Result<(Arc<str>, Arc<str>)> {
        let name = Arc::from(uri);

        match self.readFile(uri).await {
            Ok(val) => Ok((name, val.as_string().unwrap_or_default().into())),

            Err(js_val) => {
                let err: js_sys::Error = js_val
                    .dyn_into()
                    .expect("exception should be an error type");
                let message = err
                    .message()
                    .as_string()
                    .expect("error message should be a string");
                Err(Report::msg(message))
            }
        }
    }

    async fn list_directory(&self, uri: &str) -> Vec<JSFileEntry> {
        let js_val = self.listDirectory(uri).await;
        match js_val.dyn_into::<js_sys::Array>() {
            Ok(arr) => arr
                .into_iter()
                .map(|x| {
                    x.dyn_into::<js_sys::Array>()
                        .expect("expected directory listing callback to return array of arrays")
                })
                .filter_map(|js_arr| {
                    let mut arr = js_arr.into_iter().take(2);
                    #[allow(clippy::cast_possible_truncation)]
                    match (
                        arr.next().expect("should be string").as_string(),
                        arr.next().expect("should be float").as_f64(),
                    ) {
                        (Some(a), Some(b)) => Some((a, b as i32)),
                        _ => None,
                    }
                })
                .map(|(name, ty)| JSFileEntry {
                    name,
                    r#type: match ty {
                        0 => EntryType::Unknown,
                        1 => EntryType::File,
                        2 => EntryType::Folder,
                        64 => EntryType::Symlink,
                        _ => unreachable!("expected one of vscode.FileType. Received {ty:?}"),
                    },
                })
                .collect::<Vec<_>>(),
            Err(e) => unreachable!(
                "controlled callback should have returned an array -- our typescript bindings should guarantee this. {e:?}"
            ),
        }
    }

    async fn resolve_path(&self, base: &str, path: &str) -> miette::Result<Arc<str>> {
        match self.resolvePath(base, path).await {
            Ok(val) => Ok(val.as_string().unwrap_or_default().into()),
            Err(js_val) => {
                let err: js_sys::Error = js_val
                    .dyn_into()
                    .expect("exception should be an error type");
                let message = err
                    .message()
                    .as_string()
                    .expect("error message should be a string");
                Err(Report::msg(message))
            }
        }
    }

    async fn fetch_github(
        &self,
        owner: &str,
        repo: &str,
        r#ref: &str,
        path: &str,
    ) -> miette::Result<Arc<str>> {
        match self.fetchGithub(owner, repo, r#ref, path).await {
            Ok(js_val) => Ok(js_val
                .as_string()
                .expect("fetchGithub should return a string or throw")
                .into()),
            Err(js_val) => {
                let err: js_sys::Error = js_val
                    .dyn_into()
                    .expect("exception should be an error type");
                let message = err
                    .message()
                    .as_string()
                    .expect("error message should be a string");
                Err(Report::msg(message))
            }
        }
    }

    async fn find_manifest_directory(&self, doc_uri: &str) -> Option<Arc<str>> {
        let js_val = self.findManifestDirectory(doc_uri).await;
        js_val.as_string().map(Into::into)
    }
}

#[wasm_bindgen]
impl ProjectLoader {
    #[wasm_bindgen(constructor)]
    pub fn new(project_host: ProjectHost) -> Self {
        ProjectLoader(project_host)
    }

    pub async fn load_project_with_deps(
        &self,
        directory: String,
    ) -> Result<IProjectConfig, String> {
        let package_cache = PACKAGE_CACHE.with(Clone::clone);

        let dir_path = Path::new(&directory);
        let project_config = match self.0.load_project(dir_path, Some(&package_cache)).await {
            Ok(loaded_project) => loaded_project,
            Err(errs) => return Err(project_errors_into_qsharp_errors_json(&directory, &errs)),
        };

        // Will return error if project has errors
        project_config.try_into()
    }

    pub async fn load_openqasm_project(
        &self,
        file_path: String,
        source: Option<String>,
    ) -> Result<IProjectConfig, String> {
        let project_config = self
            .0
            .load_openqasm_project(Path::new(&file_path), source.map(Arc::<str>::from))
            .await;
        // Will return error if project has errors
        project_config.try_into()
    }
}

fn project_errors_into_qsharp_errors_json(
    project_dir: &str,
    errs: &[qsc_project::Error],
) -> String {
    serde_json::to_string(&project_errors_into_qsharp_errors(project_dir, errs))
        .expect("serializing errors to json should succeed")
}

impl From<qsc_project::PackageInfo> for PackageInfo {
    fn from(value: qsc_project::PackageInfo) -> Self {
        Self {
            sources: value
                .sources
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            language_features: value.language_features.into(),
            dependencies: value
                .dependencies
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            package_type: value.package_type.map(|x| {
                match x {
                    qsc_project::PackageType::Exe => "exe",
                    qsc_project::PackageType::Lib => "lib",
                }
                .into()
            }),
        }
    }
}

impl From<qsc_project::PackageGraphSources> for PackageGraphSources {
    fn from(value: qsc_project::PackageGraphSources) -> Self {
        Self {
            root: value.root.into(),
            packages: value
                .packages
                .into_iter()
                .map(|(pkg_key, pkg_info)| (pkg_key.to_string(), pkg_info.into()))
                .collect(),
            has_manifest: value.has_manifest,
        }
    }
}

serializable_type! {
    PackageGraphSources,
    {
        pub root: PackageInfo,
        pub packages: FxHashMap<PackageKey,PackageInfo>,
        pub has_manifest: bool,
    },
    r#"
    export type PackageKey = string;

    export interface IPackageGraphSources {
        root: IPackageInfo;
        packages: Record<PackageKey,IPackageInfo>;
        hasManifest: boolean;
    }"#,
    IPackageGraphSources
}

serializable_type! {
    PackageInfo,
    {
        pub sources: Vec<(String, String)>,
        pub language_features: Vec<String>,
        pub dependencies: FxHashMap<PackageAlias,PackageKey>,
        pub package_type: Option<String>,
    },
    r#"

    export interface IPackageInfo {
        sources: [string, string][];
        languageFeatures: string[];
        dependencies: Record<string,string>;
        packageType?: "exe" | "lib";
    }"#
}

impl TryFrom<qsc_project::Project> for IProjectConfig {
    type Error = String;

    fn try_from(value: qsc_project::Project) -> Result<Self, Self::Error> {
        if !value.errors.is_empty() {
            return Err(project_errors_into_qsharp_errors_json(
                &value.path,
                &value.errors,
            ));
        }
        let (project_type, package_graph_sources) = match value.project_type {
            qsc_project::ProjectType::QSharp(pgs) => ("qsharp".into(), pgs),
            qsc_project::ProjectType::OpenQASM(res) => (
                "openqasm".into(),
                qsc_project::PackageGraphSources {
                    root: qsc_project::PackageInfo {
                        sources: res,
                        language_features: LanguageFeatures::default(),
                        dependencies: FxHashMap::default(),
                        package_type: None,
                    },
                    packages: FxHashMap::default(),
                    has_manifest: false,
                },
            ),
        };
        let profile = value
            .target_profile
            .map(|p| p.to_str().to_string().to_lowercase());
        let project_config = ProjectConfig {
            project_name: value.name.to_string(),
            project_uri: value.path.to_string(),
            lints: value.lints,
            package_graph_sources: package_graph_sources.into(),
            project_type,
            profile,
        };
        Ok(project_config.into())
    }
}

serializable_type! {
    ProjectConfig,
    {
        pub project_name: String,
        pub project_uri: String,
        pub package_graph_sources: PackageGraphSources,
        pub lints: Vec<LintOrGroupConfig>,
        pub project_type: String,
        pub profile: Option<String>,
    },
    r#"export interface IProjectConfig {
        /**
         * Friendly name for the project, based on the name of the Q# document or project directory
         */
        projectName: string;
        /**
         * Uri for the qsharp.json or the root source file (single file projects)
         */
        projectUri: string;
        packageGraphSources: IPackageGraphSources;
        lints: ({ lint: string; level: string } | { group: string; level: string })[];
        errors: string[];
        /**
         * The type of project. This is used to determine how to load the project.
         */
        projectType: ProjectType;
        /**
         * QIR target profile for the project, as set in qsharp.json.
         */
        profile?: TargetProfile;
    }"#,
    IProjectConfig
}

type PackageAlias = String;
type PackageKey = String;

impl From<PackageGraphSources> for qsc_project::PackageGraphSources {
    fn from(value: PackageGraphSources) -> Self {
        Self {
            root: value.root.into(),
            packages: value
                .packages
                .into_iter()
                .map(|(k, v)| (Arc::from(k), v.into()))
                .collect(),
            has_manifest: value.has_manifest,
        }
    }
}

impl From<PackageInfo> for qsc_project::PackageInfo {
    fn from(value: PackageInfo) -> Self {
        Self {
            sources: value
                .sources
                .into_iter()
                .map(|(k, v)| (Arc::from(k), Arc::from(v)))
                .collect(),
            language_features: LanguageFeatures::from_iter(value.language_features),
            dependencies: value
                .dependencies
                .into_iter()
                .map(|(k, v)| (Arc::from(k), Arc::from(v)))
                .collect(),
            package_type: value.package_type.map(|x| match x.as_str() {
                "exe" => qsc_project::PackageType::Exe,
                "lib" => qsc_project::PackageType::Lib,
                _ => unreachable!(
                    "expected one of 'exe' or 'lib' -- should be guaranteed by TS types"
                ),
            }),
        }
    }
}

/// This returns the common parameters that the compiler/interpreter uses
#[allow(clippy::type_complexity)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn into_qsc_args(
    program: ProgramConfig,
    entry: Option<String>,
    ignore_dependency_errors: bool,
) -> Result<
    (
        qsc::SourceMap,
        qsc::TargetCapabilityFlags,
        qsc::LanguageFeatures,
        qsc::PackageStore,
        Vec<(qsc::hir::PackageId, Option<Arc<str>>)>,
    ),
    Vec<qsc::compile::Error>,
> {
    let pkg_graph: PackageGraphSources = program.packageGraphSources().into();
    let pkg_graph: qsc_project::PackageGraphSources = pkg_graph.into();

    // Use the project-level target_profile from ProgramConfig
    let profile = qsc::target::Profile::from_str(&program.profile())
        .unwrap_or_else(|()| panic!("Invalid target : {}", program.profile()));
    let capabilities = profile.into();

    let buildable_program = BuildableProgram::new(capabilities, pkg_graph);

    if !ignore_dependency_errors && !buildable_program.dependency_errors.is_empty() {
        return Err(buildable_program.dependency_errors);
    }

    let BuildableProgram {
        store,
        user_code,
        user_code_dependencies,
        ..
    } = buildable_program;

    let source_map = qsc::SourceMap::new(user_code.sources, entry.map(std::convert::Into::into));
    let language_features = qsc::LanguageFeatures::from_iter(user_code.language_features);

    Ok((
        source_map,
        buildable_program.capabilities,
        language_features,
        store,
        user_code_dependencies,
    ))
}

#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::type_complexity)]
pub(crate) fn into_openqasm_arg(
    program: ProgramConfig,
) -> (Vec<(Arc<str>, Arc<str>)>, TargetCapabilityFlags) {
    let profile = qsc::target::Profile::from_str(&program.profile())
        .unwrap_or_else(|()| panic!("Invalid target : {}", program.profile()));

    let pkg_graph: PackageGraphSources = program.packageGraphSources().into();
    let pkg_graph: qsc_project::PackageGraphSources = pkg_graph.into();
    let sources = pkg_graph.root.sources;

    (sources, profile.into())
}

/// Extracts the root user sources and language features from a Q# program config.
/// Used by the AST and HIR views, which compile against the cached core+std store.
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn into_qsharp_ast_args(
    program: ProgramConfig,
) -> (qsc::SourceMap, qsc::LanguageFeatures) {
    let pkg_graph: PackageGraphSources = program.packageGraphSources().into();
    let pkg_graph: qsc_project::PackageGraphSources = pkg_graph.into();
    let source_map = qsc::SourceMap::new(pkg_graph.root.sources, None);
    (source_map, pkg_graph.root.language_features)
}

pub(crate) fn is_openqasm_program(program: &ProgramConfig) -> bool {
    program.projectType() == "openqasm"
}
