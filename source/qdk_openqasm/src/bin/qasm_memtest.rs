// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    env,
    process::ExitCode,
    sync::atomic::{AtomicU64, Ordering},
};

use qdk_openqasm::{analyze_source, parse_source, semantic::lower_parse_result};

#[path = "../../benches/corpus.rs"]
#[allow(dead_code)]
mod corpus;

use corpus::{Corpus, ExactSize, broadcast_gate, exact_size, flat_gate, include_heavy};

struct AllocationCounter<A: GlobalAlloc> {
    allocator: A,
    current_bytes: AtomicU64,
    peak_bytes: AtomicU64,
    allocated_bytes: AtomicU64,
    deallocated_bytes: AtomicU64,
    allocation_count: AtomicU64,
    deallocation_count: AtomicU64,
}

unsafe impl<A: GlobalAlloc> GlobalAlloc for AllocationCounter<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { self.allocator.alloc(layout) };
        if !pointer.is_null() {
            self.record_alloc(layout.size() as u64);
        }
        pointer
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { self.allocator.dealloc(ptr, layout) };
        self.record_dealloc(layout.size() as u64);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let pointer = unsafe { self.allocator.realloc(ptr, layout, new_size) };
        if !pointer.is_null() {
            let old_size = layout.size() as u64;
            let new_size = new_size as u64;
            if new_size >= old_size {
                self.record_alloc(new_size - old_size);
            } else {
                self.record_dealloc(old_size - new_size);
            }
        }
        pointer
    }
}

impl<A: GlobalAlloc> AllocationCounter<A> {
    const fn new(allocator: A) -> Self {
        Self {
            allocator,
            current_bytes: AtomicU64::new(0),
            peak_bytes: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
            deallocated_bytes: AtomicU64::new(0),
            allocation_count: AtomicU64::new(0),
            deallocation_count: AtomicU64::new(0),
        }
    }

    fn reset(&self) {
        self.current_bytes.store(0, Ordering::SeqCst);
        self.peak_bytes.store(0, Ordering::SeqCst);
        self.allocated_bytes.store(0, Ordering::SeqCst);
        self.deallocated_bytes.store(0, Ordering::SeqCst);
        self.allocation_count.store(0, Ordering::SeqCst);
        self.deallocation_count.store(0, Ordering::SeqCst);
    }

    fn snapshot(&self) -> MemoryStats {
        MemoryStats {
            peak_bytes: self.peak_bytes.load(Ordering::SeqCst),
            net_live_bytes: self.current_bytes.load(Ordering::SeqCst),
            allocated_bytes: self.allocated_bytes.load(Ordering::SeqCst),
            deallocated_bytes: self.deallocated_bytes.load(Ordering::SeqCst),
            allocation_count: self.allocation_count.load(Ordering::SeqCst),
            deallocation_count: self.deallocation_count.load(Ordering::SeqCst),
        }
    }

    fn record_alloc(&self, bytes: u64) {
        let current = self.current_bytes.fetch_add(bytes, Ordering::SeqCst) + bytes;
        self.allocated_bytes.fetch_add(bytes, Ordering::SeqCst);
        self.allocation_count.fetch_add(1, Ordering::SeqCst);
        let mut peak = self.peak_bytes.load(Ordering::SeqCst);
        while current > peak {
            match self.peak_bytes.compare_exchange(
                peak,
                current,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(new_peak) => peak = new_peak,
            }
        }
    }

    fn record_dealloc(&self, bytes: u64) {
        self.current_bytes.fetch_sub(bytes, Ordering::SeqCst);
        self.deallocated_bytes.fetch_add(bytes, Ordering::SeqCst);
        self.deallocation_count.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Clone, Copy, Debug)]
struct MemoryStats {
    peak_bytes: u64,
    net_live_bytes: u64,
    allocated_bytes: u64,
    deallocated_bytes: u64,
    allocation_count: u64,
    deallocation_count: u64,
}

#[derive(Clone, Copy)]
enum Stage {
    Parse,
    ParseExact,
    SemanticLower,
    Analyze,
    AnalyzeExact,
    SemanticLowerBroadcast,
    AnalyzeBroadcast,
    AnalyzeBroadcastRetained,
    ParseInclude,
    SemanticLowerInclude,
    AnalyzeInclude,
}

#[global_allocator]
static ALLOCATOR: AllocationCounter<System> = AllocationCounter::new(System);

fn main() -> ExitCode {
    match try_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn try_main() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let stage = args
        .next()
        .as_deref()
        .map(parse_stage)
        .transpose()?
        .unwrap_or(Stage::Analyze);
    let iterations = args
        .next()
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|error| format!("invalid iteration count '{arg}': {error}"))
        })
        .transpose()?
        .unwrap_or(1);

    if iterations == 0 {
        return Err("iteration count must be greater than zero".into());
    }

    let exact_size = if matches!(stage, Stage::ParseExact | Stage::AnalyzeExact) {
        let label = args
            .next()
            .ok_or_else(|| "exact stages require a size label".to_string())?;
        Some(parse_exact_size(&label)?)
    } else {
        None
    };
    let corpus = stage.corpus(exact_size)?;

    ALLOCATOR.reset();
    let mut retained_parse_results = Vec::new();
    let mut retained_analysis_results = Vec::new();
    for _ in 0..iterations {
        match stage {
            Stage::ParseExact => retained_parse_results.push(parse(&corpus)?),
            Stage::AnalyzeExact | Stage::AnalyzeBroadcastRetained => {
                retained_analysis_results.push(analyze(&corpus)?);
            }
            _ => run_stage(stage, &corpus)?,
        }
    }
    std::hint::black_box(&retained_parse_results);
    std::hint::black_box(&retained_analysis_results);
    let stats = ALLOCATOR.snapshot();
    print_stats(stage.name(), &corpus, iterations, stats);
    Ok(())
}

fn parse_stage(stage: &str) -> Result<Stage, String> {
    match stage {
        "parse" => Ok(Stage::Parse),
        "parse-exact" => Ok(Stage::ParseExact),
        "semantic-lower" => Ok(Stage::SemanticLower),
        "analyze" => Ok(Stage::Analyze),
        "analyze-exact" => Ok(Stage::AnalyzeExact),
        "semantic-lower-broadcast" => Ok(Stage::SemanticLowerBroadcast),
        "analyze-broadcast" => Ok(Stage::AnalyzeBroadcast),
        "analyze-broadcast-retained" => Ok(Stage::AnalyzeBroadcastRetained),
        "parse-include" => Ok(Stage::ParseInclude),
        "semantic-lower-include" => Ok(Stage::SemanticLowerInclude),
        "analyze-include" => Ok(Stage::AnalyzeInclude),
        _ => Err(format!(
            "unknown stage '{stage}'. expected parse, parse-exact, semantic-lower, analyze, analyze-exact, semantic-lower-broadcast, analyze-broadcast, analyze-broadcast-retained, parse-include, semantic-lower-include, or analyze-include"
        )),
    }
}

fn parse_exact_size(label: &str) -> Result<ExactSize, String> {
    ExactSize::ALL
        .into_iter()
        .find(|size| size.label() == label)
        .ok_or_else(|| {
            format!("unknown exact size '{label}'. expected 10KiB, 100KiB, 1MiB, 5MiB, or 10MiB")
        })
}

impl Stage {
    const fn name(self) -> &'static str {
        match self {
            Self::Parse => "parse",
            Self::ParseExact => "parse-exact",
            Self::SemanticLower => "semantic-lower",
            Self::Analyze => "analyze",
            Self::AnalyzeExact => "analyze-exact",
            Self::SemanticLowerBroadcast => "semantic-lower-broadcast",
            Self::AnalyzeBroadcast => "analyze-broadcast",
            Self::AnalyzeBroadcastRetained => "analyze-broadcast-retained",
            Self::ParseInclude => "parse-include",
            Self::SemanticLowerInclude => "semantic-lower-include",
            Self::AnalyzeInclude => "analyze-include",
        }
    }

    fn corpus(self, exact_size_label: Option<ExactSize>) -> Result<Corpus, String> {
        let corpus = match self {
            Self::ParseExact | Self::AnalyzeExact => exact_size(
                exact_size_label.ok_or_else(|| "exact stages require a size label".to_string())?,
            ),
            Self::SemanticLowerBroadcast
            | Self::AnalyzeBroadcast
            | Self::AnalyzeBroadcastRetained => broadcast_gate(256, 32),
            Self::ParseInclude | Self::SemanticLowerInclude | Self::AnalyzeInclude => {
                include_heavy(64, 8)
            }
            Self::Parse | Self::SemanticLower | Self::Analyze => flat_gate(1_024),
        };
        Ok(corpus)
    }
}

fn run_stage(stage: Stage, corpus: &Corpus) -> Result<(), String> {
    match stage {
        Stage::SemanticLower | Stage::SemanticLowerBroadcast | Stage::SemanticLowerInclude => {
            let parse_result = parse(corpus)?;
            let result = lower_parse_result(parse_result);
            ensure_semantic_success(corpus, &result)?;
            std::hint::black_box(result);
        }
        Stage::Analyze
        | Stage::AnalyzeExact
        | Stage::AnalyzeBroadcast
        | Stage::AnalyzeBroadcastRetained
        | Stage::AnalyzeInclude => {
            analyze(corpus)?;
        }
        Stage::Parse | Stage::ParseExact | Stage::ParseInclude => {
            parse(corpus)?;
        }
    }
    Ok(())
}

fn parse(corpus: &Corpus) -> Result<qdk_openqasm::parser::ParseResult, String> {
    let mut resolver = corpus.resolver();
    let result = parse_source(
        corpus.source.clone(),
        corpus.path.clone(),
        Some(&mut resolver),
    );
    ensure_parse_success(corpus, &result)?;
    Ok(std::hint::black_box(result))
}

fn analyze(corpus: &Corpus) -> Result<qdk_openqasm::semantic::AnalysisResult, String> {
    let mut resolver = corpus.resolver();
    let result = analyze_source(
        corpus.source.clone(),
        corpus.path.clone(),
        Some(&mut resolver),
    );
    ensure_semantic_success(corpus, &result)?;
    Ok(std::hint::black_box(result))
}

fn ensure_parse_success(
    corpus: &Corpus,
    result: &qdk_openqasm::parser::ParseResult,
) -> Result<(), String> {
    if result.has_errors() {
        return Err(format!(
            "{} parse corpus produced {} errors",
            corpus.name,
            result.all_errors().len()
        ));
    }
    Ok(())
}

fn ensure_semantic_success(
    corpus: &Corpus,
    result: &qdk_openqasm::semantic::AnalysisResult,
) -> Result<(), String> {
    if result.has_errors() {
        return Err(format!(
            "{} semantic corpus produced {} errors",
            corpus.name,
            result.all_errors().len()
        ));
    }
    Ok(())
}

fn print_stats(stage: &str, corpus: &Corpus, iterations: usize, stats: MemoryStats) {
    println!("stage: {stage}");
    println!("corpus: {}", corpus.name);
    println!("source_bytes: {}", corpus.source_bytes());
    println!("statements: {}", corpus.statement_count);
    println!("iterations: {iterations}");
    println!("peak_bytes: {}", stats.peak_bytes);
    println!("net_live_bytes: {}", stats.net_live_bytes);
    println!("allocated_bytes: {}", stats.allocated_bytes);
    println!("deallocated_bytes: {}", stats.deallocated_bytes);
    println!("allocation_count: {}", stats.allocation_count);
    println!("deallocation_count: {}", stats.deallocation_count);
    if matches!(
        stage,
        "semantic-lower-broadcast" | "analyze-broadcast" | "analyze-broadcast-retained"
    ) {
        println!("broadcast_width: 32");
        println!("source_gate_calls: 768");
        println!("scalar_gate_applications: 24576");
    }
}
