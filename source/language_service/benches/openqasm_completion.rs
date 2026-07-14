// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{fmt::Write as _, hint::black_box, path::Path, sync::Arc};

use async_trait::async_trait;
use criterion::{Criterion, criterion_group, criterion_main};
use qsc::line_column::{Encoding, Position};
use qsc_project::{JSFileEntry, JSProjectHost};
use qsls::LanguageService;

const URI: &str = "directive_heavy.qasm";

struct SingleFileHost {
    source: Arc<str>,
}

#[async_trait(?Send)]
impl JSProjectHost for SingleFileHost {
    async fn read_file(&self, uri: &str) -> miette::Result<(Arc<str>, Arc<str>)> {
        if uri == URI {
            Ok((Arc::from(URI), self.source.clone()))
        } else {
            Err(miette::miette!("file not found: {uri}"))
        }
    }

    async fn list_directory(&self, _uri: &str) -> Vec<JSFileEntry> {
        Vec::new()
    }

    async fn resolve_path(&self, base: &str, path: &str) -> miette::Result<Arc<str>> {
        Ok(Arc::from(
            Path::new(base).join(path).to_string_lossy().as_ref(),
        ))
    }

    async fn fetch_github(
        &self,
        _owner: &str,
        _repo: &str,
        _reference: &str,
        _path: &str,
    ) -> miette::Result<Arc<str>> {
        Err(miette::miette!(
            "GitHub references are not used by this benchmark"
        ))
    }

    async fn find_manifest_directory(&self, _uri: &str) -> Option<Arc<str>> {
        None
    }
}

fn directive_heavy_source(repetitions: usize) -> (Arc<str>, Position) {
    let mut source = String::from("OPENQASM 3.0;\n");
    for index in 0..repetitions {
        let _ = writeln!(source, "pragma vendor.mode{index} opaque/*payload*/ data");
        let _ = writeln!(source, "@vendor.note{index} //payload");
        let _ = writeln!(source, "bit flag{index};");
    }
    source.push_str("#pragma qdk.qir.profile ");
    let cursor = Position::from_utf8_byte_offset(
        Encoding::Utf8,
        &source,
        u32::try_from(source.len()).expect("benchmark source should fit into u32"),
    );
    (Arc::from(source), cursor)
}

fn directive_heavy(c: &mut Criterion) {
    let (source, cursor) = directive_heavy_source(1_024);
    let mut service = LanguageService::new(Encoding::Utf8);
    let mut update_handler = service.create_update_handler(
        |_| {},
        |_| {},
        SingleFileHost {
            source: source.clone(),
        },
    );
    service.update_document(URI, 1, &source, "openqasm");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("benchmark runtime should initialize");
    runtime.block_on(async {
        tokio::select! {
            () = update_handler.run() => panic!("update handler stopped before setup completed"),
            () = async {
                while service.get_completions(URI, cursor).items.is_empty() {
                    tokio::task::yield_now().await;
                }
            } => {}
        }
    });

    c.bench_function("directive_heavy/completion_request", |bencher| {
        bencher.iter(|| black_box(service.get_completions(black_box(URI), black_box(cursor))));
    });
}

criterion_group!(benches, directive_heavy);
criterion_main!(benches);
