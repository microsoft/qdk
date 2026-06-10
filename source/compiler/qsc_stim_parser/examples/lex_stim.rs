use qsc_stim_parser::lex::Lexer;
use std::fs;

fn main() {
    let stim_code =
        fs::read_to_string("examples/example.stim").expect("Failed to read examples/example.stim");

    println!("Input:\n{stim_code}");
    println!("{:-<50}", "");
    println!("{:<20} {:<10} {:}", "TOKEN KIND", "SPAN", "TEXT");
    println!("{:-<50}", "");

    let lexer = Lexer::new(&stim_code);
    for token in lexer {
        let text = &stim_code[token.span.lo as usize..token.span.hi as usize];
        let text_display = match token.kind {
            qsc_stim_parser::lex::TokenKind::Newline => "\\n".to_string(),
            _ => format!("{:?}", text),
        };
        println!(
            "{:<20} {:<10} {}",
            token.kind.to_string(),
            format!("{}..{}", token.span.lo, token.span.hi),
            text_display
        );
    }
}
