use qsc_stim_parser::parser::parse;
use qsc_stim_parser::qir::compile_to_qir;
use std::fs;

fn main() {
    let stim_code =
        fs::read_to_string("examples/example.stim").expect("Failed to read examples/example.stim");

    let circuit = parse(&stim_code);
    let qir = compile_to_qir(&circuit);

    fs::write("examples/example.qir", &qir).expect("Failed to write examples/example.qir");

    println!("Wrote examples/example.qir");
}
