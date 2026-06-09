use crate::lex::Delim::Brace;
use crate::lex::Lexer;
use crate::lex::Token;
use crate::lex::TokenKind;
use qsc_data_structures::span::Span;
use std::{
    fmt::{self, Display, Formatter},
    iter::Peekable,
};

pub struct Circuit {
    pub span: Span,
    pub items: Vec<Item>,
}

pub enum Item {
    Line(Line),
    Block(Block),
}

pub struct Block {
    pub span: Span,
    pub block_instruction: Instruction, // currently, only the "REPEAT" instruction is supported
    pub items: Vec<Item>,
}

pub struct Line {
    pub span: Span,
    pub kind: LineKind,
}

pub enum LineKind {
    Instruction(Instruction),
    Comment(String),
}

pub struct Instruction {
    pub span: Span,
    pub name: String,
    pub tag: Option<String>,
    pub args: Vec<f64>,
    pub targets: Vec<Target>,
}

pub struct Target {
    pub span: Span,
    pub kind: TargetKind,
}

pub enum TargetKind {
    Qubit {
        negated: bool,
        value: u32,
    },
    MeasurementRecord {
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
    Combiner {
        value: bool,
    },
}

pub enum Pauli {
    X,
    Y,
    Z,
}

struct Parser<'a> {
    input: &'a str,
    tokens: Peekable<Lexer<'a>>,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            tokens: Lexer::new(input).peekable(),
        }
    }

    pub fn expect(&mut self, kind: TokenKind) -> Token {
        let token = self.tokens.next().expect("expected token");
        if token.kind != kind {
            panic!("expected token of kind {:?}", kind);
        }
        token
    }

    pub fn parse(&mut self) -> Circuit {
        let input_len = self
            .input
            .len()
            .try_into()
            .expect("input length should fit into u32");

        let mut items = Vec::new();
        while let Some(item) = self.parse_item() {
            items.push(item);
        }

        Circuit {
            span: Span {
                lo: 0,
                hi: input_len,
            },
            items,
        }
    }

    fn parse_item(&mut self) -> Option<Item> {
        // TODO do I want to throw an error in the none case here?
        let token = self.tokens.peek()?;
        if token.kind == TokenKind::InstructionName {
            let instruction = self.parse_instruction();
            //TODO use something else instead of unwrap here, I don't want it to panic
            match self.tokens.peek().unwrap().kind {
                TokenKind::Open(Brace) => {
                    return Some(Item::Block(self.parse_block(instruction)));
                }
                _ => {
                    return Some(Item::Line(self.parse_line(Some(instruction))));
                }
            }
        } else {
            let line = self.parse_line(None);
            return Some(Item::Line(line));
        }
    }

    fn parse_instruction(&mut self) -> Instruction {}

    fn parse_block(&mut self, instruction: Instruction) -> Block {
        let lo = instruction.span.lo;
        let mut items = Vec::new();
        self.expect(TokenKind::Newline);
        while self
            .tokens
            .peek()
            .is_some_and(|t| t.kind != TokenKind::Close(Brace))
        {
            let item = self.parse_item();
            if item.is_some() {
                items.push(item.unwrap());
            }
        }
        let closing_brace = self.expect(TokenKind::Close(Brace));
        let hi = closing_brace.span.hi;
        Block {
            span: Span { lo, hi },
            block_instruction: instruction,
            items,
        }
    }

    fn parse_line(&mut self, instruction: Option<Instruction>) -> Line {
        let lo: u32;
        if instruction.is_none() {
            // PROCEDURE!
        } else {
            return Line {
                span: instruction.unwrap().span,
                kind: LineKind::Instruction(instruction.unwrap()),
            };
        }
    }
}
