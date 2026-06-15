// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::lex::Delim::Brace;
use crate::lex::Delim::Paren;
use crate::lex::Lexer;
use crate::lex::Token;
use crate::lex::TokenKind;
use qsc_data_structures::span::Span;
use std::{iter::Peekable, str::FromStr};

#[derive(Debug)]
pub struct Circuit {
    pub span: Span,
    pub items: Vec<Item>,
}

#[derive(Debug)]
pub enum Item {
    Line(Line),
    Block(Block),
}

#[derive(Debug)]
pub struct Line {
    pub span: Span,
    pub instruction: Instruction,
}

#[derive(Debug)]
pub struct Block {
    pub span: Span,
    pub block_instruction: Instruction, // currently, only the "REPEAT" instruction is supported
    pub items: Vec<Item>,
}

#[derive(Debug)]
pub struct Instruction {
    pub span: Span,
    pub name: String,
    pub tag: Option<String>,
    pub args: Vec<f64>,
    pub targets: Vec<Target>,
}

#[derive(Debug)]
pub struct Target {
    pub span: Span,
    pub kind: TargetKind,
}

#[derive(Debug)]
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
    Loss {
        value: u32,
    },
    Combiner,
}

#[derive(Debug)]
pub enum Pauli {
    X,
    Y,
    Z,
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

pub fn parse(input: &str) -> Circuit {
    Parser::new(input).parse()
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

    fn expect_number(&mut self) -> Token {
        let token = self.tokens.next().expect("expected number");
        if token.kind != TokenKind::Uint && token.kind != TokenKind::Double {
            panic!("expected number, got {:?}", token.kind);
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
        // Skip any leading newlines
        while self
            .tokens
            .peek()
            .is_some_and(|t| t.kind == TokenKind::Newline)
        {
            self.tokens.next();
        }

        if let TokenKind::InstructionName = self.tokens.peek()?.kind {
            // Could be the start of a block or of a line
            let instruction = self.parse_instruction();
            if let Some(token) = self.tokens.peek()
                && token.kind == TokenKind::Open(Brace)
            {
                return Some(Item::Block(self.parse_block(instruction)));
            }

            Some(Item::Line(self.parse_line(instruction)))
        } else {
            // TODO error! The start of every item should be an instruction;
            None
        }
    }

    fn parse_block(&mut self, instruction: Instruction) -> Block {
        let lo = instruction.span.lo;
        let mut items = Vec::new();
        self.expect(TokenKind::Open(Brace));
        self.expect(TokenKind::Newline);
        loop {
            if self
                .tokens
                .peek()
                .is_some_and(|t| t.kind == TokenKind::Close(Brace))
            {
                break;
            }
            match self.parse_item() {
                Some(item) => items.push(item),
                None => break,
            }
        }
        let closing_brace = self.expect(TokenKind::Close(Brace));
        self.expect(TokenKind::Newline);
        let hi = closing_brace.span.hi;
        Block {
            span: Span { lo, hi },
            block_instruction: instruction,
            items,
        }
    }

    fn parse_line(&mut self, instruction: Instruction) -> Line {
        self.expect(TokenKind::Newline);
        Line {
            span: instruction.span,
            instruction,
        }
    }

    fn parse_instruction(&mut self) -> Instruction {
        let name_token = self.expect(TokenKind::InstructionName);
        let lo = name_token.span.lo;
        let name = self.extract_string(name_token, None);

        let tag_token = self.tokens.next_if(|t| t.kind == TokenKind::Tag);
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

        if self
            .tokens
            .peek()
            .is_some_and(|t| t.kind == TokenKind::Open(Paren))
        {
            self.expect(TokenKind::Open(Paren)); // consume '('
            // Parse first arg (no leading comma)
            if self
                .tokens
                .peek()
                .is_some_and(|t| t.kind != TokenKind::Close(Paren))
            {
                let arg = self.expect_number();
                args.push(self.extract_double(arg, None));
            }
            // Each subsequent arg must be preceded by a comma
            while self
                .tokens
                .peek()
                .is_some_and(|t| t.kind != TokenKind::Close(Paren))
            {
                self.expect(TokenKind::Comma);
                let arg = self.expect_number();
                args.push(self.extract_double(arg, None));
            }
            self.expect(TokenKind::Close(Paren));
        }

        while let Some(&token) = self.tokens.peek() {
            if !self.is_target_start(&token) {
                break;
            }
            targets.push(self.parse_target());
        }

        let hi = targets
            .last()
            .map(|t| t.span.hi)
            .unwrap_or(name_token.span.hi);

        Instruction {
            span: Span { lo, hi },
            name,
            tag,
            args,
            targets,
        }
    }

    fn is_target_start(&self, token: &Token) -> bool {
        match token.kind {
            TokenKind::Uint
            | TokenKind::Rec
            | TokenKind::Sweep
            | TokenKind::Bang
            | TokenKind::Star => true,
            TokenKind::InstructionName => {
                let text = self.extract_string(*token, None);
                text.starts_with('X')
                    || text.starts_with('Y')
                    || text.starts_with('Z') // STARTS WITH PAULI
                    || text.starts_with('L') // OR LOSS
            }
            _ => false,
        }
    }

    fn parse_target(&mut self) -> Target {
        let negated_token = self.tokens.next_if(|t| t.kind == TokenKind::Bang);
        let negated = negated_token.is_some();
        let first_token = self.tokens.next().expect("target empty");
        let lo = negated_token.map_or(first_token.span.lo, |t| t.span.lo);
        let span = Span {
            lo,
            hi: first_token.span.hi,
        };

        match first_token.kind {
            TokenKind::Uint => Target {
                span,
                kind: TargetKind::Qubit {
                    negated,
                    value: self.extract_uint(first_token, None),
                },
            },
            TokenKind::InstructionName => {
                let head = self.extract_string(
                    first_token,
                    Some(Span {
                        lo: span.lo,
                        hi: span.lo + 1,
                    }),
                );
                let value = self.extract_uint(
                    first_token,
                    Some(Span {
                        lo: span.lo + 1,
                        hi: span.hi,
                    }),
                );
                if head == "L" {
                    Target {
                        span,
                        kind: TargetKind::Loss { value },
                    }
                } else {
                    Target {
                        span,
                        kind: TargetKind::Pauli {
                            negated,
                            pauli: head.parse::<Pauli>().unwrap(),
                            value,
                        },
                    }
                }
            } // Already validated
            TokenKind::Rec => Target {
                span,
                kind: TargetKind::MeasurementRecord {
                    value: self.extract_uint(
                        first_token,
                        Some(Span {
                            lo: span.lo + 5,
                            hi: span.hi - 1,
                        }),
                    ), // Strips 'rec[-' prefix and trailing ']' TODO validate it
                },
            },
            TokenKind::Sweep => Target {
                span,
                kind: TargetKind::SweepBit {
                    value: self.extract_uint(
                        first_token,
                        Some(Span {
                            lo: span.lo + 6,
                            hi: span.hi - 1,
                        }),
                    ),
                }, // Strips 'sweep[' prefix and trailing ']' TODO validate it
            },
            TokenKind::Star => Target {
                span,
                kind: TargetKind::Combiner,
            },
            _ => panic!("Unexpected target kind"),
        }
    }

    fn extract_uint(&mut self, token: Token, span: Option<Span>) -> u32 {
        self.extract_string(token, span).parse().unwrap()
    }

    fn extract_double(&mut self, token: Token, span: Option<Span>) -> f64 {
        self.extract_string(token, span).parse().unwrap()
    }

    fn extract_string(&self, token: Token, span: Option<Span>) -> String {
        if let Some(span) = span {
            self.input[span.lo as usize..span.hi as usize].to_string()
        } else {
            self.input[token.span.lo as usize..token.span.hi as usize].to_string()
        }
    }
}
