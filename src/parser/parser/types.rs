//! Type hints, type parameters, contexts and constraints.

use smallvec::SmallVec;

use super::repair::ListKind;
use super::{Parser, R};
use crate::ast::*;
use crate::checks::{Feature, ParamModifier, param_modifier_rank};
use crate::cst::SyntaxKind as K;
use crate::span::Span;
use crate::token::T;

const TYPE_EXPECTED: &str = "A type specifier is expected here.";
const PAREN_OR_COMMA_EXPECTED: &str = "A right parenthesis `)` or comma `,` is expected here.";
const MATCHING_SEPARATOR_EXPECTED: &str = "Expected matching separator here.";

/// What HHVM's translation to the AST says of a type that is not there.
const MISSING_TYPE_HINT: &str = "Encountered unexpected text ``, was expecting a type hint.";

/// What the checks on the parameters of a function type need to know about
/// one of them that the tree does not keep.
struct WrittenParam {
    start: u32,
    end: u32,
    /// Where its name starts, if it has one.
    name_start: Option<u32>,
    /// Where its type should have been, if it has none.
    untyped_at: Option<u32>,
    /// HHVM's tree has something in the parameter's place for `named`.
    named_slot: bool,
}

/// Whether HHVM reads `s` as an integer (it rejects such shape keys): a
/// port of `int_of_string_opt` in HHVM's `ocaml_helper`.
fn looks_like_int(s: &[u8]) -> bool {
    let (negative, s) = match s.first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let (base, digits) = match s {
        [b'0', b'x' | b'X', rest @ ..] => (16, rest),
        [b'0', b'o' | b'O', rest @ ..] => (8, rest),
        [b'0', b'b' | b'B', rest @ ..] => (2, rest),
        _ => (10, s),
    };
    if digits.is_empty() {
        return false;
    }
    let mut value = 0i64;
    for (i, &c) in digits.iter().enumerate() {
        if i != 0 && c == b'_' {
            continue;
        }
        let digit = match c {
            b'0'..=b'9' => c - b'0',
            b'A'..=b'F' => c - b'A' + 10,
            b'a'..=b'f' => c - b'a' + 10,
            _ => return false,
        };
        if digit >= base {
            return false;
        }
        // A number that does not fit is none.
        let next = value.checked_mul(base as i64).and_then(|scaled| {
            if negative {
                scaled.checked_sub(digit as i64)
            } else {
                scaled.checked_add(digit as i64)
            }
        });
        match next {
            Some(next) => value = next,
            None => return false,
        }
    }
    true
}

impl<'a> Parser<'a> {
    /// Consumes a single `>`, splitting `>>`, `>=` and `>>=` when needed
    /// (`vec<vec<int>>`).
    pub(crate) fn expect_gt(&mut self) -> R<()> {
        match self.tok.kind {
            T::Gt => {
                self.bump();
                Ok(())
            }
            T::GtGt | T::GtEq | T::GtGtEq => {
                let start = self.tok.start as usize;
                self.retokenize(T::Gt, start + 1);
                self.bump();
                Ok(())
            }
            _ => {
                // In a type HHVM reads `>>` as two tokens, so one of those
                // after the current token makes the current one unexpected.
                if matches!(self.peek_kind(), T::Gt | T::GtGt | T::GtEq | T::GtGtEq) {
                    self.skip_unexpected_token();
                    return self.expect_gt();
                }
                self.report_missing("A closing angle bracket `>` is expected here.");
                self.c_missing();
                Ok(())
            }
        }
    }

    /// Whether a `<` opening a type list is next. `<<<` is lexed as the
    /// start of a heredoc, but in a type list it is `<` followed by an
    /// attribute (`Foo<<<__Soft>> T>`), so it gets split here.
    pub(crate) fn at_type_lt(&mut self) -> bool {
        match self.tok.kind {
            T::Lt => true,
            T::Heredoc | T::Nowdoc => {
                let start = self.tok.start as usize;
                self.retokenize(T::Lt, start + 1);
                true
            }
            _ => false,
        }
    }

    pub(crate) fn parse_hint(&mut self) -> R<Hint<'a>> {
        self.parse_hint_ex(true)
    }

    /// Whether the current token is one a type starts with: those
    /// [`parse_hint_ex`](Self::parse_hint_ex) has a rule for.
    pub(crate) fn at_hint_start(&mut self) -> bool {
        self.retokenize_xhp_class_name();
        matches!(
            self.tok.kind,
            T::This
                | T::SelfKw
                | T::Double
                | T::Bool
                | T::Boolean
                | T::Binary
                | T::Int_
                | T::Integer
                | T::Float_
                | T::Real
                | T::Num
                | T::String_
                | T::Arraykey
                | T::Void
                | T::Noreturn
                | T::Resource
                | T::Mixed
                | T::Null
                | T::Category
                | T::Xhp
                | T::XhpClassName
                | T::Name
                | T::Namespace
                | T::Backslash
                | T::Varray
                | T::Vec
                | T::Keyset
                | T::Dict
                | T::Darray
                | T::Classname
                | T::Enum
                | T::Class
                | T::LParen
                | T::Shape
                | T::Tuple
                | T::Question
                | T::QuestionColon
                | T::Tilde
                | T::LtLt
        )
    }

    fn apply_hint(&self, start: u32, name: &'a str, args: &'a [Hint<'a>]) -> Hint<'a> {
        Hint {
            span: self.span_from(start),
            kind: if args.is_empty() {
                HintKind::Name(name)
            } else {
                HintKind::Generic(self.alloc(GenericHint { name, args }))
            },
        }
    }

    /// `Name`, `Name<..>` or `Name::T::U` once the name has been consumed.
    fn finish_named_hint(&mut self, start: u32, name: &'a str, allow_generic: bool) -> R<Hint<'a>> {
        if allow_generic && self.at_type_lt() {
            let args = self.parse_type_arguments()?;
            self.c_node(K::GenericTypeSpecifier);
            return Ok(self.apply_hint(start, name, args));
        }
        let root = self.apply_hint(start, name, &[]);
        if name.len() > 11 && name.as_bytes()[(name.as_bytes()[0] == b'\\') as usize] == b'H' {
            self.check_coeffect_namespace(&root, name);
        }
        if self.at(T::ColonColon) {
            return self.finish_type_constant(start, root);
        }
        self.c_node(K::SimpleTypeSpecifier);
        if name == "_" {
            return Ok(Hint {
                span: root.span,
                kind: HintKind::Wildcard,
            });
        }
        Ok(root)
    }

    fn finish_type_constant(&mut self, start: u32, root: Hint<'a>) -> R<Hint<'a>> {
        let mut names = self.vec();
        while self.eat(T::ColonColon) {
            if !self.tok.kind.is_name_like() {
                // What is no name belongs to whatever comes next.
                names.push(self.missing_name());
                self.c_node(K::TypeConstant);
                break;
            }
            let tok = self.take();
            self.c_name();
            self.c_node(K::TypeConstant);
            names.push(self.id(tok));
        }
        Ok(Hint {
            span: self.span_from(start),
            kind: HintKind::Access(self.alloc(AccessHint {
                root,
                names: names.into_bump_slice(),
            })),
        })
    }

    pub(crate) fn parse_hint_ex(&mut self, allow_attr: bool) -> R<Hint<'a>> {
        self.enter()?;
        self.retokenize_xhp_class_name();
        let start = self.tok.start;
        let hint = match self.tok.kind {
            T::This => {
                let tok = self.take();
                self.finish_named_hint(start, self.tok_text(tok), false)?
            }
            T::SelfKw => {
                let tok = self.take();
                if self.at(T::ColonColon) {
                    let root = self.apply_hint(start, self.tok_text(tok), &[]);
                    self.finish_type_constant(start, root)?
                } else {
                    self.unqualified_self_hint(tok)
                }
            }
            T::Double
            | T::Bool
            | T::Boolean
            | T::Binary
            | T::Int_
            | T::Integer
            | T::Float_
            | T::Real
            | T::Num
            | T::String_
            | T::Arraykey
            | T::Void
            | T::Noreturn
            | T::Resource
            | T::Mixed
            | T::Null
            | T::Category
            | T::Xhp
            | T::XhpClassName => {
                let tok = self.take();
                self.finish_named_hint(start, self.tok_text(tok), true)?
            }
            T::Name | T::Namespace | T::Backslash => {
                if self.tok.mixed_case_kw || self.tok.mixed_case_soft {
                    // A keyword in another case is that keyword here, and
                    // read as such.
                    self.lowercase_keyword();
                    if !self.at(T::Name) {
                        self.leave();
                        return self.parse_hint_ex(allow_attr);
                    }
                }
                let (name, _) = self.parse_qualified_name()?;
                self.finish_named_hint(start, name.name, true)?
            }
            T::Varray | T::Vec | T::Keyset | T::Dict | T::Darray | T::Classname => {
                let tok = self.take();
                let name = self.tok_text(tok);
                if tok.kind == T::Dict && self.at_type_lt() {
                    let args = self.parse_type_arguments()?;
                    if self.c_on() {
                        self.c_inline_type_arguments(true);
                        self.c_node(K::DictionaryTypeSpecifier);
                    }
                    self.apply_hint(start, name, args)
                } else if self.at_type_lt() {
                    // One type (two with a comma between them for
                    // `darray`), an optional comma and the `>`.
                    self.bump();
                    let mut args = self.vec();
                    args.push(self.parse_hint()?);
                    if tok.kind == T::Darray {
                        self.require(T::Comma);
                        args.push(self.parse_hint()?);
                    }
                    if !self.eat(T::Comma) {
                        self.c_missing();
                    }
                    self.expect_gt()?;
                    self.c_node(match tok.kind {
                        T::Vec => K::VectorTypeSpecifier,
                        T::Keyset => K::KeysetTypeSpecifier,
                        T::Varray => K::VarrayTypeSpecifier,
                        T::Darray => K::DarrayTypeSpecifier,
                        _ => K::ClassnameTypeSpecifier,
                    });
                    self.apply_hint(start, name, args.into_bump_slice())
                } else {
                    if tok.kind == T::Classname {
                        self.c_missing_n(4);
                        self.c_node(K::ClassnameTypeSpecifier);
                    } else {
                        self.c_node(K::SimpleTypeSpecifier);
                    }
                    self.apply_hint(start, name, &[])
                }
            }
            T::Enum | T::Class => {
                let kind = if self.at(T::Enum) {
                    ClassPtrKind::Enum
                } else {
                    ClassPtrKind::Class
                };
                self.bump();
                self.require(T::Lt);
                let inner = self.parse_hint()?;
                if !self.eat(T::Comma) {
                    self.c_missing();
                }
                self.expect_gt()?;
                self.c_node(K::ClassPtrTypeSpecifier);
                Hint {
                    span: self.span_from(start),
                    kind: HintKind::ClassPtr(kind, self.alloc(inner)),
                }
            }
            T::LParen => {
                let next = self.peek_kind();
                if next == T::Function || next == T::Readonly {
                    self.parse_closure_hint()?
                } else {
                    self.parse_tuple_like_hint()?
                }
            }
            T::Shape => self.parse_shape_hint()?,
            T::Tuple => {
                // `tuple<A, B>`
                let tok = self.take();
                if self.at_type_lt() {
                    self.bump();
                } else {
                    self.require_missing(T::Lt, "An opening angle bracket `<` is expected here.");
                }
                let mut args = self.vec();
                let types = self.c_mark();
                self.separated_list(
                    T::Comma,
                    ListKind::TrailingAllowed,
                    |kind| matches!(kind, T::Gt | T::GtGt | T::GtEq | T::GtGtEq),
                    TYPE_EXPECTED,
                    Self::missing_hint,
                    |p| {
                        args.push(p.parse_hint()?);
                        Ok(())
                    },
                )?;
                self.c_list(types);
                if matches!(self.tok.kind, T::Gt | T::GtGt | T::GtEq | T::GtGtEq) {
                    self.expect_gt()?;
                } else {
                    // What stands in the place of the `>` stays.
                    self.report_missing(PAREN_OR_COMMA_EXPECTED);
                    self.c_missing();
                }
                self.c_node(K::TupleTypeExplicitSpecifier);
                self.apply_hint(start, self.tok_text(tok), args.into_bump_slice())
            }
            T::Question => {
                self.bump();
                let inner = self.parse_hint()?;
                self.c_node(K::NullableTypeSpecifier);
                Hint {
                    span: self.span_from(start),
                    kind: HintKind::Option(self.alloc(inner)),
                }
            }
            T::QuestionColon => {
                // `?:xhp:name`: HHVM's lexer never forms `?:` inside a type.
                let s = self.tok.start as usize;
                self.retokenize(T::Question, s + 1);
                self.bump();
                let inner = self.parse_hint()?;
                self.c_node(K::NullableTypeSpecifier);
                Hint {
                    span: self.span_from(start),
                    kind: HintKind::Option(self.alloc(inner)),
                }
            }
            T::Tilde => {
                self.bump();
                let inner = self.parse_hint()?;
                self.c_node(K::LikeTypeSpecifier);
                if !self.hhi {
                    self.require_feature(start, self.prev_end, Feature::LikeTypeHints);
                }
                Hint {
                    span: self.span_from(start),
                    kind: HintKind::Like(self.alloc(inner)),
                }
            }
            T::LtLt if allow_attr => {
                let attrs = self.parse_attribute_spec()?;
                let inner = self.parse_hint()?;
                self.c_node(K::AttributizedSpecifier);
                // Only `<<__Soft>>` may be applied to a type.
                if attrs.iter().any(|attr| attr.name.name != "__Soft") {
                    self.report_lowering(
                        start,
                        self.prev_end,
                        "Only the `__Soft` attribute is allowed here.",
                    );
                }
                Hint {
                    span: self.span_from(start),
                    kind: HintKind::Soft(self.alloc(inner)),
                }
            }
            _ => self.unexpected_hint(),
        };
        let hint = if self.at(T::With) && self.peek_kind() == T::LBrace {
            self.parse_refinement(start, hint)?
        } else {
            hint
        };
        self.leave();
        Ok(hint)
    }

    /// `<T1, T2>`; the current token must be `<`.
    pub(crate) fn parse_type_arguments(&mut self) -> R<&'a [Hint<'a>]> {
        if !self.at_type_lt() {
            return self.fail("expected `<`");
        }
        self.bump();
        let mut args = self.vec();
        let types = self.c_mark();
        loop {
            // Where a type is due and the list closes, the file ends or a
            // comma stands, HHVM takes the type for missing.
            if matches!(
                self.tok.kind,
                T::Gt | T::GtGt | T::GtEq | T::GtGtEq | T::Eof | T::Comma
            ) {
                let more = self.missing_list_item("A type specifier is expected here.");
                self.missing_hint();
                if more {
                    continue;
                }
                break;
            }
            if self.at(T::Reify) {
                // `reify` belongs in declarations: HHVM gives up on a type
                // argument that has it.
                let start = self.take().start;
                args.push(self.parse_hint()?);
                self.c_node(K::ReifiedTypeArgument);
                self.report_lowering(
                    start,
                    self.prev_end,
                    "`reify` keyword can only appear at function or class type parameter position",
                );
                self.unit_aborted = true;
            } else {
                args.push(self.parse_hint()?);
            }
            if self.eat_separator(T::Comma) {
                if matches!(self.tok.kind, T::Gt | T::GtGt | T::GtEq | T::GtGtEq) {
                    break;
                }
                continue;
            }
            break;
        }
        self.c_list(types);
        if matches!(self.tok.kind, T::Gt | T::GtGt | T::GtEq | T::GtGtEq) {
            self.expect_gt()?;
        } else {
            // What stands in the place of the `>` stays.
            self.report_missing("A closing angle bracket `>` or comma is expected here.");
            self.c_missing();
        }
        self.c_node(K::TypeArguments);
        Ok(args.into_bump_slice())
    }

    fn parse_closure_hint(&mut self) -> R<Hint<'a>> {
        // A function type in another is two levels down.
        self.enter()?;
        let hint = self.parse_closure_hint_inner();
        self.leave();
        hint
    }

    fn parse_closure_hint_inner(&mut self) -> R<Hint<'a>> {
        let start = self.tok.start;
        self.require(T::LParen);
        let is_readonly = self.eat(T::Readonly);
        if !is_readonly {
            self.c_missing();
        }
        self.require(T::Function);
        let tparams = self.parse_type_params(true)?;
        self.require(T::LParen);
        let mut params = self.vec();
        let mut written: SmallVec<[WrittenParam; 8]> = SmallVec::new();
        let list_start = self.tok.start;
        let mut trailing_comma = None;
        let mut all_flags = 0u8;
        let mut named_count = 0;
        let parameters = self.c_mark();
        while !self.at(T::RParen) {
            let start = self.tok.start;
            let mut inout = None;
            let mut flags = 0u8;
            let mut named = false;
            // The modifier keywords as written; HHVM reads at most four.
            let mut mods: [ParamModifier; 4] = [(T::Eof, 0, 0); 4];
            let mut mod_count = 0;
            let modifiers = self.c_mark();
            while mod_count < 4
                && matches!(
                    self.tok.kind,
                    T::Optional | T::Inout | T::Named | T::Readonly
                )
            {
                let tok = self.take();
                mods[mod_count] = (tok.kind, tok.start, tok.end);
                mod_count += 1;
                match tok.kind {
                    T::Optional => flags |= fun_hint_flags::OPTIONAL,
                    T::Inout => inout = Some(Self::tok_span(tok)),
                    T::Named => named = true,
                    _ => flags |= fun_hint_flags::READONLY,
                }
            }
            self.c_parameter_modifiers(modifiers);
            let ellipsis = self.c_mark();
            let leading_ellipsis = self.eat(T::DotDotDot);
            let ellipsis_end = self.prev_end;
            let mut untyped_at = None;
            // HHVM's parser lets the type go missing, but a parameter that
            // is nothing at all is a syntax error.
            let hint = if self.at_hint_start() || (mod_count == 0 && !leading_ellipsis) {
                if !leading_ellipsis {
                    self.c_missing();
                }
                self.parse_hint()?
            } else {
                // Without a type, the `...` counts as the one after the
                // type (and the name).
                self.c_insert_missing_n(ellipsis, 2);
                let at = self.missing_pos();
                untyped_at = Some(at);
                Hint {
                    span: Span::new(at, at),
                    kind: HintKind::Name(""),
                }
            };
            let typed = untyped_at.is_none();
            let mut name = None;
            let mut name_start = None;
            if self.at(T::Variable) {
                if !named {
                    // Reported on the name and all the trivia around it.
                    let start = self.missing_pos();
                    let end = self.tok.end as usize;
                    let end = end + crate::cst::trivia::hhvm_trailing_width(self.bytes, end);
                    self.report_syntax(
                        start,
                        end as u32,
                        "Unexpected parameter name. Only `named` parameters can have names.",
                    );
                }
                let tok = self.take();
                name = Some(self.tok_text(tok));
                name_start = Some(tok.start);
            } else {
                self.c_missing();
                if named {
                    name = Some("");
                    if !self.at(T::DotDotDot) {
                        // Only `named T...` goes without a name.
                        if let Some(&(_, start, end)) =
                            mods[..mod_count].iter().find(|m| m.0 == T::Named)
                        {
                            self.report_syntax(
                                start,
                                end,
                                "Each `named` parameter in a function type must have a parameter name. For example: `(function(named int $x, named int $y): void)`.",
                            );
                        }
                    }
                }
            }
            let variadic = if typed {
                let found = self.eat(T::DotDotDot);
                if !found {
                    self.c_missing();
                }
                found
            } else {
                if leading_ellipsis {
                    self.c_move_last(ellipsis + 2);
                } else {
                    self.c_missing();
                }
                leading_ellipsis
            };
            if variadic {
                flags |= fun_hint_flags::VARIADIC;
            } else if let Some(at) = untyped_at {
                // HHVM translates the variadic parameters after the others.
                self.abort_lowering(at, at + 1, MISSING_TYPE_HINT);
            }
            if typed && leading_ellipsis {
                flags |= fun_hint_flags::SPLAT;
            }
            self.c_node(K::ClosureParameterTypeSpecifier);
            // A `...` moved behind the name is still where the parameter
            // ends as far as HHVM is concerned.
            let end = if typed || !leading_ellipsis {
                self.prev_end
            } else {
                ellipsis_end
            };
            let mut named_slot = false;
            if mod_count != 0 {
                let mods = &mods[..mod_count];
                self.check_param_modifiers(mods, (start, end), (start, end), true);
                // Modifiers written out of order sit in HHVM's tree in the
                // order written, so the third is where it looks for `named`.
                let in_order = mods
                    .windows(2)
                    .all(|w| param_modifier_rank(w[0].0) < param_modifier_rank(w[1].0));
                named_slot = if in_order { named } else { mod_count >= 3 };
                named_count += named_slot as u32;
            }
            all_flags |= flags;
            written.push(WrittenParam {
                start,
                end,
                name_start,
                untyped_at,
                named_slot,
            });
            params.push(FunHintParam {
                hint,
                inout,
                named: name,
                flags,
            });
            if self.at(T::Comma) {
                trailing_comma = Some(self.tok.start);
                self.bump();
                self.c_list_item(true);
            } else {
                trailing_comma = None;
                self.c_list_item(false);
                break;
            }
        }
        let list_end = self.prev_end;
        self.c_list(parameters);
        self.require(T::RParen);
        if all_flags & fun_hint_flags::VARIADIC != 0 {
            self.check_fun_hint_variadics(&params, &written, (list_start, list_end));
        }
        if all_flags & (fun_hint_flags::VARIADIC | fun_hint_flags::SPLAT) != 0 || named_count > 1 {
            self.check_fun_hint_params(&params, &written, trailing_comma);
        }
        let ctxs = self.parse_contexts_opt()?;
        self.require(T::Colon);
        let readonly_ret = self.eat(T::Readonly);
        if !readonly_ret {
            self.c_missing();
        }
        let ret = self.parse_hint()?;
        self.require(T::RParen);
        self.c_node(K::ClosureTypeSpecifier);
        let fun = FunHint {
            tparams,
            params: params.into_bump_slice(),
            ctxs,
            ret,
            is_readonly,
            readonly_ret,
        };
        Ok(Hint {
            span: self.span_from(start),
            kind: HintKind::Fun(self.alloc(fun)),
        })
    }

    /// What HHVM's translation to the AST objects to in the variadic
    /// parameters of a function type, which it gets to after the others.
    /// `list` is the extent of the parameter list.
    #[cold]
    fn check_fun_hint_variadics(
        &mut self,
        params: &[FunHintParam<'a>],
        written: &[WrittenParam],
        list: (u32, u32),
    ) {
        let mut unnamed = 0;
        for (p, w) in params.iter().zip(written) {
            if p.flags & fun_hint_flags::VARIADIC == 0 {
                continue;
            }
            if w.untyped_at.is_some() {
                self.report_lowering(w.start, w.end, "Cannot use ... without a typehint");
            }
            if p.flags & fun_hint_flags::OPTIONAL != 0 {
                self.report_lowering(
                    w.start,
                    w.end,
                    "Cannot use `optional` on variadic or splat parameters",
                );
            }
            if let Some(at) = w.untyped_at {
                self.abort_lowering(at, at + 1, MISSING_TYPE_HINT);
            }
            if p.named.is_none() {
                unnamed += 1;
            }
        }
        if unnamed > 1 {
            self.abort_lowering(
                list.0,
                list.1,
                format!(
                    "{unnamed} unnamed variadic parameters found. There should be no more than one."
                ),
            );
        }
    }

    /// The checks on a parameter list as a whole (see
    /// [`check_param_list`](Self::check_param_list)) for the parameters of
    /// a function type, and the one that only they have: no two `named`
    /// parameters may share a name.
    #[cold]
    fn check_fun_hint_params(
        &mut self,
        params: &[FunHintParam<'a>],
        written: &[WrittenParam],
        trailing_comma: Option<u32>,
    ) {
        // The parameters the way those checks look at a declared one.
        let mut declared: SmallVec<[Param<'a>; 8]> = SmallVec::new();
        let mut spans: SmallVec<[(u32, u32); 8]> = SmallVec::new();
        let mut names: SmallVec<[&str; 8]> = SmallVec::new();
        let mut duplicate = None;
        for (p, w) in params.iter().zip(written) {
            let mut flags = 0;
            for (flag, declared_flag) in [
                (fun_hint_flags::VARIADIC, param_flags::VARIADIC),
                (fun_hint_flags::READONLY, param_flags::READONLY),
                (fun_hint_flags::OPTIONAL, param_flags::OPTIONAL),
                (fun_hint_flags::SPLAT, param_flags::SPLAT),
            ] {
                if p.flags & flag != 0 {
                    flags |= declared_flag;
                }
            }
            let name = match (p.named, w.name_start) {
                (Some(name), Some(start)) => {
                    flags |= param_flags::NAMED;
                    let span = Span::new(start, start + name.len() as u32);
                    if w.named_slot && p.flags & fun_hint_flags::VARIADIC == 0 {
                        if names.contains(&name) {
                            duplicate.get_or_insert(span);
                        } else {
                            names.push(name);
                        }
                    }
                    Id { span, name }
                }
                (named, _) => {
                    if named.is_some() {
                        flags |= param_flags::NAMED;
                    }
                    Id {
                        span: Span::new(w.end, w.end),
                        name: "...",
                    }
                }
            };
            spans.push((w.start, w.end));
            declared.push(Param {
                name,
                hint: None,
                default: None,
                attrs: &[],
                visibility: None,
                inout: p.inout,
                flags,
            });
        }
        self.check_param_list(&declared, &spans, trailing_comma, true);
        if let Some(span) = duplicate {
            self.report(
                span.start,
                span.end,
                "A function type cannot contain duplicate named parameter names.",
            );
        }
    }

    /// `(A, B)`, `(A | B)` or `(A & B)`.
    fn parse_tuple_like_hint(&mut self) -> R<Hint<'a>> {
        let start = self.tok.start;
        self.require(T::LParen);
        let mut required = self.vec();
        let mut optional = self.vec();
        let mut variadic = None;
        let mut splat = None;
        let mut separator = None;
        // Where the complaints about the elements are among the errors of
        // HHVM's translation to the AST.
        let mut misplaced: SmallVec<[usize; 2]> = SmallVec::new();
        let types = self.c_mark();
        // An element is due after the `(` unless the `)` follows at once,
        // and after each separator unless the `)` follows it.
        let mut more = !self.at(T::RParen);
        while more {
            // Where an element is due and the list closes, the file ends
            // or a separator stands, HHVM takes the element for missing.
            let kind = self.tok.kind;
            if matches!(kind, T::RParen | T::Eof) {
                self.report_missing(TYPE_EXPECTED);
                self.c_missing();
                self.missing_hint();
                self.c_list_item(false);
                break;
            }
            if matches!(kind, T::Comma | T::Bar | T::Amp) {
                if *separator.get_or_insert(kind) != kind {
                    self.report_missing(MATCHING_SEPARATOR_EXPECTED);
                }
                self.c_missing();
                self.bump();
                self.report_missing(TYPE_EXPECTED);
                self.missing_hint();
                self.c_list_item(true);
                continue;
            }
            let is_optional = self.eat(T::Optional);
            if !is_optional {
                self.c_missing();
            }
            let pre_ellipsis = self.eat(T::DotDotDot);
            if !pre_ellipsis {
                self.c_missing();
            }
            let hint = self.parse_hint()?;
            let ellipsis = self.eat(T::DotDotDot);
            if !ellipsis {
                self.c_missing();
            }
            self.c_node(K::TupleOrUnionOrIntersectionElementTypeSpecifier);
            // What HHVM's translation to the AST finds wrong with the
            // elements of a tuple type is about the whole type, which has
            // not ended yet.
            let mut wrong = |p: &mut Self, message: &'static str| {
                misplaced.extend(p.begin_lowering_error(start, message));
            };
            if matches!(separator.unwrap_or(self.tok.kind), T::Bar | T::Amp) {
                // In a union or intersection HHVM only looks at the types.
                required.push(hint);
            } else if ellipsis || pre_ellipsis {
                if ellipsis && pre_ellipsis {
                    wrong(self, "A splat element cannot also be variadic");
                }
                if variadic.is_some() || splat.is_some() {
                    wrong(self, "There must be only one variadic or splat element");
                } else if is_optional {
                    wrong(
                        self,
                        "Cannot use `optional` on variadic or splat parameters",
                    );
                }
                if ellipsis {
                    variadic = Some(hint);
                } else {
                    if !optional.is_empty() {
                        wrong(
                            self,
                            "There must be no splat elements following an optional element",
                        );
                    }
                    splat = Some(hint);
                }
            } else {
                if variadic.is_some() || splat.is_some() {
                    wrong(
                        self,
                        "There must be no elements following a variadic or splat element",
                    );
                }
                if is_optional {
                    optional.push(hint);
                } else {
                    if !optional.is_empty() {
                        wrong(
                            self,
                            "There must be no required elements following an optional element",
                        );
                    }
                    required.push(hint);
                }
            }
            match self.tok.kind {
                T::Comma | T::Bar | T::Amp => {
                    let kind = self.tok.kind;
                    // HHVM says so about a separator that is not the one
                    // the list started with, and reads on.
                    if *separator.get_or_insert(kind) != kind {
                        self.report_missing(MATCHING_SEPARATOR_EXPECTED);
                    }
                    self.bump();
                    self.c_list_item(true);
                    more = !self.at(T::RParen);
                }
                _ => {
                    self.c_list_item(false);
                    break;
                }
            }
        }
        self.c_list(types);
        if self.eat(T::RParen) {
            if self.c_on() {
                self.c_node(match separator {
                    Some(T::Bar) => K::UnionTypeSpecifier,
                    Some(T::Amp) => K::IntersectionTypeSpecifier,
                    _ => K::TupleTypeSpecifier,
                });
            }
        } else {
            // What stands in the place of the `)` stays, and HHVM takes
            // what it has for a tuple.
            self.report_missing(PAREN_OR_COMMA_EXPECTED);
            self.c_missing();
            self.c_node(K::TupleTypeSpecifier);
            if matches!(separator, Some(T::Bar | T::Amp)) {
                separator = Some(T::Comma);
            }
        }
        let span = self.span_from(start);
        for error in misplaced {
            self.end_lowering_error(Some(error), span.end);
        }
        let kind = match separator {
            Some(T::Bar) | Some(T::Amp) => {
                self.require_feature(start, span.end, Feature::UnionIntersectionTypeHints);
                if separator == Some(T::Bar) {
                    HintKind::Union(required.into_bump_slice())
                } else {
                    HintKind::Intersection(required.into_bump_slice())
                }
            }
            _ => HintKind::Tuple(self.alloc(TupleHint {
                required: required.into_bump_slice(),
                optional: optional.into_bump_slice(),
                variadic,
                splat,
            })),
        };
        Ok(Hint { span, kind })
    }

    /// Converts an expression used as a shape key. `raw_start` is where the
    /// key begins in the source and `in_type` tells a shape type from a
    /// shape expression. Keys HHVM reports an error for are replaced by a
    /// placeholder.
    pub(crate) fn shape_key(
        &mut self,
        e: Expr<'a>,
        raw_start: u32,
        in_type: bool,
    ) -> R<ShapeKey<'a>> {
        const INVALID: &str =
            "Shape field name must be a nonempty single-quoted string or a class constant";
        // The key may be missing altogether (see `repair.rs`).
        let (start, end) = (raw_start, self.prev_end.max(raw_start));
        // Parentheses around the key make it something else.
        let bare = e.span.start == raw_start && e.span.end == end;
        match e.kind {
            ExprKind::String(value)
                if bare && matches!(self.bytes[e.span.start as usize], b'\'' | b'"') =>
            {
                // HHVM reads a double-quoted `''` or `""` as nothing.
                let value = if self.bytes[e.span.start as usize] == b'"'
                    && (value == b"''" || value == b"\"\"")
                {
                    &[]
                } else {
                    value
                };
                if e.span.end - e.span.start == 2 {
                    self.report_lowering(start, end, INVALID);
                } else if looks_like_int(value) {
                    self.report_lowering(
                        start,
                        end,
                        "Shape field name must not be an int-like string (i.e. \"123\")",
                    );
                }
                Ok(ShapeKey::Str {
                    span: e.span,
                    value,
                })
            }
            ExprKind::ClassConst(access) | ExprKind::ClassGet(access) if bare => {
                // The key is not kept as an expression, so what is checked
                // of one has to be checked here.
                self.check_scope_resolution(&e, access, self.class.is_some());
                // HHVM takes the text of the qualifier and of the name,
                // whatever they are, for the names of a class and of a
                // constant.
                let name = if self.bytes[end as usize - 1] == b'}' {
                    let span = Span::new(self.braced_name_start(&access.class), end);
                    Id {
                        span,
                        name: span.text(self.src),
                    }
                } else {
                    access.name
                };
                let class = match access.class.kind {
                    ExprKind::Id(name) => Id {
                        span: access.class.span,
                        name,
                    },
                    _ => {
                        let span = Span::new(start, self.qualifier_end(&access.class));
                        Id {
                            span,
                            name: span.text(self.src),
                        }
                    }
                };
                if class.name == "parent" || class.name == "static" {
                    // HHVM gives up on the declaration after this one.
                    self.report_lowering(
                        start,
                        end,
                        "Cannot use `parent::` or `static::` for class constant shape keys",
                    );
                    self.unit_aborted = true;
                } else if name.name == "class" {
                    self.report_lowering(
                        start,
                        end,
                        format!(
                            "Shape field name must not be a `{}::class` expression",
                            class.name
                        ),
                    );
                }
                Ok(ShapeKey::ClassConst(self.alloc((class, name))))
            }
            ExprKind::Nameof(target) if bare => match target.kind {
                ExprKind::Id(class) => {
                    if matches!(class, "self" | "parent" | "static") {
                        self.report_lowering(
                            start,
                            end,
                            format!(
                                "Shape field name must not be a `{}::class` expression",
                                class
                            ),
                        );
                    }
                    Ok(ShapeKey::Nameof {
                        start,
                        class: self.alloc(Id {
                            span: target.span,
                            name: class,
                        }),
                    })
                }
                _ => self.fail_at(raw_start, "unsupported shape key"),
            },
            _ => {
                // HHVM's translation to the AST says this, and for an
                // expression so do its syntax checks before that, whatever
                // happens to the enclosing declaration.
                if !in_type {
                    self.report(start, end, INVALID);
                }
                self.report_lowering(start, end, INVALID);
                Ok(ShapeKey::Str {
                    span: crate::span::Span::new(start, end),
                    value: &self.bytes[start as usize..end as usize],
                })
            }
        }
    }

    /// Where the qualifier of a scope resolution ends, any parentheses
    /// around it included.
    fn qualifier_end(&self, qualifier: &Expr<'a>) -> u32 {
        let mut end = qualifier.span.end as usize;
        let mut at = end;
        loop {
            match self.bytes.get(at) {
                Some(b')') => {
                    at += 1;
                    end = at;
                }
                Some(_) => match crate::cst::trivia::scan(self.bytes, at) {
                    Some(piece) => at += piece.width,
                    None => return end as u32,
                },
                None => return end as u32,
            }
        }
    }

    fn parse_shape_hint(&mut self) -> R<Hint<'a>> {
        let start = self.tok.start;
        self.require(T::Shape);
        self.require(T::LParen);
        let mut fields = self.vec();
        let mut unknown_fields = None;
        // Whether the last element was followed by a comma.
        let mut trailing_comma = true;
        let field_list = self.c_mark();
        loop {
            if self.at(T::RParen) {
                break;
            }
            let element = if self.at(T::DotDotDot) {
                // A bare `...` closes the list; `...T` splats another shape.
                if matches!(self.peek_kind(), T::RParen | T::Comma) {
                    break;
                }
                let splat_start = self.take().start;
                let splat = self.parse_hint()?;
                self.c_node(K::ShapeSplatSpecifier);
                // Either of the two features lets a shape be splatted.
                let features =
                    Feature::ShapeSplatConcrete.bit() | Feature::ShapeSplatTypeParameters.bit();
                if self.features & features == 0 {
                    self.require_feature(splat_start, self.prev_end, Feature::ShapeSplatConcrete);
                }
                ShapeHintElement::Splat(splat)
            } else if self.at_typed_ellipsis() {
                // The type of the unknown fields follows the list of fields.
                self.c_list(field_list);
                unknown_fields = Some(self.parse_hint()?);
                break;
            } else if self.at(T::Absent) {
                let absent_start = self.tok.start;
                self.bump();
                let key_start = self.tok.start;
                let key = self.parse_expr()?;
                self.c_node(K::AbsentFieldSpecifier);
                self.require_feature(absent_start, self.prev_end, Feature::ShapeSplatConcrete);
                let key = self.shape_key(key, key_start, true)?;
                ShapeHintElement::Absent {
                    span: self.span_from(absent_start),
                    key,
                }
            } else {
                let optional = self.eat(T::Question);
                if !optional {
                    self.c_missing();
                }
                let key_start = self.tok.start;
                let key = self.parse_expr()?;
                let key = self.shape_key(key, key_start, true)?;
                self.require(T::FatArrow);
                let hint = self.parse_hint()?;
                self.c_node(K::FieldSpecifier);
                ShapeHintElement::Field(ShapeHintField {
                    key,
                    optional,
                    hint,
                })
            };
            fields.push(element);
            trailing_comma = self.eat_separator(T::Comma);
            if !trailing_comma {
                // What ends the list is fine without a comma in front (as
                // far as the parser goes); anything else is one element
                // too many.
                let ends = matches!(self.tok.kind, T::RParen | T::Eof)
                    || (self.at(T::DotDotDot) && matches!(self.peek_kind(), T::RParen | T::Comma));
                if !ends && !self.at_typed_ellipsis() {
                    self.report_missing("A comma `,` is expected here.");
                }
                break;
            }
        }
        if unknown_fields.is_none() {
            self.c_list(field_list);
            self.c_missing();
        }
        let ellipsis = self.eat(T::DotDotDot);
        if !ellipsis {
            self.c_missing();
        }
        let open = ellipsis || unknown_fields.is_some();
        self.require(T::RParen);
        self.c_node(K::ShapeTypeSpecifier);
        if open && !trailing_comma {
            self.report(
                start,
                self.prev_end,
                "A comma is required before the `...` in a shape type",
            );
        }
        let shape = ShapeHint {
            fields: fields.into_bump_slice(),
            open,
            unknown_fields,
        };
        Ok(Hint {
            span: self.span_from(start),
            kind: HintKind::Shape(self.alloc(shape)),
        })
    }

    /// Whether a type followed by `...` starts here (`string...` at the end
    /// of a shape type), as opposed to a field whose key looks like a type.
    fn at_typed_ellipsis(&mut self) -> bool {
        // The usual case, a literal key, cannot start a type.
        if matches!(self.tok.kind, T::SingleQuoted | T::DoubleQuoted | T::Int) {
            return false;
        }
        let snap = self.snapshot();
        let saved_error = self.error;
        let saved_lexer_error = self.lexer.error;
        let found = self.parse_hint().is_ok() && self.at(T::DotDotDot);
        self.restore(snap);
        self.error = saved_error;
        self.lexer.error = saved_lexer_error;
        found
    }

    /// `[io, rx]` if present.
    pub(crate) fn parse_contexts_opt(&mut self) -> R<Option<Contexts<'a>>> {
        if !self.at(T::LBracket) {
            self.c_missing();
            return Ok(None);
        }
        let start = self.tok.start;
        self.bump();
        let mut hints = self.vec();
        let types = self.c_mark();
        while !self.at(T::RBracket) {
            let item_start = self.tok.start;
            let hint = match self.tok.kind {
                T::Ctx => {
                    self.bump();
                    let var = self.require_id(T::Variable);
                    self.c_node(K::FunctionCtxTypeSpecifier);
                    Hint {
                        span: self.span_from(item_start),
                        kind: HintKind::FunContext(var.name),
                    }
                }
                T::Variable => {
                    let var = self.take();
                    let root = Hint {
                        span: Self::tok_span(var),
                        kind: HintKind::Var(self.tok_text(var)),
                    };
                    self.require(T::ColonColon);
                    let name = self.require_id(T::Name);
                    self.c_node(K::TypeConstant);
                    let names = self.arena.alloc_slice_copy(&[name]);
                    Hint {
                        span: self.span_from(item_start),
                        kind: HintKind::Access(self.alloc(AccessHint { root, names })),
                    }
                }
                _ => self.parse_hint_ex(false)?,
            };
            hints.push(hint);
            if !self.eat_separator(T::Comma) {
                break;
            }
        }
        self.c_list(types);
        self.require(T::RBracket);
        self.c_node(K::Contexts);
        Ok(Some(Contexts {
            span: self.span_from(start),
            hints: hints.into_bump_slice(),
        }))
    }

    /// `<T as Foo, reify U, +V>` if present.
    pub(crate) fn parse_type_params_opt(&mut self) -> R<&'a [Tparam<'a>]> {
        self.parse_type_params(false)
    }

    /// Like [`parse_type_params_opt`](Self::parse_type_params_opt).
    /// `polymorphic` is set for the type parameters of a function type or
    /// of a function written as an expression, which are plainer than
    /// those of a declaration.
    pub(crate) fn parse_type_params(&mut self, polymorphic: bool) -> R<&'a [Tparam<'a>]> {
        if !self.at_type_lt() {
            self.c_missing();
            return Ok(&[]);
        }
        self.bump();
        let mut params = self.vec();
        if !self.reified_tparams.is_empty() {
            self.reified_tparams.clear();
        }
        let parameters = self.c_mark();
        loop {
            let start = self.tok.start;
            let attrs = if self.at(T::LtLt) {
                let attrs = self.parse_attribute_spec()?;
                self.check_declaration_attrs(attrs);
                attrs
            } else {
                self.c_missing();
                &[]
            };
            let reified = self.eat(T::Reify);
            if !reified {
                self.c_missing();
            }
            let variance = match self.tok.kind {
                T::Plus => {
                    self.bump();
                    Variance::Covariant
                }
                T::Minus => {
                    self.bump();
                    Variance::Contravariant
                }
                _ => {
                    self.c_missing();
                    Variance::Invariant
                }
            };
            let name = self.expect_name_allow_keywords()?;
            let mut constraints = self.vec();
            let constraint_list = self.c_mark();
            loop {
                let kind = match self.tok.kind {
                    T::As => ConstraintKind::As,
                    T::Super => ConstraintKind::Super,
                    _ => break,
                };
                self.bump();
                constraints.push((kind, self.parse_hint()?));
                self.c_node(K::TypeConstraint);
            }
            self.c_list(constraint_list);
            self.c_node(K::TypeParameter);
            if reified {
                if variance != Variance::Invariant {
                    self.report_lowering(
                        start,
                        self.prev_end,
                        "Reified generics cannot be covariant or contravariant",
                    );
                }
                self.reified_tparams
                    .push((params.len(), start, self.prev_end));
            }
            if polymorphic {
                self.check_polymorphic_type_param(start, reified, variance, attrs);
            }
            params.push(Tparam {
                name,
                variance,
                reified,
                attrs,
                constraints: constraints.into_bump_slice(),
            });
            if self.eat_separator(T::Comma) {
                if matches!(self.tok.kind, T::Gt | T::GtGt | T::GtEq | T::GtGtEq) {
                    break;
                }
                continue;
            }
            break;
        }
        self.c_list(parameters);
        self.expect_gt()?;
        self.c_node(K::TypeParameters);
        Ok(params.into_bump_slice())
    }

    /// What a type parameter of a function type or function expression may
    /// not have; `start` is where the parameter begins.
    fn check_polymorphic_type_param(
        &mut self,
        start: u32,
        reified: bool,
        variance: Variance,
        attrs: &[Attribute<'a>],
    ) {
        let end = self.prev_end;
        if reified {
            self.report_lowering(
                start,
                end,
                "Reified generics are not allowed in polymorphic function hints or polymorphic lambda signatures.",
            );
        }
        if variance != Variance::Invariant {
            self.report_lowering(
                start,
                end,
                "Variance annotation are not allowed on generics in polymorphic function hints or polymorphic lambda signatures.",
            );
        }
        for attr in attrs {
            if !attr.args.is_empty() {
                self.report_lowering(
                    start,
                    end,
                    "Attributes with parameters are not allowed for type parameters in polymorphic function hints or polymorphic lambda signatures.",
                );
            }
        }
    }

    /// `where T as Foo, U = Bar` if present.
    pub(crate) fn parse_where_opt(&mut self) -> R<&'a [WhereConstraint<'a>]> {
        if !self.eat(T::Where) {
            self.c_missing();
            return Ok(&[]);
        }
        let mut out = self.vec();
        let constraints = self.c_mark();
        loop {
            if matches!(self.tok.kind, T::LBrace | T::Semicolon | T::Bar) {
                break;
            }
            let constraint_start = self.tok.start;
            let left = self.parse_hint()?;
            let kind = match self.tok.kind {
                T::As => Some(ConstraintKind::As),
                T::Super => Some(ConstraintKind::Super),
                T::Eq => Some(ConstraintKind::Eq),
                _ => None,
            };
            if kind.is_some() {
                self.bump();
            } else {
                // HHVM's parser lets it pass; its translation to the AST
                // cannot do without.
                self.c_missing();
                self.missing_syntax("constraint operator");
            }
            let right = self.parse_hint()?;
            self.c_node(K::WhereConstraint);
            if let Some(kind) = kind {
                out.push(WhereConstraint { left, kind, right });
            }
            // The comma between two constraints may be left out.
            self.eat_separator(T::Comma);
            if self.at(T::Eof) || self.tok.start == constraint_start {
                break;
            }
        }
        self.c_list(constraints);
        self.c_node(K::WhereClause);
        Ok(out.into_bump_slice())
    }

    /// `as T` / `super T` bounds on type constants and aliases. Aliases may
    /// have at most one bound of each kind.
    pub(crate) fn parse_bounds(&mut self, unique: bool) -> R<&'a [(ConstraintKind, Hint<'a>)]> {
        let mut out = self.vec();
        let constraints = self.c_mark();
        loop {
            let kind = match self.tok.kind {
                T::As => ConstraintKind::As,
                T::Super => ConstraintKind::Super,
                _ => break,
            };
            self.bump();
            out.push((kind, self.parse_hint()?));
            self.c_node(K::TypeConstraint);
        }
        self.c_list(constraints);
        for kind in [ConstraintKind::As, ConstraintKind::Super] {
            if unique && out.iter().filter(|(k, _)| *k == kind).count() > 1 {
                return self.fail("multiple bounds of the same kind");
            }
        }
        Ok(out.into_bump_slice())
    }
}
