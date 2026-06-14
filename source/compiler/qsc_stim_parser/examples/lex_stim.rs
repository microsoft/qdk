use qsc_stim_parser::lex::Lexer;
use std::fs;
use std::io::Write;

fn main() {
    let stim_code =
        fs::read_to_string("examples/example.stim").expect("Failed to read examples/example.stim");

    let mut out =
        fs::File::create("examples/lex_output.txt").expect("Failed to create output file");

    writeln!(out, "{:<20} {:<10} TEXT", "TOKEN KIND", "SPAN").unwrap();
    writeln!(out, "{:-<50}", "").unwrap();

    let lexer = Lexer::new(&stim_code);
    for token in lexer {
        let text = &stim_code[token.span.lo as usize..token.span.hi as usize];
        let text_display = match token.kind {
            qsc_stim_parser::lex::TokenKind::Newline => "\\n".to_string(),
            _ => format!("{:?}", text),
        };
        writeln!(
            out,
            "{:<20} {:<10} {}",
            token.kind.to_string(),
            format!("{}..{}", token.span.lo, token.span.hi),
            text_display
        )
        .unwrap();
    }

    println!("Wrote examples/lex_output.txt");
}
