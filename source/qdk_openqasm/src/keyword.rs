// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use enum_iterator::Sequence;
use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Sequence)]
pub enum Keyword {
    Barrier,
    Box,
    Break,
    Cal,
    Case,
    Const,
    Continue,
    CReg,
    Ctrl,
    Def,
    DefCal,
    DefCalGrammar,
    Default,
    Delay,
    Dim,
    Else,
    End,
    Extern,
    False,
    For,
    Gate,
    If,
    In,
    Include,
    Input,
    Inv,
    Let,
    Measure,
    Mutable,
    NegCtrl,
    OpenQASM,
    Output,
    Pow,
    Pragma,
    QReg,
    Qubit,
    Reset,
    True,
    ReadOnly,
    Return,
    Switch,
    Void,
    While,
}

impl Keyword {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Keyword::Barrier => "barrier",
            Keyword::Box => "box",
            Keyword::Break => "break",
            Keyword::Cal => "cal",
            Keyword::Case => "case",
            Keyword::Const => "const",
            Keyword::Continue => "continue",
            Keyword::CReg => "creg",
            Keyword::Ctrl => "ctrl",
            Keyword::Def => "def",
            Keyword::DefCal => "defcal",
            Keyword::DefCalGrammar => "defcalgrammar",
            Keyword::Default => "default",
            Keyword::Delay => "delay",
            Keyword::Dim => "#dim",
            Keyword::Else => "else",
            Keyword::End => "end",
            Keyword::Extern => "extern",
            Keyword::False => "false",
            Keyword::For => "for",
            Keyword::Gate => "gate",
            Keyword::If => "if",
            Keyword::In => "in",
            Keyword::Include => "include",
            Keyword::Input => "input",
            Keyword::Inv => "inv",
            Keyword::Let => "let",
            Keyword::Measure => "measure",
            Keyword::Mutable => "mutable",
            Keyword::NegCtrl => "negctrl",
            Keyword::OpenQASM => "OPENQASM",
            Keyword::Output => "output",
            Keyword::Pow => "pow",
            Keyword::Pragma => "pragma",
            Keyword::QReg => "qreg",
            Keyword::Qubit => "qubit",
            Keyword::Reset => "reset",
            Keyword::True => "true",
            Keyword::ReadOnly => "readonly",
            Keyword::Return => "return",
            Keyword::Switch => "switch",
            Keyword::Void => "void",
            Keyword::While => "while",
        }
    }
}

impl Display for Keyword {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Keyword {
    type Err = ();

    // This is a hot function. Use a match expression so that the Rust compiler
    // can optimize the string comparisons better, and order the cases by
    // frequency in Q# so that fewer comparisons are needed on average.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "barrier" => Ok(Self::Barrier),
            "box" => Ok(Self::Box),
            "break" => Ok(Self::Break),
            "cal" => Ok(Self::Cal),
            "case" => Ok(Self::Case),
            "const" => Ok(Self::Const),
            "continue" => Ok(Self::Continue),
            "creg" => Ok(Self::CReg),
            "ctrl" => Ok(Self::Ctrl),
            "def" => Ok(Self::Def),
            "defcal" => Ok(Self::DefCal),
            "defcalgrammar" => Ok(Self::DefCalGrammar),
            "default" => Ok(Self::Default),
            "delay" => Ok(Self::Delay),
            "dim" => Ok(Self::Dim),
            "else" => Ok(Self::Else),
            "end" => Ok(Self::End),
            "extern" => Ok(Self::Extern),
            "false" => Ok(Self::False),
            "for" => Ok(Self::For),
            "gate" => Ok(Self::Gate),
            "if" => Ok(Self::If),
            "in" => Ok(Self::In),
            "include" => Ok(Self::Include),
            "input" => Ok(Self::Input),
            "inv" => Ok(Self::Inv),
            "let" => Ok(Self::Let),
            "measure" => Ok(Self::Measure),
            "mutable" => Ok(Self::Mutable),
            "negctrl" => Ok(Self::NegCtrl),
            "OPENQASM" => Ok(Self::OpenQASM),
            "output" => Ok(Self::Output),
            "pow" => Ok(Self::Pow),
            "pragma" => Ok(Self::Pragma),
            "qreg" => Ok(Self::QReg),
            "qubit" => Ok(Self::Qubit),
            "reset" => Ok(Self::Reset),
            "true" => Ok(Self::True),
            "readonly" => Ok(Self::ReadOnly),
            "return" => Ok(Self::Return),
            "switch" => Ok(Self::Switch),
            "void" => Ok(Self::Void),
            "while" => Ok(Self::While),
            _ => Err(()),
        }
    }
}
