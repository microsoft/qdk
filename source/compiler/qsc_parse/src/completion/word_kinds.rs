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

        /// A path in an expression. Callables, UDT constructors, local variables.
        const PathExpr = 1 << 0;
        /// A path in a type. Builtins, type params (the leading '), UDTs, including structs.
        const PathTy = 1 << 1;
        /// A path to a struct UDT.
        const PathStruct = 1 << 2;
        /// A namespace.
        const PathNamespace = 1 << 3;
        /// A path to a name that can be imported. Items (callables, UDTs) and namespaces.
        const PathImport = 1 << 4;

        /// A path segment that follows a `.`
        /// A more specific name kind can be inferred from a recovered AST.
        const PathSegment = 1 << 5;
        /// A type parameter, without the leading `'`.
        const TyParam = 1 << 6;
        /// A primitive class.
        const PrimitiveClass = 1 << 7;
        /// A field name. Can follow a `.` or `::` in a field access expression,
        /// or can be in a field assignment.
        const Field = 1 << 8;

        //
        // End names.
        //

        //
        // Begin hardcoded identifiers.
        //

        /// An attribute, without the leading `@`.
        const Attr = 1 << 9;
        /// The word `Qubit`.
        const Qubit = 1 << 10;
        /// The word `size`.
        const Size = 1 << 11;

        //
        // End hardcoded identifiers.
        //

        //
        // Begin keywords.
        //

        const Adj = keyword_bit(Keyword::Adj);
        const Adjoint = keyword_bit(Keyword::Adjoint);
        const AdjointUpper = keyword_bit(Keyword::AdjointUpper);
        const And = keyword_bit(Keyword::And);
        const Apply = keyword_bit(Keyword::Apply);
        const As = keyword_bit(Keyword::As);
        const Auto = keyword_bit(Keyword::Auto);
        const Body = keyword_bit(Keyword::Body);
        const Borrow = keyword_bit(Keyword::Borrow);
        const Controlled = keyword_bit(Keyword::Controlled);
        const ControlledUpper = keyword_bit(Keyword::ControlledUpper);
        const Ctl = keyword_bit(Keyword::Ctl);
        const Distribute = keyword_bit(Keyword::Distribute);
        const Elif = keyword_bit(Keyword::Elif);
        const Else = keyword_bit(Keyword::Else);
        const Export = keyword_bit(Keyword::Export);
        const Fail = keyword_bit(Keyword::Fail);
        const False = keyword_bit(Keyword::False);
        const Fixup = keyword_bit(Keyword::Fixup);
        const For = keyword_bit(Keyword::For);
        const Function = keyword_bit(Keyword::Function);
        const If = keyword_bit(Keyword::If);
        const Import = keyword_bit(Keyword::Import);
        const In = keyword_bit(Keyword::In);
        const Internal = keyword_bit(Keyword::Internal);
        const Intrinsic = keyword_bit(Keyword::Intrinsic);
        const Invert = keyword_bit(Keyword::Invert);
        const Is = keyword_bit(Keyword::Is);
        const Let = keyword_bit(Keyword::Let);
        const Mutable = keyword_bit(Keyword::Mutable);
        const Namespace = keyword_bit(Keyword::Namespace);
        const Newtype = keyword_bit(Keyword::Newtype);
        const New = keyword_bit(Keyword::New);
        const Not = keyword_bit(Keyword::Not);
        const One = keyword_bit(Keyword::One);
        const Open = keyword_bit(Keyword::Open);
        const Operation = keyword_bit(Keyword::Operation);
        const Or = keyword_bit(Keyword::Or);
        const Parallel = keyword_bit(Keyword::Parallel);
        const PauliI = keyword_bit(Keyword::PauliI);
        const PauliX = keyword_bit(Keyword::PauliX);
        const PauliY = keyword_bit(Keyword::PauliY);
        const PauliZ = keyword_bit(Keyword::PauliZ);
        const Repeat = keyword_bit(Keyword::Repeat);
        const Return = keyword_bit(Keyword::Return);
        const Slf = keyword_bit(Keyword::Slf);
        const Set = keyword_bit(Keyword::Set);
        const Struct = keyword_bit(Keyword::Struct);
        const True = keyword_bit(Keyword::True);
        const Underscore = keyword_bit(Keyword::Underscore);
        const Until = keyword_bit(Keyword::Until);
        const Use = keyword_bit(Keyword::Use);
        const While = keyword_bit(Keyword::While);
        const Within = keyword_bit(Keyword::Within);
        const Zero = keyword_bit(Keyword::Zero);
    }
}

const KEYWORDS_START: u8 = 12;
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
            WordKinds::PathTy => Some(NameKind::Path(PathKind::Ty)),
            WordKinds::PathStruct => Some(NameKind::Path(PathKind::Struct)),
            WordKinds::PathNamespace => Some(NameKind::Path(PathKind::Namespace)),
            WordKinds::PathImport => Some(NameKind::Path(PathKind::Import)),
            WordKinds::PathSegment => Some(NameKind::PathSegment),
            WordKinds::TyParam => Some(NameKind::TyParam),
            WordKinds::Field => Some(NameKind::Field),
            WordKinds::PrimitiveClass => Some(NameKind::PrimitiveClass),
            _ => None,
        })
    }

    /// Returns only the hardcoded identifier kinds that this prediction set contains.
    pub fn iter_hardcoded_ident_kinds(&self) -> impl Iterator<Item = HardcodedIdentKind> + '_ {
        self.iter().filter_map(|p| match p {
            WordKinds::Attr => Some(HardcodedIdentKind::Attr),
            WordKinds::Qubit => Some(HardcodedIdentKind::Qubit),
            WordKinds::Size => Some(HardcodedIdentKind::Size),
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
    Attr,
    /// The word `Qubit`.
    Qubit,
    /// The word `size`.
    Size,
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
    /// A type parameter, without the leading `'`.
    TyParam,
    /// A field name that follows a `.` or `::` in a field access expression.
    Field,
    /// A primitive class, like Eq, Exp, or Add.
    PrimitiveClass,
}

/// A path (see: [`Predictions`])
///
/// Maps to a subset of values in [`Predictions`], but an enum
/// for friendly consumption.
#[derive(Debug, Clone, Copy)]
pub enum PathKind {
    /// A path in an expression. Callables, UDT constructors, local variables.
    Expr,
    /// A path in a type. Builtins, type params (the leading '), UDTs, including structs.
    Ty,
    /// A path to a struct UDT.
    Struct,
    /// A namespace.
    Namespace,
    /// A path to a name that can be imported. Items (callables, UDTs) and namespaces.
    Import,
}
