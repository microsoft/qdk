// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::keyword::Keyword;
use bitflags::bitflags;
use enum_iterator::all;

bitflags! {
    ///
    /// Words can be of these kinds:
    ///     - Names
    ///     - Hardcoded words:
    ///         - Keywords
    ///         - Hardcoded identifiers
    ///
    /// Names are identifiers or paths that can be resolved to a definition
    /// in the code, e.g. callable names, type names, namespaces.
    ///
    /// Keywords are known words that are not allowed as identifiers, e.g. `function`, `if`.
    ///
    /// Hardcoded identifiers are treated as identifiers by the parser, but the
    /// possible names are hardcoded into the language, e.g. "EntryPoint", "Qubit".
    ///
    /// IF UPDATING: If new values are added before the keyword range,
    ///   [`KEYWORDS_START`] *must* be updated.
    ///
    #[repr(transparent)]
    #[derive(Default, PartialEq, Debug, Clone, Copy)]
    pub struct WordKinds: u128 {

        //
        // Begin names.
        //

        /// A path in an expression. Namespaced annotations and pragmas
        /// as suggested by the QASM3 spec. Examples:
        /// `pragma qsharp.Profile.Base`
        /// `@qsharp.SimulatableIntrinsic`
        const PathExpr = 1 << 0;

        /// A path segment that follows a `.`
        /// A more specific name kind can be inferred from a recovered AST.
        const PathSegment = 1 << 1;

        //
        // End names.
        //

        //
        // Begin hardcoded identifiers.
        //

        /// An annotation, without the leading `@`.
        const Annotation = 1 << 2;


        //
        // End hardcoded identifiers.
        //

        const Durationof = 1 << 3; // `durationof` call, e.g. `durationof (scope)`

        //
        // Begin keywords.
        //

        const Barrier = keyword_bit(Keyword::Barrier);
        const Box = keyword_bit(Keyword::Box);
        const Break = keyword_bit(Keyword::Break);
        const Cal = keyword_bit(Keyword::Cal);
        const Case = keyword_bit(Keyword::Case);
        const Const = keyword_bit(Keyword::Const);
        const Continue = keyword_bit(Keyword::Continue);
        const CReg = keyword_bit(Keyword::CReg);
        const Ctrl = keyword_bit(Keyword::Ctrl);
        const Def = keyword_bit(Keyword::Def);
        const DefCal = keyword_bit(Keyword::DefCal);
        const DefCalGrammar = keyword_bit(Keyword::DefCalGrammar);
        const Default = keyword_bit(Keyword::Default);
        const Delay = keyword_bit(Keyword::Delay);
        const Else = keyword_bit(Keyword::Else);
        const End = keyword_bit(Keyword::End);
        const Extern = keyword_bit(Keyword::Extern);
        const False = keyword_bit(Keyword::False);
        const For = keyword_bit(Keyword::For);
        const Gate = keyword_bit(Keyword::Gate);
        const If = keyword_bit(Keyword::If);
        const In = keyword_bit(Keyword::In);
        const Include = keyword_bit(Keyword::Include);
        const Input = keyword_bit(Keyword::Input);
        const Inv = keyword_bit(Keyword::Inv);
        const Let = keyword_bit(Keyword::Let);
        const Measure = keyword_bit(Keyword::Measure);
        const Mutable = keyword_bit(Keyword::Mutable);
        const NegCtrl = keyword_bit(Keyword::NegCtrl);
        const OpenQASM = keyword_bit(Keyword::OpenQASM);
        const Output = keyword_bit(Keyword::Output);
        const Pow = keyword_bit(Keyword::Pow);
        const Pragma = keyword_bit(Keyword::Pragma);
        const QReg = keyword_bit(Keyword::QReg);
        const Qubit = keyword_bit(Keyword::Qubit);
        const Reset = keyword_bit(Keyword::Reset);
        const True = keyword_bit(Keyword::True);
        const ReadOnly = keyword_bit(Keyword::ReadOnly);
        const Return = keyword_bit(Keyword::Return);
        const Switch = keyword_bit(Keyword::Switch);
        const Void = keyword_bit(Keyword::Void);
        const While = keyword_bit(Keyword::While);
    }
}

const KEYWORDS_START: u8 = 4;
const fn keyword_bit(k: Keyword) -> u128 {
    1 << (k as u8 + KEYWORDS_START)
}

impl From<Keyword> for WordKinds {
    fn from(k: Keyword) -> Self {
        Self::from_bits_truncate(keyword_bit(k))
    }
}

impl WordKinds {
    /// Returns only the name kinds that this prediction set contains.
    pub fn iter_name_kinds(&self) -> impl Iterator<Item = NameKind> + '_ {
        self.iter().filter_map(|p| match p {
            WordKinds::PathExpr => Some(NameKind::Path(PathKind::Expr)),
            WordKinds::PathSegment => Some(NameKind::PathSegment),
            _ => None,
        })
    }

    /// Returns only the hardcoded identifier kinds that this prediction set contains.
    pub fn iter_hardcoded_ident_kinds(&self) -> impl Iterator<Item = HardcodedIdentKind> + '_ {
        self.iter().filter_map(|p| match p {
            WordKinds::Annotation => Some(HardcodedIdentKind::Annotation),
            _ => None,
        })
    }

    /// Returns only the keywords that this prediction set contains.
    pub fn iter_keywords(&self) -> impl Iterator<Item = Keyword> + '_ {
        all::<Keyword>().filter(|k| self.contains((*k).into()))
    }
}

/// A hardcoded identifier.
///
/// Maps to a subset of values in [`Predictions`], but an enum
/// for friendly consumption.
pub enum HardcodedIdentKind {
    /// An attribute, without the leading `@`.
    Annotation,
}

/// A name (see: [`Predictions`])
///
/// Maps to a subset of values in [`Predictions`], but an enum
/// for friendly consumption.
pub enum NameKind {
    /// A path.
    Path(PathKind),
    /// A path segment that follows a `.`
    /// A more specific name kind can only be inferred from a recovered AST.
    PathSegment,
}

/// A path (see: [`Predictions`])
///
/// Maps to a subset of values in [`Predictions`], but an enum
/// for friendly consumption.
#[derive(Debug, Clone, Copy)]
pub enum PathKind {
    /// A path in an expression. Callables, UDT constructors, local variables.
    Expr,
}
