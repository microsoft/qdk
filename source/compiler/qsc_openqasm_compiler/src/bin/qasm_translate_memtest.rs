use std::{
    alloc::{GlobalAlloc, Layout, System},
    env,
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use qdk_openqasm::io::InMemorySourceResolver;
use qsc_openqasm_compiler::{
    CompilerConfig, OutputSemantics, ProgramType, QasmCompileUnit, QubitSemantics,
    compiler::parse_and_compile_to_qsharp_ast_with_config,
};

const REPETITIONS: usize = 256;
const REGISTER_WIDTH: usize = 32;
const SOURCE_GATE_CALLS: usize = REPETITIONS * 3;
const SCALAR_GATE_APPLICATIONS: usize = SOURCE_GATE_CALLS * REGISTER_WIDTH;

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

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { self.allocator.dealloc(pointer, layout) };
        self.record_dealloc(layout.size() as u64);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_pointer = unsafe { self.allocator.realloc(pointer, layout, new_size) };
        if !new_pointer.is_null() {
            let old_size = layout.size() as u64;
            let new_size = new_size as u64;
            if new_size >= old_size {
                self.record_alloc(new_size - old_size);
            } else {
                self.record_dealloc(old_size - new_size);
            }
        }
        new_pointer
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

#[derive(Clone, Copy)]
struct MemoryStats {
    peak_bytes: u64,
    net_live_bytes: u64,
    allocated_bytes: u64,
    deallocated_bytes: u64,
    allocation_count: u64,
    deallocation_count: u64,
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
    let scenario = args.next().unwrap_or_else(|| "broadcast".into());
    if scenario != "broadcast" {
        return Err(format!("unknown scenario '{scenario}'. expected broadcast"));
    }
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

    let source = broadcast_source();
    ALLOCATOR.reset();
    let mut retained_units = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        retained_units.push(translate(source.clone())?);
    }
    std::hint::black_box(&retained_units);
    let stats = ALLOCATOR.snapshot();

    println!("scenario: broadcast");
    println!("iterations: {iterations}");
    println!("broadcast_width: {REGISTER_WIDTH}");
    println!("source_gate_calls: {SOURCE_GATE_CALLS}");
    println!("scalar_gate_applications: {SCALAR_GATE_APPLICATIONS}");
    println!("peak_bytes: {}", stats.peak_bytes);
    println!("net_live_bytes: {}", stats.net_live_bytes);
    println!("allocated_bytes: {}", stats.allocated_bytes);
    println!("deallocated_bytes: {}", stats.deallocated_bytes);
    println!("allocation_count: {}", stats.allocation_count);
    println!("deallocation_count: {}", stats.deallocation_count);
    Ok(())
}

fn broadcast_source() -> Arc<str> {
    let mut source = String::from(
        "OPENQASM 3.0;\ninclude \"stdgates.inc\";\nqubit[32] left;\nqubit[32] right;\n",
    );
    for _ in 0..REPETITIONS {
        source.push_str("h left;\n");
        source.push_str("cx left, right;\n");
        source.push_str("rz(0.25) right;\n");
    }
    Arc::from(source)
}

fn translate(source: Arc<str>) -> Result<QasmCompileUnit, String> {
    let mut resolver = InMemorySourceResolver::from_iter([]);
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::OpenQasm,
        ProgramType::File,
        Some(Arc::from("BroadcastBenchmark")),
        None,
    );
    let unit = parse_and_compile_to_qsharp_ast_with_config(
        source,
        "broadcast.qasm",
        Some(&mut resolver),
        config,
    );
    if unit.has_errors() {
        return Err(format!(
            "broadcast translation produced {} errors",
            unit.errors().len()
        ));
    }
    Ok(unit)
}
