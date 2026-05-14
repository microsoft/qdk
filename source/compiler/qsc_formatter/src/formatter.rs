// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::span::Span;
use qsc_frontend::{
    keyword::Keyword,
    lex::{
        Delim, InterpolatedEnding, InterpolatedStart,
        concrete::{self, ConcreteToken, ConcreteTokenKind},
        cooked::{ClosedBinOp, StringToken, TokenKind},
    },
};

#[cfg(test)]
mod tests;

// Public functions

/// Applies formatting rules to the give code str and returns
/// the formatted string.
#[must_use]
pub fn format_str(code: &str) -> String {
    let mut edits = calculate_format_edits(code);
    edits.sort_by_key(|edit| edit.span.hi); // sort edits by their span's hi value from lowest to highest
    edits.reverse(); // sort from highest to lowest so that that as edits are applied they don't invalidate later applications of edits
    let mut new_code = String::from(code);

    for edit in edits {
        let range = (edit.span.lo as usize)..(edit.span.hi as usize);
        new_code.replace_range(range, &edit.new_text);
    }

    new_code
}

/// Applies formatting rules to the given code str, generating edits where
/// the source code needs to be changed to comply with the format rules.
#[must_use]
pub fn calculate_format_edits(code: &str) -> Vec<TextEdit> {
    let tokens = concrete::ConcreteTokenIterator::new(code);
    let mut edits = vec![];

    let mut formatter = Formatter {
        code,
        indent_level: 0,
        delim_newlines_stack: vec![],
        type_param_state: TypeParameterListState::NoState,
        spec_decl_state: SpecDeclState::NoState,
        import_export_state: ImportExportState::NoState,
    };

    // The sliding window used is over three adjacent tokens
    #[allow(unused_assignments)]
    let mut one = None;
    let mut two = None;
    let mut three = None;

    for token in tokens {
        // Advance the token window
        one = two;
        two = three;
        three = Some(token);

        let mut edits_for_triple = match (&one, &two, &three) {
            (Some(one), Some(two), Some(three)) => {
                if matches!(one.kind, ConcreteTokenKind::WhiteSpace) {
                    // first token is whitespace, continue scanning
                    continue;
                } else if matches!(two.kind, ConcreteTokenKind::WhiteSpace) {
                    // whitespace in the middle
                    formatter.apply_rules(one, get_token_contents(code, two), three)
                } else {
                    // one, two are adjacent tokens with no whitespace in the middle
                    formatter.apply_rules(one, "", two)
                }
            }
            (None, None, Some(three)) => {
                // Remove any whitespace at the start of a file
                if matches!(three.kind, ConcreteTokenKind::WhiteSpace) {
                    vec![TextEdit::new("", three.span.lo, three.span.hi)]
                } else {
                    vec![]
                }
            }
            _ => {
                // not enough tokens to apply a rule
                continue;
            }
        };

        edits.append(&mut edits_for_triple);
    }

    edits
}

// Public types

#[derive(Debug)]
pub struct TextEdit {
    pub new_text: String,
    pub span: Span,
}

impl TextEdit {
    fn new(new_text: &str, lo: u32, hi: u32) -> Self {
        Self {
            new_text: new_text.to_string(),
            span: Span { lo, hi },
        }
    }
}

// Private types

/// This is to keep track of whether the formatter is currently
/// processing a sequence with newline separators or single-space
/// separator.
#[derive(Clone, Copy)]
enum NewlineContext {
    /// The formatter is not in a sequence, so separators should
    /// use their default logic: newlines for `;` and single
    /// spaces for `,`.
    NoContext,
    /// In a sequence that uses newline separators.
    Newlines,
    /// In a sequence that uses single-space separators.
    Spaces,
}

/// This is to keep track of whether or not the formatter
/// is currently in a callable's type-parameter list. This
/// is necessary to disambiguate the `<` and `>` characters
/// that delimit the type-parameter list from the binary
/// comparison operators.
#[derive(Clone, Copy)]
enum TypeParameterListState {
    /// Not in a type-parameter list.
    NoState,
    /// Not in a list but have seen the callable keyword,
    /// either `function` or `operation`.
    SeenCallableKeyword,
    /// Not in a list but have seen the callable identifier.
    SeenCallableName,
    /// In the type-parameter list, or have seen the
    /// starting `<`.
    InTypeParamList,
}

/// Whether or not we are currently handling an import or export statement.
#[derive(Clone, Copy, Debug)]
enum ImportExportState {
    /// Yes, this is an import or export statement.
    HandlingImportExportStatement,
    /// No, this is not an import or export statement.
    NoState,
}

/// This is to keep track of whether or not the formatter
/// is currently processing a functor specialization
/// declaration.
#[derive(Clone, Copy)]
enum SpecDeclState {
    /// Not in a location relevant to this state.
    NoState,
    /// Formatter is on the specialization keyword.
    /// (it is the left-hand token)
    OnSpecKeyword,
    /// Formatter is on the specialization ellipse.
    /// (it is the left-hand token)
    OnEllipse,
}

/// Enum for a token's status as a delimiter.
/// `<` and `>` are delimiters only with type-parameter lists,
/// which is determined using the `TypeParameterListState` enum.
#[derive(Clone, Copy)]
enum Delimiter {
    // The token is an open delimiter. i.e. `{`, `[`, `(`, and sometimes `<`.
    Open,
    // The token is a close delimiter. i.e. `}`, `]`, `)`, and sometimes `>`.
    Close,
    // The token is not a delimiter.
    NonDelim,
}

impl Delimiter {
    /// Constructs a Delimiter from a token, given the current type-parameter state.
    fn from_concrete_token_kind(
        kind: &ConcreteTokenKind,
        type_param_state: TypeParameterListState,
        import_export_state: ImportExportState,
    ) -> Delimiter {
        match kind {
            ConcreteTokenKind::Syntax(TokenKind::Open(_)) => Delimiter::Open,
            ConcreteTokenKind::Syntax(TokenKind::Lt)
                if matches!(type_param_state, TypeParameterListState::InTypeParamList) =>
            {
                Delimiter::Open
            }
            ConcreteTokenKind::Syntax(TokenKind::Close(_)) => Delimiter::Close,
            ConcreteTokenKind::Syntax(TokenKind::Gt)
                if matches!(type_param_state, TypeParameterListState::InTypeParamList) =>
            {
                Delimiter::Close
            }
            ConcreteTokenKind::Syntax(TokenKind::Keyword(Keyword::Import | Keyword::Export)) => {
                Delimiter::Open
            }
            ConcreteTokenKind::Syntax(TokenKind::Semi)
                if matches!(
                    import_export_state,
                    ImportExportState::HandlingImportExportStatement
                ) =>
            {
                Delimiter::Close
            }
            _ => Delimiter::NonDelim,
        }
    }
}

struct Formatter<'a> {
    code: &'a str,
    indent_level: usize,
    delim_newlines_stack: Vec<NewlineContext>,
    type_param_state: TypeParameterListState,
    spec_decl_state: SpecDeclState,
    import_export_state: ImportExportState,
}

#[allow(clippy::too_many_lines)]
impl Formatter<'_> {
    fn apply_rules(
        &mut self,
        left: &ConcreteToken,
        whitespace: &str,
        right: &ConcreteToken,
    ) -> Vec<TextEdit> {
        use ConcreteTokenKind::*;
        use TokenKind::*;
        use qsc_frontend::keyword::Keyword;
        use qsc_frontend::lex::cooked::ClosedBinOp;

        let mut edits = vec![];
        // when we get here, neither left nor right should be whitespace

        let are_newlines_in_spaces = whitespace.contains('\n');
        let does_right_required_newline = matches!(right.kind, Syntax(cooked_right) if is_newline_keyword_or_ampersat(cooked_right));

        // Save the left token's status as a delimiter before updating the delimiter state
        let left_delim_state = Delimiter::from_concrete_token_kind(
            &left.kind,
            self.type_param_state,
            self.import_export_state,
        );

        self.update_spec_decl_state(&left.kind);
        self.update_type_param_state(&left.kind, &right.kind);
        self.update_import_export_state(&left.kind);

        // Save the right token's status as a delimiter after updating the delimiter state
        let right_delim_state = Delimiter::from_concrete_token_kind(
            &right.kind,
            self.type_param_state,
            self.import_export_state,
        );

        let newline_context = self.update_indent_level(
            left_delim_state,
            right_delim_state,
            are_newlines_in_spaces,
            does_right_required_newline,
            matches!(right.kind, Comment),
        );

        #[allow(clippy::match_same_arms)]
        match (left.kind, right.kind) {
            (Comment | Syntax(DocComment), _) => {
                // remove whitespace at the ends of comments
                effect_trim_comment(left, &mut edits, self.code);
                effect_correct_indentation(left, whitespace, right, &mut edits, self.indent_level);
            }
            (_, Comment | Syntax(DocComment)) if matches!(left_delim_state, Delimiter::Open) => {
                effect_correct_indentation(left, whitespace, right, &mut edits, self.indent_level);
            }
            (_, Comment | Syntax(DocComment)) if are_newlines_in_spaces => {
                effect_correct_indentation(left, whitespace, right, &mut edits, self.indent_level);
            }
            // else do nothing, preserving the user's spaces before the comment
            (Syntax(cooked_left), Syntax(cooked_right)) => match (cooked_left, cooked_right) {
                (ClosedBinOp(ClosedBinOp::Minus), _) | (_, ClosedBinOp(ClosedBinOp::Minus)) => {
                    // This case is used to ignore the spacing around a `-`.
                    // This is done because we currently don't have the architecture
                    // to be able to differentiate between the unary `-` and the binary `-`
                    // which would have different spacing rules.
                }
                (_, ClosedBinOp(ClosedBinOp::Star))
                    if matches!(
                        self.import_export_state,
                        ImportExportState::HandlingImportExportStatement
                    ) =>
                {
                    // if this is a star and we are in an import/export, then it isn't actually a
                    // binop and it's a glob import
                    effect_no_space(left, whitespace, right, &mut edits);
                }
                (Semi, _) if matches!(newline_context, NewlineContext::Spaces) => {
                    effect_single_space(left, whitespace, right, &mut edits);
                }
                (Semi, _) => {
                    effect_correct_indentation(
                        left,
                        whitespace,
                        right,
                        &mut edits,
                        self.indent_level,
                    );
                }
                (_, Semi) => {
                    effect_no_space(left, whitespace, right, &mut edits);
                }
                (Open(l), Close(r)) if l == r => {
                    // close empty delimiter blocks, i.e. (), [], {}
                    effect_no_space(left, whitespace, right, &mut edits);
                }
                (Lt, Gt)
                    if matches!(
                        self.type_param_state,
                        TypeParameterListState::InTypeParamList
                    ) =>
                {
                    // close empty delimiter blocks <>
                    effect_no_space(left, whitespace, right, &mut edits);
                }
                (_, _)
                    if matches!(left_delim_state, Delimiter::Open)
                        && matches!(newline_context, NewlineContext::Newlines) =>
                {
                    effect_correct_indentation(
                        left,
                        whitespace,
                        right,
                        &mut edits,
                        self.indent_level,
                    );
                }
                (Comma, _) if matches!(newline_context, NewlineContext::Newlines) => {
                    effect_correct_indentation(
                        left,
                        whitespace,
                        right,
                        &mut edits,
                        self.indent_level,
                    );
                }
                (Comma, _) => {
                    effect_single_space(left, whitespace, right, &mut edits);
                }
                (_, _)
                    if matches!(right_delim_state, Delimiter::Close)
                        && matches!(newline_context, NewlineContext::Newlines) =>
                {
                    effect_correct_indentation(
                        left,
                        whitespace,
                        right,
                        &mut edits,
                        self.indent_level,
                    );
                }
                (Open(Delim::Bracket | Delim::Paren), _)
                | (_, Close(Delim::Bracket | Delim::Paren)) => {
                    effect_no_space(left, whitespace, right, &mut edits);
                }
                (Lt, _) | (_, Gt)
                    if matches!(
                        self.type_param_state,
                        TypeParameterListState::InTypeParamList
                    ) =>
                {
                    effect_no_space(left, whitespace, right, &mut edits);
                }
                (Open(Delim::Brace), _) | (_, Close(Delim::Brace)) => {
                    effect_single_space(left, whitespace, right, &mut edits);
                }
                (At, Ident) => {
                    effect_no_space(left, whitespace, right, &mut edits);
                }
                (Keyword(Keyword::Internal), _) => {
                    effect_single_space(left, whitespace, right, &mut edits);
                }
                (Keyword(Keyword::Adjoint), Keyword(Keyword::Controlled))
                | (Keyword(Keyword::Controlled), Keyword(Keyword::Adjoint)) => {
                    effect_single_space(left, whitespace, right, &mut edits);
                }
                (_, _) if does_right_required_newline => {
                    effect_correct_indentation(
                        left,
                        whitespace,
                        right,
                        &mut edits,
                        self.indent_level,
                    );
                }
                (
                    _,
                    Keyword(
                        Keyword::Until
                        | Keyword::In
                        | Keyword::As
                        | Keyword::Elif
                        | Keyword::Else
                        | Keyword::Apply,
                    ),
                ) => {
                    effect_single_space(left, whitespace, right, &mut edits);
                }
                (
                    _,
                    Keyword(
                        Keyword::Auto
                        | Keyword::Distribute
                        | Keyword::Intrinsic
                        | Keyword::Invert
                        | Keyword::Slf,
                    ),
                ) => {
                    effect_single_space(left, whitespace, right, &mut edits);
                }
                (Close(Delim::Brace), _)
                    if is_newline_after_brace(cooked_right, right_delim_state) =>
                {
                    effect_correct_indentation(
                        left,
                        whitespace,
                        right,
                        &mut edits,
                        self.indent_level,
                    );
                }
                (String(StringToken::Interpolated(_, InterpolatedEnding::LBrace)), _)
                | (_, String(StringToken::Interpolated(InterpolatedStart::RBrace, _))) => {
                    effect_no_space(left, whitespace, right, &mut edits);
                }
                (DotDotDot, _) if matches!(self.spec_decl_state, SpecDeclState::OnEllipse) => {
                    // Special-case specialization declaration ellipses to have a space after
                    effect_single_space(left, whitespace, right, &mut edits);
                }
                (_, Open(Delim::Brace)) => {
                    // Special-case braces to have a leading single space with values
                    if is_prefix(cooked_left) {
                        effect_no_space(left, whitespace, right, &mut edits);
                    } else {
                        effect_single_space(left, whitespace, right, &mut edits);
                    }
                }
                (_, _) if matches!(right_delim_state, Delimiter::Open) => {
                    // Otherwise, all open delims have the same logic
                    if is_value_token_left(cooked_left, left_delim_state) || is_prefix(cooked_left)
                    {
                        // i.e. foo() or foo[3]
                        effect_no_space(left, whitespace, right, &mut edits);
                    } else {
                        // i.e. let x = (1, 2, 3);
                        effect_single_space(left, whitespace, right, &mut edits);
                    }
                }
                (_, DotDotDot) => {
                    if is_value_token_left(cooked_left, left_delim_state) {
                        effect_no_space(left, whitespace, right, &mut edits);
                    } else {
                        effect_single_space(left, whitespace, right, &mut edits);
                    }
                }
                (_, Keyword(Keyword::Is)) => {
                    effect_single_space(left, whitespace, right, &mut edits);
                }
                (_, Keyword(keyword)) if is_starter_keyword(keyword) => {
                    effect_single_space(left, whitespace, right, &mut edits);
                }
                (_, _) if is_value_token_right(cooked_right, right_delim_state) => {
                    if is_prefix(cooked_left) {
                        effect_no_space(left, whitespace, right, &mut edits);
                    } else {
                        effect_single_space(left, whitespace, right, &mut edits);
                    }
                }
                (_, _) if is_suffix(cooked_right) => {
                    effect_no_space(left, whitespace, right, &mut edits);
                }
                (_, _) if is_prefix_with_space(cooked_right) => {
                    if is_prefix(cooked_left) {
                        effect_no_space(left, whitespace, right, &mut edits);
                    } else {
                        effect_single_space(left, whitespace, right, &mut edits);
                    }
                }
                (_, _) if is_prefix_without_space(cooked_right) => {
                    effect_no_space(left, whitespace, right, &mut edits);
                }
                (_, Keyword(keyword)) if is_prefix_keyword(keyword) => {
                    effect_single_space(left, whitespace, right, &mut edits);
                }
                (_, WSlash) if are_newlines_in_spaces => {
                    effect_correct_indentation(
                        left,
                        whitespace,
                        right,
                        &mut edits,
                        self.indent_level + 1,
                    );
                }
                (_, _) if is_bin_op(cooked_right) => {
                    effect_single_space(left, whitespace, right, &mut edits);
                }
                _ => {}
            },
            _ => {}
        }
        edits
    }

    fn update_spec_decl_state(&mut self, left_kind: &ConcreteTokenKind) {
        use ConcreteTokenKind::*;
        use TokenKind::*;
        use qsc_frontend::keyword::Keyword;

        match left_kind {
            Comment => {
                // Comments don't update state
            }
            Syntax(Keyword(Keyword::Body | Keyword::Adjoint | Keyword::Controlled)) => {
                self.spec_decl_state = SpecDeclState::OnSpecKeyword;
            }
            Syntax(DotDotDot) if matches!(self.spec_decl_state, SpecDeclState::OnSpecKeyword) => {
                self.spec_decl_state = SpecDeclState::OnEllipse;
            }
            _ => {
                self.spec_decl_state = SpecDeclState::NoState;
            }
        }
    }

    fn update_import_export_state(&mut self, left_kind: &ConcreteTokenKind) {
        use ConcreteTokenKind::*;
        use TokenKind::*;
        use qsc_frontend::keyword::Keyword;

        match left_kind {
            Comment => {
                // Comments don't update state
            }
            Syntax(Keyword(Keyword::Import | Keyword::Export)) => {
                self.import_export_state = ImportExportState::HandlingImportExportStatement;
            }
            Syntax(Semi) => {
                self.import_export_state = ImportExportState::NoState;
            }
            _ => (),
        }
    }

    /// Updates the `type_param_state` of the `FormatterState` based
    /// on the left and right token kinds.
    fn update_type_param_state(
        &mut self,
        left_kind: &ConcreteTokenKind,
        right_kind: &ConcreteTokenKind,
    ) {
        use ConcreteTokenKind::*;
        use TokenKind::*;
        use qsc_frontend::{keyword::Keyword, lex::cooked::ClosedBinOp};

        // If we are leaving a type param list, reset the state
        if matches!(left_kind, Syntax(Gt))
            && matches!(
                self.type_param_state,
                TypeParameterListState::InTypeParamList
            )
        {
            self.type_param_state = TypeParameterListState::NoState;
        }

        match right_kind {
            Comment => {
                // comments don't update state
            }
            Syntax(Keyword(Keyword::Operation | Keyword::Function)) => {
                self.type_param_state = TypeParameterListState::SeenCallableKeyword;
            }
            Syntax(Ident)
                if matches!(
                    self.type_param_state,
                    TypeParameterListState::SeenCallableKeyword
                ) =>
            {
                self.type_param_state = TypeParameterListState::SeenCallableName;
            }
            Syntax(Lt)
                if matches!(
                    self.type_param_state,
                    TypeParameterListState::SeenCallableName
                ) =>
            {
                self.type_param_state = TypeParameterListState::InTypeParamList;
            }
            Syntax(AposIdent | Comma | Gt | ClosedBinOp(ClosedBinOp::Plus) | Ident | Colon)
                if matches!(
                    self.type_param_state,
                    TypeParameterListState::InTypeParamList
                ) =>
            {
                // type param identifiers and commas don't take us out of the type parameter list context
                // Gt only takes us out of the list once we are past it (it is the left-hand token)
            }
            _ => {
                self.type_param_state = TypeParameterListState::NoState;
            }
        }
    }

    /// Updates the indent level and manages the `delim_newlines_stack`
    /// of the `FormatterState`.
    /// Returns the current newline context.
    fn update_indent_level(
        &mut self,
        left_delim_state: Delimiter,
        right_delim_state: Delimiter,
        are_newlines_in_spaces: bool,
        does_right_required_newline: bool,
        is_right_comment: bool,
    ) -> NewlineContext {
        let mut newline_context = self
            .delim_newlines_stack
            .last()
            .map_or(NewlineContext::NoContext, |b| *b);

        match (left_delim_state, right_delim_state) {
            (Delimiter::Open, Delimiter::Close) => {
                // Don't change the indentation if empty sequence.
            }
            (Delimiter::Open, _) if are_newlines_in_spaces => {
                newline_context = NewlineContext::Newlines;
                self.delim_newlines_stack.push(newline_context);
                self.indent_level += 1;
            }
            (Delimiter::Open, _) if does_right_required_newline => {
                newline_context = NewlineContext::Newlines;
                self.delim_newlines_stack.push(newline_context);
                self.indent_level += 1;
            }
            (Delimiter::Open, _) if is_right_comment => {
                newline_context = NewlineContext::Newlines;
                self.delim_newlines_stack.push(newline_context);
                self.indent_level += 1;
            }
            (Delimiter::Open, _) => {
                newline_context = NewlineContext::Spaces;
                self.delim_newlines_stack.push(newline_context);
            }
            (_, Delimiter::Close) => {
                self.delim_newlines_stack.pop();
                if matches!(newline_context, NewlineContext::Newlines) {
                    self.indent_level = self.indent_level.saturating_sub(1);
                }
            }
            _ => {}
        }

        newline_context
    }
}

// Helper Functions

fn make_indent_string(level: usize) -> String {
    "    ".repeat(level)
}

fn get_token_contents<'a>(code: &'a str, token: &ConcreteToken) -> &'a str {
    &code[token.span.lo as usize..token.span.hi as usize]
}

// Rule Conditions

fn is_bin_op(cooked: TokenKind) -> bool {
    matches!(
        cooked,
        TokenKind::Bar
            | TokenKind::BinOpEq(_)
            | TokenKind::ClosedBinOp(_)
            | TokenKind::Colon
            | TokenKind::Eq
            | TokenKind::EqEq
            | TokenKind::FatArrow
            | TokenKind::Gt
            | TokenKind::Gte
            | TokenKind::LArrow
            | TokenKind::Lt
            | TokenKind::Lte
            | TokenKind::Ne
            | TokenKind::Question
            | TokenKind::RArrow
            | TokenKind::WSlash
            | TokenKind::WSlashEq
            | TokenKind::Keyword(Keyword::And | Keyword::Or)
    )
}

fn is_prefix_with_space(cooked: TokenKind) -> bool {
    matches!(cooked, TokenKind::TildeTildeTilde)
}

fn is_prefix_without_space(cooked: TokenKind) -> bool {
    matches!(
        cooked,
        TokenKind::ColonColon
            | TokenKind::Dot
            | TokenKind::DotDot
            | TokenKind::ClosedBinOp(ClosedBinOp::Caret)
    )
}

fn is_prefix(cooked: TokenKind) -> bool {
    is_prefix_with_space(cooked)
        || is_prefix_without_space(cooked)
        || matches!(cooked, TokenKind::DotDotDot)
}

fn is_suffix(cooked: TokenKind) -> bool {
    matches!(cooked, TokenKind::Bang | TokenKind::Comma)
}

fn is_prefix_keyword(keyword: Keyword) -> bool {
    use Keyword::*;
    matches!(keyword, Not | AdjointUpper | ControlledUpper)
}

fn is_keyword_value(keyword: Keyword) -> bool {
    use Keyword::*;
    matches!(
        keyword,
        True | False | Zero | One | PauliI | PauliX | PauliY | PauliZ | Underscore
        // Adj and Ctl are not really values, but have the same spacing as values
        | Adj | Ctl
    )
}

fn is_newline_keyword_or_ampersat(cooked: TokenKind) -> bool {
    use Keyword::*;
    matches!(
        cooked,
        TokenKind::At
            | TokenKind::Keyword(
                Internal
                    | Operation
                    | Function
                    | Newtype
                    | Struct
                    | Namespace
                    | Open
                    | Body
                    | Adjoint
                    | Controlled
                    | Let
                    | Mutable
                    | Set
                    | Use
                    | Borrow
                    | Fixup
                    | Import
                    | Export
            )
    )
}

fn is_starter_keyword(keyword: Keyword) -> bool {
    use Keyword::*;
    matches!(
        keyword,
        For | While | Repeat | If | Within | New | Return | Fail
    )
}

fn is_newline_after_brace(cooked: TokenKind, delim_state: Delimiter) -> bool {
    is_value_token_right(cooked, delim_state)
        || matches!(cooked, TokenKind::Keyword(keyword) if is_starter_keyword(keyword))
        || matches!(cooked, TokenKind::Keyword(keyword) if is_prefix_keyword(keyword))
}

/// Note that this does not include interpolated string literals
fn is_value_lit(cooked: TokenKind) -> bool {
    matches!(
        cooked,
        TokenKind::BigInt(_)
            | TokenKind::Float
            | TokenKind::Int(_)
            | TokenKind::String(StringToken::Normal)
    )
}

fn is_value_token_left(cooked: TokenKind, delim_state: Delimiter) -> bool {
    // a closed delim represents a value on the left
    if matches!(delim_state, Delimiter::Close) {
        return true;
    }

    match cooked {
        _ if is_value_lit(cooked) => true,
        TokenKind::Keyword(keyword) if is_keyword_value(keyword) => true,
        TokenKind::Ident
        | TokenKind::AposIdent
        | TokenKind::String(StringToken::Interpolated(_, InterpolatedEnding::Quote)) => true,
        _ => false,
    }
}

fn is_value_token_right(cooked: TokenKind, delim_state: Delimiter) -> bool {
    // an open delim represents a value on the right
    if matches!(delim_state, Delimiter::Open) {
        return true;
    }

    match cooked {
        _ if is_value_lit(cooked) => true,
        TokenKind::Keyword(keyword) if is_keyword_value(keyword) => true,
        TokenKind::Ident
        | TokenKind::AposIdent
        | TokenKind::String(StringToken::Interpolated(InterpolatedStart::DollarQuote, _)) => true,
        _ => false,
    }
}

// Rule Effects

fn effect_no_space(
    left: &ConcreteToken,
    whitespace: &str,
    right: &ConcreteToken,
    edits: &mut Vec<TextEdit>,
) {
    if !whitespace.is_empty() {
        edits.push(TextEdit::new("", left.span.hi, right.span.lo));
    }
}

fn effect_single_space(
    left: &ConcreteToken,
    whitespace: &str,
    right: &ConcreteToken,
    edits: &mut Vec<TextEdit>,
) {
    if whitespace != " " {
        edits.push(TextEdit::new(" ", left.span.hi, right.span.lo));
    }
}

fn effect_trim_comment(left: &ConcreteToken, edits: &mut Vec<TextEdit>, code: &str) {
    let comment_contents = get_token_contents(code, left);
    let new_comment_contents = comment_contents.trim_end();
    if comment_contents != new_comment_contents {
        edits.push(TextEdit::new(
            new_comment_contents,
            left.span.lo,
            left.span.hi,
        ));
    }
}

fn effect_correct_indentation(
    left: &ConcreteToken,
    whitespace: &str,
    right: &ConcreteToken,
    edits: &mut Vec<TextEdit>,
    indent_level: usize,
) {
    let mut count_newlines = whitespace.chars().filter(|c| *c == '\n').count();

    // There should always be at least one newline
    if count_newlines < 1 {
        count_newlines = 1;
    }

    let mut new_whitespace = if whitespace.contains("\r\n") {
        "\r\n".repeat(count_newlines)
    } else {
        "\n".repeat(count_newlines)
    };
    new_whitespace.push_str(&make_indent_string(indent_level));
    if whitespace != new_whitespace {
        edits.push(TextEdit::new(
            new_whitespace.as_str(),
            left.span.hi,
            right.span.lo,
        ));
    }
}
