use qsc_stim_parser::parser::{Circuit, Instruction, Item, Pauli, Target, TargetKind, parse};
use std::fs;
use std::io::Write;

fn write_circuit(out: &mut impl Write, circuit: &Circuit) {
    writeln!(out, "(circuit").unwrap();
    for item in &circuit.items {
        write_item(out, item, 1);
    }
    writeln!(out, ")").unwrap();
}

fn write_item(out: &mut impl Write, item: &Item, indent: usize) {
    let pad = "  ".repeat(indent);
    match item {
        Item::Line(line) => {
            write!(out, "{pad}(").unwrap();
            write_instruction(out, &line.instruction);
            writeln!(out, ")").unwrap();
        }
        Item::Block(block) => {
            write!(out, "{pad}(").unwrap();
            write_instruction(out, &block.block_instruction);
            writeln!(out).unwrap();
            for item in &block.items {
                write_item(out, item, indent + 1);
            }
            writeln!(out, "{pad})").unwrap();
        }
    }
}

fn write_instruction(out: &mut impl Write, instr: &Instruction) {
    write!(out, "{}", instr.name).unwrap();
    if let Some(tag) = &instr.tag {
        write!(out, "[{}]", tag).unwrap();
    }
    if !instr.args.is_empty() {
        for arg in &instr.args {
            write!(out, " {}", arg).unwrap();
        }
    }
    for target in &instr.targets {
        write!(out, " ").unwrap();
        write_target(out, target);
    }
}

fn write_target(out: &mut impl Write, target: &Target) {
    match &target.kind {
        TargetKind::Qubit { negated, value } => {
            if *negated {
                write!(out, "!").unwrap();
            }
            write!(out, "{}", value).unwrap();
        }
        TargetKind::MeasurementRecord { value } => write!(out, "rec[-{}]", value).unwrap(),
        TargetKind::SweepBit { value } => write!(out, "sweep[{}]", value).unwrap(),
        TargetKind::Pauli {
            negated,
            pauli,
            value,
        } => {
            if *negated {
                write!(out, "!").unwrap();
            }
            let p = match pauli {
                Pauli::X => "X",
                Pauli::Y => "Y",
                Pauli::Z => "Z",
            };
            write!(out, "{}{}", p, value).unwrap();
        }
        TargetKind::Combiner => write!(out, "*").unwrap(),
    }
}

fn main() {
    let stim_code =
        fs::read_to_string("examples/example.stim").expect("Failed to read examples/example.stim");

    let circuit = parse(&stim_code);

    let mut out =
        fs::File::create("examples/parse_output.txt").expect("Failed to create output file");
    write_circuit(&mut out, &circuit);

    println!("Wrote examples/parse_output.txt");
}
