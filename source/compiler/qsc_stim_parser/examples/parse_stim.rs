use qsc_stim_parser::parser::{Circuit, Instruction, Item, Pauli, Target, TargetKind, parse};

fn print_circuit(circuit: &Circuit) {
    println!("(circuit");
    for item in &circuit.items {
        print_item(item, 1);
    }
    println!(")");
}

fn print_item(item: &Item, indent: usize) {
    let pad = "  ".repeat(indent);
    match item {
        Item::Line(line) => {
            print!("{pad}(");
            print_instruction(&line.instruction);
            println!(")");
        }
        Item::Block(block) => {
            print!("{pad}(");
            print_instruction(&block.block_instruction);
            println!();
            for item in &block.items {
                print_item(item, indent + 1);
            }
            println!("{pad})");
        }
    }
}

fn print_instruction(instr: &Instruction) {
    print!("{}", instr.name);
    if let Some(tag) = &instr.tag {
        print!("[{}]", tag);
    }
    if !instr.args.is_empty() {
        for arg in &instr.args {
            print!(" {}", arg);
        }
    }
    for target in &instr.targets {
        print!(" ");
        print_target(target);
    }
}

fn print_target(target: &Target) {
    match &target.kind {
        TargetKind::Qubit { negated, value } => {
            if *negated {
                print!("!");
            }
            print!("{}", value);
        }
        TargetKind::MeasurementRecord { value } => print!("rec[-{}]", value),
        TargetKind::SweepBit { value } => print!("sweep[{}]", value),
        TargetKind::Pauli {
            negated,
            pauli,
            value,
        } => {
            if *negated {
                print!("!");
            }
            let p = match pauli {
                Pauli::X => "X",
                Pauli::Y => "Y",
                Pauli::Z => "Z",
            };
            print!("{}{}", p, value);
        }
        TargetKind::Combiner => print!("*"),
    }
}

fn main() {
    let stim_code = std::fs::read_to_string("examples/example.stim")
        .expect("Failed to read examples/example.stim");

    println!("Input:\n{stim_code}");
    println!("{:=<60}", "");

    let circuit = parse(&stim_code);
    print_circuit(&circuit);
}
