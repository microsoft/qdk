// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

allocator::assign_global!();

use clap::{crate_version, ArgGroup, Parser, ValueEnum};
use log::info;
use miette::{Context, IntoDiagnostic, Report};
use qsc::hir::PackageId;
use qsc::{compile::compile, PassContext};
use qsc_codegen::qir::fir_to_qir;
use qsc_data_structures::{language_features::LanguageFeatures, target::TargetCapabilityFlags};
use qsc_frontend::{
    compile::{PackageStore, SourceContents, SourceMap, SourceName},
    error::WithSource,
};
use qsc_hir::hir::Package;
use qsc_partial_eval::ProgramEntry;
use qsc_passes::PackageType;
use qsc_project::{FileSystem, Manifest, StdFs};
use std::{
    concat, fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::ExitCode,
    string::String,
};

#[derive(clap::ValueEnum, Clone, Debug, Default, PartialEq)]
pub enum Profile {
    /// This is the default profile, which allows all operations.
    #[default]
    Unrestricted,
    /// This profile restricts the set of operations to those that are supported by the Base profile.
    Base,
    /// This profile restricts the set of operations to those that are supported by the AdaptiveRI profile.
    AdaptiveRI,
}

// convert Profile into qsc::target::Profile
impl From<Profile> for qsc::target::Profile {
    fn from(profile: Profile) -> Self {
        match profile {
            Profile::Unrestricted => qsc::target::Profile::Unrestricted,
            Profile::Base => qsc::target::Profile::Base,
            Profile::AdaptiveRI => qsc::target::Profile::AdaptiveRI,
        }
    }
}

#[derive(Debug, Parser)]
#[command(version = concat!(crate_version!(), " (", env!("QSHARP_GIT_HASH"), ")"), arg_required_else_help(false))]
#[clap(group(ArgGroup::new("input").args(["entry", "sources"]).required(false).multiple(true)))]
struct Cli {
    /// Disable automatic inclusion of the standard library.
    #[arg(long)]
    nostdlib: bool,

    /// Emit the compilation unit in the specified format.
    #[arg(long, value_enum)]
    emit: Vec<Emit>,

    /// Write output to compiler-chosen filename in <dir>.
    #[arg(long = "outdir", value_name = "DIR")]
    out_dir: Option<PathBuf>,

    /// Enable verbose output.
    #[arg(short, long)]
    verbose: bool,

    /// Entry expression to execute as the main operation.
    #[arg(short, long)]
    entry: Option<String>,

    /// Target QIR profile for code generation
    #[arg(short, long)]
    profile: Option<Profile>,

    /// Q# source files to compile, or `-` to read from stdin.
    #[arg()]
    sources: Vec<PathBuf>,

    /// Path to a Q# manifest for a project
    #[arg(short, long)]
    qsharp_json: Option<PathBuf>,

    /// Language features to compile with
    #[arg(short, long)]
    features: Vec<String>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum Emit {
    Hir,
    Qir,
}

fn main() -> miette::Result<ExitCode> {
    env_logger::init();
    let cli = Cli::parse();
    let mut store = PackageStore::new(qsc::compile::core());
    let mut dependencies = Vec::new();
    let profile: qsc::target::Profile = cli.profile.unwrap_or_default().into();
    let capabilities = profile.into();
    let package_type = if cli.emit.contains(&Emit::Qir) {
        PackageType::Exe
    } else {
        PackageType::Lib
    };

    if !cli.nostdlib {
        dependencies.push(store.insert(qsc::compile::std(&store, capabilities)));
    }

    let mut features = LanguageFeatures::from_iter(cli.features);

    let mut sources = cli
        .sources
        .iter()
        .map(read_source)
        .collect::<miette::Result<Vec<_>>>()?;

    if sources.is_empty() {
        let fs = StdFs;
        let manifest = Manifest::load(cli.qsharp_json)?;
        if let Some(manifest) = manifest {
            let project = fs.load_project(&manifest)?;
            let mut project_sources = project.sources;

            sources.append(&mut project_sources);

            features.merge(LanguageFeatures::from_iter(
                manifest.manifest.language_features,
            ));
        }
    }

    let entry = cli.entry.unwrap_or_default();
    let sources = SourceMap::new(sources, Some(entry.into()));
    let (unit, errors) = compile(
        &store,
        &dependencies,
        sources,
        package_type,
        capabilities,
        features,
    );
    let package_id = store.insert(unit);
    let unit = store.get(package_id).expect("package should be in store");

    let out_dir = cli.out_dir.as_ref().map_or(".".as_ref(), PathBuf::as_path);
    for emit in &cli.emit {
        match emit {
            Emit::Hir => emit_hir(&unit.package, out_dir)?,
            Emit::Qir => {
                if package_type != PackageType::Exe {
                    eprintln!("QIR generation is only supported for executable packages");
                    return Ok(ExitCode::FAILURE);
                }
                if capabilities == TargetCapabilityFlags::all() {
                    eprintln!("QIR generation is not supported for unrestricted profile");
                    return Ok(ExitCode::FAILURE);
                }
                if errors.is_empty() {
                    if let Err(reports) = emit_qir(out_dir, &store, package_id, capabilities) {
                        for report in reports {
                            eprintln!("{report:?}");
                        }
                        return Ok(ExitCode::FAILURE);
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(ExitCode::SUCCESS)
    } else {
        for error in errors {
            eprintln!("{:?}", Report::new(error));
        }

        Ok(ExitCode::FAILURE)
    }
}

fn read_source(path: impl AsRef<Path>) -> miette::Result<(SourceName, SourceContents)> {
    let path = path.as_ref();
    if path.as_os_str() == "-" {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .into_diagnostic()
            .context("could not read standard input")?;

        Ok(("<stdin>".into(), input.into()))
    } else {
        let contents = fs::read_to_string(path)
            .into_diagnostic()
            .with_context(|| format!("could not read source file `{}`", path.display()))?;

        Ok((path.to_string_lossy().into(), contents.into()))
    }
}

fn emit_hir(package: &Package, dir: impl AsRef<Path>) -> miette::Result<()> {
    let path = dir.as_ref().join("hir.txt");
    info!(
        "Writing HIR output file to: {}",
        path.to_str().unwrap_or_default()
    );
    fs::write(&path, package.to_string())
        .into_diagnostic()
        .with_context(|| format!("could not emit HIR file `{}`", path.display()))
}

fn emit_qir(
    out_dir: &Path,
    store: &PackageStore,
    package_id: PackageId,
    capabilities: TargetCapabilityFlags,
) -> Result<(), Vec<Report>> {
    let (fir_store, fir_package_id) = qsc_passes::lower_hir_to_fir(store, package_id);
    let package = fir_store.get(fir_package_id);
    let entry = ProgramEntry {
        exec_graph: package.entry_exec_graph.clone(),
        expr: (
            fir_package_id,
            package
                .entry
                .expect("package must have an entry expression"),
        )
            .into(),
    };

    let results = PassContext::run_fir_passes_on_fir(&fir_store, fir_package_id, capabilities);
    if results.is_err() {
        let errors = results.expect_err("should have errors");
        let errors = errors.into_iter().map(Report::new).collect();
        return Err(errors);
    }
    let compute_properties = results.expect("should have compute properties");

    match fir_to_qir(&fir_store, capabilities, Some(compute_properties), &entry) {
        Ok(qir) => {
            let path = out_dir.join("qir.ll");
            info!(
                "Writing QIR output file to: {}",
                path.to_str().unwrap_or_default()
            );
            fs::write(&path, qir)
                .into_diagnostic()
                .with_context(|| format!("could not emit QIR file `{}`", path.display()))
                .map_err(|err| vec![err])
        }
        Err(error) => {
            let source_package = match error.span() {
                Some(span) => span.package,
                None => package_id,
            };
            let unit = store
                .get(source_package)
                .expect("package should be in store");
            Err(vec![Report::new(WithSource::from_map(
                &unit.sources,
                error,
            ))])
        }
    }
}
