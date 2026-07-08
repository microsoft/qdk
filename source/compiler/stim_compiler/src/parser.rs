// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::lex::{
    self,
    Delim::{Brace, Paren},
    Lexer, Token,
    TokenKind::{self},
};
use miette::Diagnostic;
use qsc_data_structures::{
    display::{
        write_field, write_list_field, writeln_field, writeln_header_with_span, writeln_list_field,
        writeln_opt_field,
    },
    span::Span,
};
use std::{fmt::Display, iter::Peekable, num::IntErrorKind, str::FromStr};
use thiserror::Error;

#[derive(Debug)]
pub struct Circuit {
    pub span: Span,
    pub items: Vec<Item>,
}

impl Display for Circuit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln_header_with_span(f, "Circuit", self.span)?;
        write_list_field(f, "items", &self.items)
    }
}

#[derive(Debug)]
pub enum Item {
    Line(Line),
    Block(Block),
}

impl Display for Item {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Item::Line(line) => write!(f, "{line}"),
            Item::Block(block) => write!(f, "{block}"),
        }
    }
}

#[derive(Debug)]
pub struct Line {
    pub instruction: Instruction,
}

impl Display for Line {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.instruction.fmt(f)
    }
}

#[derive(Debug)]
pub struct Block {
    pub span: Span,
    pub block_instruction: Instruction, // currently, only the "REPEAT" instruction is supported
    pub items: Vec<Item>,
}

impl Display for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln_header_with_span(f, "Block", self.span)?;
        writeln_field(f, "block_instruction", &self.block_instruction)?;
        write_list_field(f, "items", &self.items)
    }
}

#[derive(Debug)]
pub struct Instruction {
    pub span: Span,
    pub name: String,
    pub tag: Option<String>,
    pub args: Vec<f64>,
    pub targets: Vec<Target>,
}

impl Display for Instruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln_header_with_span(f, "Instruction", self.span)?;
        writeln_field(f, "name", &self.name)?;
        writeln_opt_field(f, "tag", self.tag.as_ref())?;
        writeln_list_field(f, "args", &self.args)?;
        write_list_field(f, "targets", &self.targets)
    }
}

#[derive(Debug)]
pub struct Target {
    pub span: Span,
    pub kind: TargetKind,
}

impl Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln_header_with_span(f, "Target", self.span)?;
        write_field(f, "kind", &self.kind)
    }
}

#[derive(Debug)]
pub enum TargetKind {
    Qubit {
        negated: bool,
        value: u32,
    },
    MeasurementRecord {
        negated: bool,
        value: u32,
    },
    SweepBit {
        value: u32,
    },
    Pauli {
        negated: bool,
        pauli: Pauli,
        value: u32,
    },
    Loss {
        value: u32,
    },
    Combiner,
}

impl Display for TargetKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TargetKind::Qubit { negated, value } => {
                if *negated {
                    write!(f, "Qubit(-{value})")
                } else {
                    write!(f, "Qubit({value})")
                }
            }
            TargetKind::MeasurementRecord { negated, value } => {
                if *negated {
                    write!(f, "MeasurementRecord(-{value})")
                } else {
                    write!(f, "MeasurementRecord({value})")
                }
            }
            TargetKind::SweepBit { value } => write!(f, "SweepBit({value})"),
            TargetKind::Pauli {
                negated,
                pauli,
                value,
            } => {
                if *negated {
                    write!(f, "Pauli(-{pauli} {value})")
                } else {
                    write!(f, "Pauli({pauli} {value})")
                }
            }
            TargetKind::Loss { value } => write!(f, "Loss({value})"),
            TargetKind::Combiner => write!(f, "Combiner"),
        }
    }
}

#[derive(Debug)]
pub enum Pauli {
    X,
    Y,
    Z,
}

impl Display for Pauli {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Pauli::X => write!(f, "X"),
            Pauli::Y => write!(f, "Y"),
            Pauli::Z => write!(f, "Z"),
        }
    }
}

impl FromStr for Pauli {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "X" => Ok(Pauli::X),
            "Y" => Ok(Pauli::Y),
            "Z" => Ok(Pauli::Z),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Lex(lex::Error),
    #[error("expected {expected}, found {found}")]
    #[diagnostic(code("Stim.Parser.ExpectedToken"))]
    ExpectedToken {
        expected: TokenKind,
        found: TokenKind,
        #[label]
        span: Span,
    },
    #[error("expected {expected}, found {found}")]
    #[diagnostic(code("Stim.Parser.Expected"))]
    Expected {
        expected: &'static str,
        found: TokenKind,
        #[label]
        span: Span,
    },
    #[error("unexpected end of input")]
    #[diagnostic(code("Stim.Parser.UnexpectedEof"))]
    UnexpectedEof {
        #[label]
        span: Span,
    },
    #[error("integer literal is too large to fit in a 32-bit unsigned integer")]
    #[diagnostic(code("Stim.Parser.IntegerTooLarge"))]
    IntegerTooLarge {
        #[label]
        span: Span,
    },
    #[error("measurement record offset cannot be zero; the most recent measurement is rec[-1]")]
    #[diagnostic(code("Stim.Parser.ZeroMeasurementRecord"))]
    ZeroMeasurementRecord {
        #[label]
        span: Span,
    },
    #[error("only qubit and Pauli targets can be negated with '!'")]
    #[diagnostic(code("Stim.Parser.CannotNegateTarget"))]
    CannotNegateTarget {
        #[label]
        span: Span,
    },
    #[error("input is too large; Stim programs must be smaller than 4 GiB")]
    #[diagnostic(code("Stim.Parser.InputTooLarge"))]
    InputTooLarge,
}

pub fn parse(input: &str) -> (Circuit, Vec<Error>) {
    let mut parser = Parser::new(input);
    let circuit = parser.parse();
    (circuit, parser.errors)
}

struct Parser<'a> {
    input: &'a str,
    input_len: u32,
    tokens: Peekable<Lexer<'a>>,
    errors: Vec<Error>,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        let mut errors = Vec::new();
        let input_len = u32::try_from(input.len()).unwrap_or_else(|_| {
            errors.push(Error::InputTooLarge);
            u32::MAX
        });
        Self {
            input,
            input_len,
            tokens: Lexer::new(input).peekable(),
            errors,
        }
    }

    fn next(&mut self) -> Option<Token> {
        loop {
            match self.tokens.next()? {
                Ok(token) => return Some(token),
                Err(error) => self.emit_error(Error::Lex(error)),
            }
        }
    }

    fn peek(&mut self) -> Option<Token> {
        loop {
            match *self.tokens.peek()? {
                Ok(token) => return Some(token),
                Err(error) => {
                    self.emit_error(Error::Lex(error));
                    self.tokens.next();
                }
            }
        }
    }

    fn next_if(&mut self, func: impl FnOnce(Token) -> bool) -> Option<Token> {
        if self.peek().is_some_and(func) {
            self.next()
        } else {
            None
        }
    }

    fn slice_input(&self, span: Span) -> &str {
        self.input
            .get(span.lo as usize..span.hi as usize)
            .unwrap_or("")
    }

    fn emit_error(&mut self, error: Error) {
        self.errors.push(error);
    }

    fn emit_eof_error(&mut self) {
        self.emit_error(Error::UnexpectedEof {
            span: Span {
                lo: self.input_len,
                hi: self.input_len,
            },
        });
    }

    fn expect_any(&mut self) -> Option<Token> {
        let token = self.next();
        if token.is_none() {
            self.emit_eof_error();
        }
        token
    }

    fn expect_token(&mut self, expected_kind: TokenKind) -> Option<Token> {
        match self.next() {
            Some(token) if token.kind == expected_kind => Some(token),
            Some(token) => {
                self.emit_error(Error::ExpectedToken {
                    expected: expected_kind,
                    found: token.kind,
                    span: token.span,
                });
                None
            }
            None => {
                self.emit_eof_error();
                None
            }
        }
    }

    fn expect_number(&mut self) -> Option<Token> {
        match self.next() {
            Some(token) if token.kind == TokenKind::Uint || token.kind == TokenKind::Double => {
                Some(token)
            }
            Some(token) => {
                self.emit_error(Error::Expected {
                    expected: "number",
                    found: token.kind,
                    span: token.span,
                });
                None
            }
            None => {
                self.emit_eof_error();
                None
            }
        }
    }

    fn expect_line_end(&mut self) -> Option<()> {
        match self.peek() {
            None => Some(()), // End of file
            Some(t) if t.kind == TokenKind::Newline => {
                self.next();
                Some(())
            }
            Some(t) => {
                self.emit_error(Error::ExpectedToken {
                    expected: TokenKind::Newline,
                    found: t.kind,
                    span: t.span,
                });
                None
            }
        }
    }

    pub fn parse(&mut self) -> Circuit {
        let mut items = Vec::new();

        while self.peek().is_some() {
            match self.parse_item() {
                Some(item) => items.push(item),
                None => self.recover_to_line_end(),
            }
        }

        Circuit {
            span: Span {
                lo: 0,
                hi: self.input_len,
            },
            items,
        }
    }

    fn recover_to_line_end(&mut self) {
        while let Some(token) = self.next() {
            if token.kind == TokenKind::Newline {
                break;
            }
        }
    }

    fn parse_item(&mut self) -> Option<Item> {
        // Skip any leading newlines
        while self.peek().is_some_and(|t| t.kind == TokenKind::Newline) {
            self.next();
        }

        let first_token = self.peek()?;
        if let TokenKind::InstructionName = first_token.kind {
            // Could be the start of a block or of a line
            let instruction = self.parse_instruction()?;
            if let Some(token) = self.peek()
                && token.kind == TokenKind::Open(Brace)
            {
                return Some(Item::Block(self.parse_block(instruction)?));
            }

            Some(Item::Line(self.parse_line(instruction)?))
        } else {
            self.emit_error(Error::ExpectedToken {
                expected: TokenKind::InstructionName,
                found: first_token.kind,
                span: first_token.span,
            });
            None
        }
    }

    fn parse_block(&mut self, instruction: Instruction) -> Option<Block> {
        let lo = instruction.span.lo;
        let mut items = Vec::new();
        self.expect_token(TokenKind::Open(Brace))?;
        self.expect_token(TokenKind::Newline)?;
        while self
            .peek()
            .is_some_and(|t| t.kind != TokenKind::Close(Brace))
        {
            match self.parse_item() {
                Some(item) => items.push(item),
                None => self.recover_to_line_end(),
            }
        }
        let closing_brace = self.expect_token(TokenKind::Close(Brace))?;
        self.expect_line_end()?;
        let hi = closing_brace.span.hi;
        Some(Block {
            span: Span { lo, hi },
            block_instruction: instruction,
            items,
        })
    }

    fn parse_line(&mut self, instruction: Instruction) -> Option<Line> {
        self.expect_line_end()?;
        Some(Line { instruction })
    }

    fn parse_instruction(&mut self) -> Option<Instruction> {
        let name_token = self.expect_token(TokenKind::InstructionName)?;
        let lo = name_token.span.lo;
        let name = self.extract_string(name_token, None);

        let tag_token = self.next_if(|t| t.kind == TokenKind::Tag);
        let tag: Option<String> = tag_token.map(|tag_token| {
            self.extract_string(
                tag_token,
                Some(Span {
                    lo: tag_token.span.lo + 1,
                    hi: tag_token.span.hi - 1,
                }),
            )
        });

        let mut args = Vec::new();
        let mut targets = Vec::new();
        let mut paren_hi = None;

        if self
            .peek()
            .is_some_and(|t| t.kind == TokenKind::Open(Paren))
        {
            self.expect_token(TokenKind::Open(Paren))?; // consume '('
            // Parse first arg (no leading comma)
            if self
                .peek()
                .is_some_and(|t| t.kind != TokenKind::Close(Paren))
            {
                let arg = self.expect_number()?;
                args.push(self.extract_double(arg, None));
            }
            // Each subsequent arg must be preceded by a comma
            while self
                .peek()
                .is_some_and(|t| t.kind != TokenKind::Close(Paren))
            {
                self.expect_token(TokenKind::Comma)?;
                let arg = self.expect_number()?;
                args.push(self.extract_double(arg, None));
            }
            paren_hi = Some(self.expect_token(TokenKind::Close(Paren))?.span.hi);
        }

        while let Some(token) = self.peek() {
            if !self.is_target_start(&token) {
                break;
            }
            targets.push(self.parse_target()?);
        }

        // The span ends at the rightmost component present: targets, else args,
        // else tag, else just the name.
        let hi = targets
            .last()
            .map(|t| t.span.hi)
            .or(paren_hi)
            .or(tag_token.map(|t| t.span.hi))
            .unwrap_or(name_token.span.hi);

        Some(Instruction {
            span: Span { lo, hi },
            name,
            tag,
            args,
            targets,
        })
    }

    fn is_target_start(&self, token: &Token) -> bool {
        match token.kind {
            TokenKind::Uint
            | TokenKind::Double // can't actually start a target, it's only here for nicer error messages
            | TokenKind::Rec
            | TokenKind::Sweep
            | TokenKind::Bang
            | TokenKind::Star => true,
            TokenKind::InstructionName => {
                let text = self.slice_input(token.span);
                text.starts_with('X')
                    || text.starts_with('Y')
                    || text.starts_with('Z')
                    || text.starts_with('L') // Starts with Pauli or Loss
            }
            _ => false,
        }
    }

    fn parse_target(&mut self) -> Option<Target> {
        let negated_token = self.next_if(|t| t.kind == TokenKind::Bang);
        let negated = negated_token.is_some();
        let first_token = self.expect_any()?;

        let kind = match first_token.kind {
            TokenKind::Uint => TargetKind::Qubit {
                negated,
                value: self.extract_uint(first_token, None)?,
            },
            TokenKind::InstructionName => self.parse_pauli_or_loss_target(first_token, negated)?,
            TokenKind::Rec => {
                // Strips 'rec[-' prefix and trailing ']'.
                let value_span = Span {
                    lo: first_token.span.lo + 5,
                    hi: first_token.span.hi - 1,
                };
                let value = self.extract_uint(first_token, Some(value_span))?;
                if value == 0 {
                    self.emit_error(Error::ZeroMeasurementRecord { span: value_span });
                    return None;
                }
                TargetKind::MeasurementRecord { negated, value }
            }
            TokenKind::Sweep => TargetKind::SweepBit {
                // Strips 'sweep[' prefix and trailing ']'.
                value: self.extract_uint(
                    first_token,
                    Some(Span {
                        lo: first_token.span.lo + 6,
                        hi: first_token.span.hi - 1,
                    }),
                )?,
            },
            TokenKind::Star => TargetKind::Combiner,
            _ => {
                self.emit_error(Error::Expected {
                    expected: "a target",
                    found: first_token.kind,
                    span: first_token.span,
                });
                return None;
            }
        };

        if let Some(bang) = negated_token
            && !matches!(
                kind,
                TargetKind::Qubit { .. }
                    | TargetKind::Pauli { .. }
                    | TargetKind::MeasurementRecord { .. }
            )
        {
            self.emit_error(Error::CannotNegateTarget { span: bang.span });
            return None;
        }

        Some(Target {
            span: Span {
                lo: negated_token.map_or(first_token.span.lo, |t| t.span.lo),
                hi: first_token.span.hi,
            },
            kind,
        })
    }

    fn parse_pauli_or_loss_target(&mut self, token: Token, negated: bool) -> Option<TargetKind> {
        let head = self.slice_input(Span {
            lo: token.span.lo,
            hi: token.span.lo + 1,
        });
        let value_span = Span {
            lo: token.span.lo + 1,
            hi: token.span.hi,
        };

        if head == "L" {
            return Some(TargetKind::Loss {
                value: self.extract_uint(token, Some(value_span))?,
            });
        }

        let Ok(pauli) = head.parse::<Pauli>() else {
            self.emit_error(Error::Expected {
                expected: "a Pauli operator (X, Y, or Z)",
                found: token.kind,
                span: token.span,
            });
            return None;
        };

        Some(TargetKind::Pauli {
            negated,
            pauli,
            value: self.extract_uint(token, Some(value_span))?,
        })
    }

    fn extract_uint(&mut self, token: Token, span: Option<Span>) -> Option<u32> {
        let span = span.unwrap_or(token.span);
        match self.slice_input(span).parse::<u32>() {
            Ok(value) => Some(value),
            Err(error) => {
                self.emit_error(match error.kind() {
                    IntErrorKind::PosOverflow => Error::IntegerTooLarge { span },
                    _ => Error::Expected {
                        expected: "an integer",
                        found: token.kind,
                        span,
                    },
                });
                None
            }
        }
    }

    fn extract_double(&self, token: Token, span: Option<Span>) -> f64 {
        self.extract_string(token, span)
            .parse::<f64>()
            .unwrap_or_else(|_| unreachable!("lexer guarantees a valid double literal"))
    }

    fn extract_string(&self, token: Token, span: Option<Span>) -> String {
        self.slice_input(span.unwrap_or(token.span)).to_string()
    }
}
