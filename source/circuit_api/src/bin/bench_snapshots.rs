// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Single-threaded native snapshot measurements. The allocation window counts
//! requested Rust heap bytes for the operation and its result, excluding the
//! held input/workload and tracker bookkeeping. It is not a process RSS metric.

#[path = "bench_snapshots/allocation.rs"]
mod allocation;

use std::{collections::BTreeSet, error::Error, hint::black_box, io::Write, time::Instant};

use qdk_circuit_api::{Circuit, RegisterSpec};
use qsc_circuit_store::{
    AngleExpr, AngleOp, Clbit, ControlFlow, Expr, IfElse, PackedOperation, ParamValue, Qubit,
    StandardGate, UnitaryOperation,
};
use qsc_qhdg::{
    Authority, BorrowKind, CleanupState, Direction, Edge, EdgeKind, Epoch, Graph, GraphBuilder,
    OwnerPath, PortKind, RegionKind, ResourceId, Stage, Type, Usage, canonical_graph_snapshot,
    decode_graph_snapshot, graph_snapshot_text, verify,
};

#[global_allocator]
static ALLOCATOR: allocation::MeasuredAllocator = allocation::MeasuredAllocator;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

enum Fixture {
    Circuit(Circuit),
    Graph(Graph),
}

impl Fixture {
    fn binary(&self) -> Result<Vec<u8>> {
        match self {
            Self::Circuit(circuit) => Ok(circuit.to_bytes()?),
            Self::Graph(graph) => Ok(canonical_graph_snapshot(graph)?),
        }
    }

    fn text(&self) -> Result<String> {
        match self {
            Self::Circuit(circuit) => Ok(circuit.to_text()?),
            Self::Graph(graph) => Ok(graph_snapshot_text(graph)?),
        }
    }

    fn restore(&self, bytes: &[u8]) -> Result<Self> {
        match self {
            Self::Circuit(_) => Ok(Self::Circuit(Circuit::from_bytes(bytes)?)),
            Self::Graph(_) => Ok(Self::Graph(decode_graph_snapshot(bytes)?)),
        }
    }
}

fn append(circuit: &mut Circuit, gate: StandardGate, qubits: &[Qubit]) -> Result<()> {
    circuit.append(PackedOperation::Standard(gate), qubits, &[], None)?;
    Ok(())
}

fn alternating(gates: usize) -> Result<Circuit> {
    let mut circuit = Circuit::new(2, 0);
    for _ in 0..gates / 2 {
        append(&mut circuit, StandardGate::H, &[Qubit(0)])?;
        append(&mut circuit, StandardGate::Cx, &[Qubit(0), Qubit(1)])?;
    }
    Ok(circuit)
}

fn symbolic() -> Result<Circuit> {
    let mut circuit = Circuit::new(2, 0);
    let store = circuit.store_mut();
    let theta = store.declare_parameter("theta");
    let parameter = store.push_angle(AngleExpr::Parameter(theta));
    let scale = store.push_angle(AngleExpr::Literal(0.5));
    let angle = store.push_angle(AngleExpr::Binary(AngleOp::Mul, parameter, scale));
    for index in 0..256 {
        circuit.append(
            PackedOperation::Standard(StandardGate::Rz),
            &[Qubit(index % 2)],
            &[],
            Some(Box::new([ParamValue::Expr(angle)])),
        )?;
    }
    Ok(circuit)
}

fn structured() -> Result<Circuit> {
    let mut circuit = Circuit::from_registers(
        [
            RegisterSpec::Quantum {
                name: "qubits".into(),
                size: 2,
            },
            RegisterSpec::Classical {
                name: "readouts".into(),
                size: 1,
            },
        ],
        0,
    )?;
    circuit.store_mut().set_global_phase(-0.0);
    append(&mut circuit, StandardGate::H, &[Qubit(0)])?;
    circuit.append(
        PackedOperation::Standard(StandardGate::MeasureZ),
        &[Qubit(0)],
        &[Clbit(0)],
        None,
    )?;
    let store = circuit.store_mut();
    let condition = store.push_expr(Expr::Clbit(0));
    let then_block = store.new_block();
    let else_block = store.new_block();
    for _ in 0..128 {
        store.push_into_block(
            then_block,
            PackedOperation::Standard(StandardGate::X),
            &[Qubit(1)],
            &[],
            None,
        )?;
        store.push_into_block(
            else_block,
            PackedOperation::Standard(StandardGate::Z),
            &[Qubit(1)],
            &[],
            None,
        )?;
    }
    store.push_control_flow(ControlFlow::IfElse(IfElse {
        condition,
        then_block,
        else_block: Some(else_block),
    }))?;
    Ok(circuit)
}

fn matrix() -> Result<Circuit> {
    let mut circuit = Circuit::new(6, 0);
    let matrix = (0..4096)
        .map(|index| (if index / 64 == index % 64 { 1.0 } else { 0.0 }, 0.0))
        .collect();
    circuit.append(
        PackedOperation::Unitary(Box::new(UnitaryOperation {
            qubit_arity: 6,
            matrix,
        })),
        &(0..6).map(Qubit).collect::<Vec<_>>(),
        &[],
        None,
    )?;
    Ok(circuit)
}

fn checkpoint() -> Result<Graph> {
    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    let producer = builder.add_node(region, "quantum.allocate", vec![])?;
    let consumer = builder.add_node(region, "quantum.release", vec![])?;
    let source = builder.add_port(
        producer,
        Direction::Output,
        PortKind::Capability,
        Type::Qubit,
        Stage::Generation,
        Usage::Affine,
    )?;
    let target = builder.add_port(
        consumer,
        Direction::Input,
        PortKind::Capability,
        Type::Qubit,
        Stage::Generation,
        Usage::Affine,
    )?;
    builder.connect(Edge {
        source,
        target,
        kind: EdgeKind::AffineCapability,
    })?;
    let mut graph = builder.finish();
    let mut transaction = graph.transaction();
    for index in 0..128 {
        let node = transaction.add_node(region, format!("checkpoint.{index}"), vec![])?;
        if index % 2 == 0 {
            transaction.retire_node(node)?;
        }
    }
    transaction.commit()?;
    let path = OwnerPath::root(7);
    graph.ownership_mut().add_authority(Authority {
        path: path.clone(),
        resources: BTreeSet::from([ResourceId(7)]),
        epoch: Epoch(3),
        cleanup: CleanupState::CleanZ,
    })?;
    graph.declare_resource(source, ResourceId(7))?;
    graph.ownership_mut().borrow(&path, BorrowKind::Shared)?;
    if !verify(&graph).is_empty() {
        return Err("checkpoint fixture fails graph verification".into());
    }
    Ok(graph)
}

fn fixture(name: &str) -> Result<Fixture> {
    Ok(match name {
        "empty" => Fixture::Circuit(Circuit::new(2, 0)),
        "one_h" => {
            let mut circuit = Circuit::new(1, 0);
            append(&mut circuit, StandardGate::H, &[Qubit(0)])?;
            Fixture::Circuit(circuit)
        }
        "circuit_1000" => Fixture::Circuit(alternating(1000)?),
        "circuit_10000" => Fixture::Circuit(alternating(10000)?),
        "graph_1000" => Fixture::Graph(alternating(1000)?.to_graph()?),
        "symbolic" => Fixture::Circuit(symbolic()?),
        "structured" => Fixture::Circuit(structured()?),
        "matrix" => Fixture::Circuit(matrix()?),
        "checkpoint" => Fixture::Graph(checkpoint()?),
        _ => return Err(format!("unknown workload {name}").into()),
    })
}

const CASES: [&str; 9] = [
    "empty",
    "one_h",
    "circuit_1000",
    "circuit_10000",
    "graph_1000",
    "symbolic",
    "structured",
    "matrix",
    "checkpoint",
];

fn measure(
    output: &mut impl Write,
    case: &str,
    encoding: &str,
    direction: &str,
    encoded_bytes: usize,
    samples: usize,
    mut operation: impl FnMut() -> Result<()>,
) -> Result<()> {
    operation()?;
    for sample in 0..samples {
        let start = Instant::now();
        operation()?;
        let elapsed = start.elapsed().as_nanos();
        writeln!(
            output,
            "{case},{encoding},{direction},{sample},{elapsed},{encoded_bytes},,"
        )?;
    }
    operation()?;
    for sample in 0..samples {
        let window = allocation::Window::start()?;
        let result = operation();
        let stats = window.finish();
        result?;
        let stats = stats?;
        writeln!(
            output,
            "{case},{encoding},{direction},{sample},,{encoded_bytes},{},{}",
            stats.peak_bytes, stats.allocations
        )?;
    }
    Ok(())
}

fn measure_local_scaling(output: &mut impl Write, samples: usize) -> Result<()> {
    for count in [0, 1_000, 10_000] {
        let mut graph = checkpoint()?;
        let region = graph.regions().next().ok_or("missing region")?.id;
        let node = graph.nodes().next().ok_or("missing node")?.id;
        let matched = BTreeSet::from([graph.stable_id(node).ok_or("missing stable ID")?]);
        let expected =
            qsc_qhdg::region_digest(&graph, &qsc_qhdg::Adjacency::new(&graph), &matched)?;
        for _ in 0..count {
            graph.add_node(region, "unrelated", vec![])?;
        }
        let adjacency = qsc_qhdg::Adjacency::new(&graph);
        if qsc_qhdg::region_digest(&graph, &adjacency, &matched)? != expected {
            return Err("unrelated nodes changed the local fingerprint".into());
        }
        measure(
            output,
            &format!("match_{count}"),
            "fingerprint",
            "hash",
            expected.len(),
            samples,
            || {
                drop(black_box(qsc_qhdg::region_digest(
                    &graph, &adjacency, &matched,
                )?));
                Ok(())
            },
        )?;
    }
    Ok(())
}

fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let selected = args.next().unwrap_or_else(|| "all".into());
    let samples = args.next().map_or(Ok(9), |v| v.parse::<usize>())?;
    if samples < 9 || args.next().is_some() {
        return Err("usage: bench_snapshots [all|CASE|match-scaling] [SAMPLES>=9]".into());
    }
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    writeln!(
        output,
        "case,encoding,direction,sample,elapsed_ns,encoded_bytes,peak_additional_live_bytes,allocation_count"
    )?;
    if selected == "match-scaling" {
        return measure_local_scaling(&mut output, samples);
    }
    for name in CASES
        .into_iter()
        .filter(|name| selected == "all" || selected == *name)
    {
        let value = fixture(name)?;
        let binary = value.binary()?;
        let text = value.text()?;
        if value.restore(&binary)?.binary()? != binary
            || value.restore(text.as_bytes())?.binary()? != binary
        {
            return Err(format!("{name} does not round-trip exactly").into());
        }
        measure(
            &mut output,
            name,
            "binary",
            "encode",
            binary.len(),
            samples,
            || {
                drop(black_box(value.binary()?));
                Ok(())
            },
        )?;
        measure(
            &mut output,
            name,
            "binary",
            "decode",
            binary.len(),
            samples,
            || {
                drop(black_box(value.restore(&binary)?));
                Ok(())
            },
        )?;
        measure(
            &mut output,
            name,
            "text",
            "encode",
            text.len(),
            samples,
            || {
                drop(black_box(value.text()?));
                Ok(())
            },
        )?;
        measure(
            &mut output,
            name,
            "text",
            "decode",
            text.len(),
            samples,
            || {
                drop(black_box(value.restore(text.as_bytes())?));
                Ok(())
            },
        )?;
    }
    if selected != "all" && !CASES.contains(&selected.as_str()) {
        return Err(format!("unknown workload {selected}").into());
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("snapshot benchmark failed: {error}");
        std::process::exit(1);
    }
}
