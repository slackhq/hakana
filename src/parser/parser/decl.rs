//! Top-level declarations and class members.

use smallvec::SmallVec;

use super::List as BVec;
use super::repair::ListKind;
use super::{Expected, Parser, R};
use crate::ast::*;
use crate::checks::{
    ClassDecl, ClassInfo, Feature, FunHeader, MemberName, ParamCtx, ParamModifier, has_attr,
};
use crate::cst::{SyntaxKind as K, TokenKind};
use crate::lexer::{Token, is_name_start};
use crate::span::Span;
use crate::token::T;

/// How many modifier tokens of a declaration are remembered for error
/// checking; no valid declaration has that many.
pub(crate) const MAX_MODIFIERS: usize = 8;

/// Modifier keywords in front of a declaration.
#[derive(Clone, Copy)]
pub(crate) struct Modifiers {
    pub(crate) is_abstract: bool,
    pub(crate) is_final: bool,
    pub(crate) is_static: bool,
    pub(crate) is_async: bool,
    pub(crate) is_readonly: bool,
    pub(crate) has_internal: bool,
    pub(crate) has_protected: bool,
    pub(crate) has_private: bool,
    /// The visibility HHVM derives when the last one written wins.
    pub(crate) visibility: Option<Visibility>,
    pub(crate) count: u32,
    /// The modifier tokens in source order (kind and start offset).
    pub(crate) toks: [(T, u32); MAX_MODIFIERS],
}

impl Default for Modifiers {
    fn default() -> Self {
        Modifiers {
            is_abstract: false,
            is_final: false,
            is_static: false,
            is_async: false,
            is_readonly: false,
            has_internal: false,
            has_protected: false,
            has_private: false,
            visibility: None,
            count: 0,
            toks: [(T::Eof, 0); MAX_MODIFIERS],
        }
    }
}

const REIFY_MISPLACED: &str =
    "`reify` keyword can only appear at function or class type parameter position";

const TYPE_EXPECTED: &str = "A type specifier is expected here.";

impl<'a> Parser<'a> {
    /// The type parameters of a class or function have just been parsed:
    /// a `reify` one must not share its name with a later `reify` one, nor
    /// (those of a method) with a type parameter of the class.
    fn check_reified_shadowing(&mut self, tparams: &[Tparam<'a>], class_tparams: &[Tparam<'a>]) {
        if self.reified_tparams.is_empty() {
            return;
        }
        const SHADOWED: &str = "You may not shadow a reified parameter";
        for (index, start, end) in std::mem::take(&mut self.reified_tparams) {
            let Some(tparam) = tparams.get(index) else {
                continue;
            };
            let name = tparam.name.name;
            if tparams[index + 1..]
                .iter()
                .any(|later| later.reified && later.name.name == name)
            {
                self.report(start, end, SHADOWED);
            }
            if class_tparams.iter().any(|outer| outer.name.name == name) {
                self.report(start, end, SHADOWED);
            }
        }
    }

    // ------------------------------------------------------------------
    // Attributes and parameters

    /// `<<Foo, Bar(1)>>`; the current token must be `<<`.
    pub(crate) fn parse_attribute_spec(&mut self) -> R<&'a [Attribute<'a>]> {
        let start = self.expect(T::LtLt)?.start;
        let mut attrs = self.vec();
        self.parse_attribute_list(&mut attrs, false, start)?;
        self.c_node(K::OldAttributeSpecification);
        Ok(attrs.into_bump_slice())
    }

    /// The attributes and closing `>>` of an attribute specification.
    /// `file` is set for `<<file: ...>>` attributes; `spec_start` is where
    /// the opening `<<` is.
    fn parse_attribute_list(
        &mut self,
        attrs: &mut BVec<'a, Attribute<'a>>,
        file: bool,
        spec_start: u32,
    ) -> R<()> {
        let mut special = false;
        if !self.simplihack_attrs.is_empty() {
            self.simplihack_attrs.clear();
        }
        let attributes = self.c_mark();
        // Where the complaint of HHVM's translation to the AST about an
        // attribute that is not there is, if it made one: it is about the
        // whole specification, which has not ended yet.
        let mut missing_attribute = None;
        let missing = if file {
            "A user attribute is expected here."
        } else {
            "A closing double angle bracket `>>` is expected here."
        };
        self.separated_list(
            T::Comma,
            ListKind::TrailingAllowed,
            |kind| kind == T::GtGt,
            missing,
            |p| {
                if !p.unit_aborted && !p.lowering_errors {
                    p.first_only_at = p.lowering_list.len();
                    missing_attribute = Some(p.lowering_list.len());
                    p.report_lowering(spec_start, spec_start, "attribute");
                }
            },
            |p| p.parse_attribute(attrs, file, &mut special),
        )?;

        self.c_list(attributes);
        if !self.at(T::GtGt) {
            self.require_missing(
                T::GtGt,
                "A closing double angle bracket `>>` is expected here.",
            );
        } else {
            self.bump();
        }
        self.end_lowering_abort(missing_attribute, self.prev_end);
        if special {
            self.check_special_attributes(attrs, spec_start);
            if !file {
                self.check_bare_simplihack(spec_start);
            }
        }
        Ok(())
    }

    /// One attribute of an attribute specification: the call of a
    /// constructor, whose class may be written as any expression.
    fn parse_attribute(
        &mut self,
        attrs: &mut BVec<'a, Attribute<'a>>,
        file: bool,
        special: &mut bool,
    ) -> R<()> {
        self.check_case()?;
        let mut is_type = !self.tok.mixed_case_soft;
        let mut name;
        let named = matches!(self.tok.kind, T::Name | T::Backslash);
        if named {
            (name, _) = self.parse_qualified_name()?;
        } else {
            // Not a name at all: an expression all the same.
            let span = self.parse_new_designator_span()?;
            name = Id {
                span,
                name: span.text(self.src),
            };
            is_type = false;
        }
        if named && !matches!(self.tok.kind, T::LParen | T::Comma | T::GtGt) {
            // Like the class of a `new`, the class of an attribute may
            // be any expression; the attribute is called what the
            // expression says.
            let span = self.parse_new_designator_after(name)?;
            name = Id {
                span,
                name: span.text(self.src),
            };
            is_type = false;
        }
        // An attribute is a constructor call; the class is only
        // wrapped as a type when there are arguments (and, as in `new`,
        // when it is not spelled like a keyword).
        let args = if self.at(T::LParen) {
            if is_type {
                self.c_node(K::SimpleTypeSpecifier);
            }
            let (args, unpacked) = self.parse_arguments()?;
            if unpacked.is_some() {
                return self.fail("unpacked attribute argument");
            }
            if args
                .iter()
                .any(|a| matches!(a.kind, ExprKind::InoutArg(_) | ExprKind::NamedArg(_)))
            {
                return self.fail("unsupported attribute argument");
            }
            args
        } else {
            self.c_missing_n(3);
            &[]
        };
        self.c_node(K::ConstructorCall);
        let attr = Attribute { name, args };
        if file {
            self.check_file_attribute(&attr);
        }
        if name.name.starts_with("__") {
            *special = true;
        }
        if name.name == "__SimpliHack" && !file {
            self.simplihack_attrs.push((name.span.start, self.prev_end));
        } else if !args.is_empty() {
            self.check_attribute_args(&attr);
        }
        attrs.push(attr);
        Ok(())
    }

    /// `__SimpliHack` is a gated feature: HHVM says so about an attribute
    /// list that has it without arguments.
    fn check_bare_simplihack(&mut self, spec_start: u32) {
        let end = self.prev_end.max(spec_start);
        let spec = &self.src[spec_start as usize..end as usize];
        let list = spec.strip_prefix("<<").unwrap_or(spec);
        let list = list.strip_suffix(">>").unwrap_or(list);
        if list.split(',').any(|attr| attr.trim() == "__SimpliHack") {
            self.require_feature(spec_start, end, Feature::SimpliHack);
        }
    }

    /// The other half of that gate: each `__SimpliHack` attribute of a
    /// function, method, class or type parameter. To be called once the
    /// attributes parsed last turn out to belong to one of those.
    pub(crate) fn check_simplihack_attrs(&mut self) {
        for (start, end) in std::mem::take(&mut self.simplihack_attrs) {
            self.require_feature(start, end, Feature::SimpliHack);
        }
    }

    fn parse_attributes_opt(&mut self) -> R<&'a [Attribute<'a>]> {
        if self.at(T::LtLt) {
            self.parse_attribute_spec()
        } else {
            self.c_missing();
            Ok(&[])
        }
    }

    /// `(params)`; the current token must be `(`.
    pub(crate) fn parse_params(&mut self, ctx: ParamCtx) -> R<&'a [Param<'a>]> {
        self.require(T::LParen);
        let mut params = self.vec();
        let mut spans: SmallVec<[(u32, u32); 8]> = SmallVec::new();
        let list_start = self.tok.start;
        let mut trailing_comma = None;
        let mut all_flags = 0u8;
        let parameters = self.c_mark();
        // The parameter before was missing (see `repair.rs`), in which
        // case the `)` does not end the list just yet.
        let mut after_gap = false;
        while !self.at(T::RParen) || after_gap {
            if matches!(self.tok.kind, T::Comma | T::Eof | T::RParen) {
                after_gap = self.missing_list_item("A right parenthesis `)` is expected here.");
                self.missing_syntax("function parameter");
                if after_gap {
                    continue;
                }
                break;
            }
            after_gap = false;
            let start = self.tok.start;
            let param = self.parse_param(ctx)?;
            spans.push((start, self.prev_end));
            all_flags |= param.flags;
            params.push(param);
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
        self.c_list(parameters);
        self.params_span = (list_start, self.prev_end);
        self.require(T::RParen);
        if all_flags & (param_flags::VARIADIC | param_flags::SPLAT) != 0 {
            self.check_param_list(&params, &spans, trailing_comma, false);
        }
        Ok(params.into_bump_slice())
    }

    fn parse_param(&mut self, ctx: ParamCtx) -> R<Param<'a>> {
        let start = self.tok.start;
        let attrs = self.parse_attributes_opt()?;
        let visibility = match self.tok.kind {
            T::Public => Some(Visibility::Public),
            T::Protected => Some(Visibility::Protected),
            T::Private => Some(Visibility::Private),
            T::Internal => Some(Visibility::Internal),
            _ => None,
        };
        if visibility.is_some() {
            self.bump();
        } else {
            self.c_missing();
        }
        let mut flags = 0u8;
        let mut inout = None;
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
                T::Optional => flags |= param_flags::OPTIONAL,
                T::Inout => inout = Some(Self::tok_span(tok)),
                T::Named => flags |= param_flags::NAMED,
                _ => {
                    flags |= param_flags::READONLY;
                    self.uses_readonly = true;
                }
            }
        }
        self.c_parameter_modifiers(modifiers);
        let ellipsis = self.c_mark();
        // The `...` that makes the parameter variadic.
        let mut variadic = self.tok;
        let leading_ellipsis = self.eat(T::DotDotDot);
        let mut hint = None;
        let untyped = matches!(self.tok.kind, T::Variable | T::DotDotDot)
            || (leading_ellipsis && matches!(self.tok.kind, T::Comma | T::RParen | T::Eq));
        if untyped {
            // Without a type, the `...` counts as the one after the type.
            if leading_ellipsis {
                flags |= param_flags::VARIADIC;
                self.c_insert_missing_n(ellipsis, 2);
            } else {
                self.c_missing_n(3);
            }
        } else {
            if !leading_ellipsis {
                self.c_missing();
            }
            let ty = self.parse_hint_ex(false)?;
            self.note_param_hint(&ty);
            hint = Some(ty);
            variadic = self.tok;
            if self.eat(T::DotDotDot) {
                flags |= param_flags::VARIADIC;
            } else {
                self.c_missing();
            }
            if leading_ellipsis {
                flags |= param_flags::SPLAT;
            }
        }
        let named_variadic = param_flags::NAMED | param_flags::VARIADIC;
        let name = if flags & named_variadic == named_variadic && !self.at(T::Variable) {
            // A variadic `named` parameter has no name (`named int...`);
            // the `...` stands in for it.
            self.c_missing();
            self.id(variadic)
        } else {
            self.expect_variable()?
        };
        let default = if self.at(T::Eq) {
            let eq = self.take();
            // The function's contexts are not known yet, so anything they
            // might forbid is looked at again later.
            let outer_strict = std::mem::replace(&mut self.strict_co, true);
            let in_literal = std::mem::replace(&mut self.in_expression_tree, false);
            let value = self.parse_initializer();
            self.in_expression_tree = in_literal;
            self.strict_co = outer_strict;
            let value = value?;
            self.c_node(K::SimpleInitializer);
            if inout.is_some() {
                self.report_from(
                    eq.start,
                    "An `inout` parameter must not have a default value.",
                );
            }
            Some(value)
        } else {
            self.c_missing();
            None
        };
        // Where the parameter ends: a child of its own, always empty.
        self.c_missing();
        self.c_node(K::ParameterDeclaration);
        if mod_count != 0 || default.is_some() || flags != 0 || name.name == "$this" {
            let param = (start, self.prev_end);
            if mod_count != 0 {
                self.check_param_modifiers(
                    &mods[..mod_count],
                    param,
                    (name.span.start, name.span.end),
                    false,
                );
            }
            self.check_param(ctx, param, name, inout, flags, default.as_ref(), attrs);
        }
        Ok(Param {
            name,
            hint,
            default,
            attrs,
            visibility,
            inout,
            flags,
        })
    }

    fn parse_modifiers(&mut self) -> Modifiers {
        let mut m = Modifiers::default();
        let modifiers = self.c_mark();
        loop {
            match self.tok.kind {
                T::Abstract => m.is_abstract = true,
                T::Final => m.is_final = true,
                T::Static => m.is_static = true,
                T::Async => m.is_async = true,
                T::Readonly => m.is_readonly = true,
                T::Public => m.visibility = Some(Visibility::Public),
                T::Private => {
                    m.has_private = true;
                    m.visibility = Some(Visibility::Private);
                }
                T::Protected => {
                    m.has_protected = true;
                    m.visibility = Some(Visibility::Protected);
                }
                T::Internal => {
                    m.has_internal = true;
                    m.visibility = Some(Visibility::Internal);
                }
                _ => break,
            }
            if (m.count as usize) < MAX_MODIFIERS {
                m.toks[m.count as usize] = (self.tok.kind, self.tok.start);
            }
            m.count += 1;
            self.bump();
        }
        self.c_list(modifiers);
        self.uses_readonly |= m.is_readonly;
        if m.has_protected && m.has_internal {
            m.visibility = Some(Visibility::ProtectedInternal);
        }
        m
    }

    // ------------------------------------------------------------------
    // Functions

    /// Parses from `function` through the body. Used for functions and
    /// methods; `start` is where the declaration began.
    fn parse_function_like(
        &mut self,
        start: u32,
        attrs: &'a [Attribute<'a>],
        m: &Modifiers,
        is_method: bool,
        doc_comment: Option<Span>,
    ) -> R<(Id<'a>, Fun<'a>)> {
        if !attrs.is_empty() {
            self.check_declaration_attrs(attrs);
        }
        let is_async = m.is_async;
        let header_start = match m.span() {
            Some((start, _)) => start,
            None => self.tok.start,
        };
        // Where the (absent) modifier list of a method would be.
        let modifiers_pos = if is_method && m.count == 0 {
            self.missing_pos()
        } else {
            header_start
        };
        self.require(T::Function);
        if !is_method {
            self.check_case()?;
        }
        let name = match self.tok.kind {
            T::Name => {
                let tok = self.take();
                self.id(tok)
            }
            T::Construct => {
                let tok = self.take();
                self.id(tok)
            }
            T::Isset | T::Unset | T::Echo | T::Empty => {
                let tok = self.take();
                self.c_name();
                self.id(tok)
            }
            kind if kind.is_keyword() && (is_method || !kind.is_reserved()) => {
                let tok = self.take();
                self.c_name();
                self.id(tok)
            }
            // `function (`: the name is missing.
            T::LParen => {
                if self.recovering {
                    // Nothing to salvage.
                    return self.fail("A name is expected here.");
                }
                self.c_missing();
                let name = self.missing_id();
                self.report_lowering(
                    name.span.start,
                    name.span.start + 1,
                    "Expected a method name",
                );
                name
            }
            _ => self.unexpected_function_name(),
        };
        let tparams_start = self.tok.start;
        let tparams = self.parse_type_params_opt()?;
        let class_tparams = match self.class {
            Some(class) if is_method => class.tparams,
            _ => &[],
        };
        self.check_reified_shadowing(tparams, class_tparams);
        let tparams_span = if tparams.is_empty() {
            (tparams_start, tparams_start)
        } else {
            (tparams_start, self.prev_end)
        };
        let params = self.parse_params(ParamCtx {
            is_async,
            is_lambda: false,
            is_construct: name.name == "__construct",
            memoized: !attrs.is_empty()
                && (has_attr(attrs, "__Memoize") || has_attr(attrs, "__MemoizeLSB")),
        })?;
        let params_span = self.params_span;
        let ctxs = self.parse_contexts_opt()?;
        let (ret, readonly_ret) = self.parse_return_hint_opt()?;
        let where_constraints = self.parse_where_opt()?;
        self.c_node(K::FunctionDeclarationHeader);
        let header = (header_start, self.prev_end);
        let semicolon = if self.at(T::Semicolon) {
            // A method without a body has its `;` after the absent body; for
            // a function the `;` is the body.
            if is_method {
                self.c_missing();
            }
            Some(self.tok.start)
        } else {
            None
        };
        let mut flags = 0u8;
        if is_async {
            flags |= fun_flags::ASYNC;
        }
        if readonly_ret {
            flags |= fun_flags::READONLY_RETURN;
            self.uses_readonly = true;
        }
        let mut trait_resolution = false;
        let body: Block<'a> = if self.eat(T::Semicolon) {
            flags |= fun_flags::NO_BODY;
            &[]
        } else if is_method && self.at(T::Eq) && !self.recovering {
            self.parse_trait_resolution(start)?;
            trait_resolution = true;
            flags |= fun_flags::NO_BODY;
            &[]
        } else if is_method && !self.at(T::LBrace) && !self.recovering {
            self.unexpected_method_body();
            flags |= fun_flags::NO_BODY;
            &[]
        } else {
            let saved_doc = self.stmt_doc.take();
            let errors_before = self.errors.len();
            let tree_errors_before = self.tree_errors.len();
            let lowering_before = self.lowering_list.len();
            let flagged_before = self.flagged.len();
            let outer = (self.fn_static, self.strict_co);
            self.fn_static = m.is_static;
            self.strict_co = ctxs.is_some();
            let r = if self.recovering {
                self.skip_function_body().map(|body| (body, false))
            } else {
                self.with_function_body(|p| p.parse_compound_statement())
            };
            (self.fn_static, self.strict_co) = outer;
            self.stmt_doc = saved_doc;
            let (body, has_yield) = r?;
            if self.decl_mode {
                self.errors.truncate(errors_before);
                self.tree_errors.truncate(tree_errors_before);
                self.lowering_list.truncate(lowering_before);
                self.flagged.truncate(flagged_before);
            }
            if has_yield {
                flags |= fun_flags::HAS_YIELD;
            }
            if is_method {
                self.c_missing();
            }
            body
        };
        if !trait_resolution {
            self.c_node(if is_method {
                K::MethodishDeclaration
            } else {
                K::FunctionDeclaration
            });
        }
        let had_abstract_method = self.class_has_abstract_method;
        self.check_function(&FunHeader {
            span: (start, self.prev_end),
            header,
            modifiers_pos,
            flags,
            m,
            attrs,
            name,
            tparams_span,
            params,
            params_span,
            ret,
            semicolon,
            is_method,
        });
        if trait_resolution {
            // No method as far as the class goes.
            self.class_has_abstract_method = had_abstract_method;
            // To HHVM's check for names declared twice it is a function,
            // which only clashes with a method in a namespace.
            if self.ns_name == "\\"
                && let Some(member) = self.class_members.last_mut()
                && member.kind == MemberName::METHOD
                && member.span == header
            {
                member.kind = MemberName::TRAIT_RESOLUTION;
            }
        }
        Ok((
            name,
            Fun {
                span: self.span_from(start),
                attrs,
                tparams,
                params,
                ret,
                ctxs,
                where_constraints,
                body,
                flags,
                doc_comment,
            },
        ))
    }

    /// `= Trait::method;` where the body of a method goes, the current
    /// token being the `=`: syntax HHVM's parser still reads (the member is
    /// a "trait resolution" to it), and which its translation to the AST
    /// knows nothing of. `start` is where the member starts.
    #[cold]
    fn parse_trait_resolution(&mut self, start: u32) -> R<()> {
        self.bump();
        let untranslated = self.lowering_state();
        self.retokenize_xhp_class_name();
        let kind = self.tok.kind;
        if matches!(kind, T::Name | T::Backslash) {
            // A name with type arguments, if any.
            self.parse_qualified_name()?;
            if self.at_type_lt() {
                self.parse_type_arguments()?;
                self.c_node(K::GenericTypeSpecifier);
            } else {
                self.c_node(K::SimpleTypeSpecifier);
            }
        } else if kind == T::XhpClassName {
            self.bump();
        } else if kind == T::Namespace || (kind.is_keyword() && !kind.is_reserved()) {
            let name = self.c_mark();
            self.parse_qualified_name()?;
            self.c_name_unless_reserved(name);
        } else {
            self.missing_name();
        }
        self.restore_lowering_state(untranslated);
        self.require(T::ColonColon);
        // The name of a method.
        if Self::is_plain_name(self.tok) || self.at(T::Construct) {
            self.bump();
        } else {
            let next = self.peek();
            if Self::is_plain_name(next) || next.kind == T::Construct {
                self.skip_unexpected_token();
                self.bump();
            } else {
                self.report_missing("A name is expected here.");
                self.c_missing();
            }
        }
        self.c_node(K::ScopeResolutionExpression);
        self.expect_semicolon()?;
        self.c_node(K::MethodishTraitResolution);
        if !self.unit_aborted && !self.lowering_errors {
            self.first_only_at = self.lowering_list.len();
            let text = &self.src[start as usize..self.prev_end as usize];
            self.report_lowering(
                start,
                self.prev_end,
                format!("Encountered unexpected text `{text}`, was expecting a class element."),
            );
        }
        Ok(())
    }

    pub(crate) fn parse_function_def(
        &mut self,
        first: Token,
        attrs: &'a [Attribute<'a>],
    ) -> R<Def<'a>> {
        let m = self.parse_modifiers();
        let doc = self.doc_before(first);
        let (name, fun) = self.parse_function_like(first.start, attrs, &m, false, doc)?;
        Ok(Def::Fun(self.alloc(FunDef {
            name,
            fun,
            internal: m.has_internal,
        })))
    }

    // ------------------------------------------------------------------
    // Classes

    /// `A, B<int>, C` after `extends`, `implements` or the `use` of an
    /// enum. HHVM reads types here for as long as a name starts one and a
    /// comma follows; anything else where a type is due is a missing type,
    /// which ends the list unless it is a comma.
    fn parse_hint_list(&mut self) -> R<&'a [Hint<'a>]> {
        let mut hints = self.vec();
        let types = self.c_mark();
        loop {
            self.retokenize_xhp_class_name();
            match self.tok.kind {
                T::Name | T::Backslash | T::Namespace | T::XhpClassName => {
                    hints.push(self.parse_hint()?);
                    if !self.eat_separator(T::Comma) {
                        break;
                    }
                }
                T::Comma => {
                    self.c_missing();
                    self.bump();
                    self.report_missing(TYPE_EXPECTED);
                    self.c_list_item(true);
                    self.missing_hint();
                }
                _ => {
                    self.report_missing(TYPE_EXPECTED);
                    self.c_missing();
                    self.c_list_item(false);
                    self.missing_hint();
                    break;
                }
            }
        }
        self.c_list(types);
        Ok(hints.into_bump_slice())
    }

    /// `A, B<int>` after the `use` of a class: names of traits, of which a
    /// name that does not start with a plain name token is not a type as
    /// far as the lossless tree is concerned.
    fn parse_trait_list(&mut self) -> R<&'a [Hint<'a>]> {
        let mut hints = self.vec();
        let types = self.c_mark();
        self.separated_list(
            T::Comma,
            ListKind::NoTrailing,
            |kind| matches!(kind, T::Semicolon | T::LBrace),
            "A name is expected here.",
            Self::missing_hint,
            |p| {
                let item = p.c_mark();
                if matches!(p.tok.kind, T::Name | T::Backslash) {
                    hints.push(p.parse_hint()?);
                } else if p.tok.kind == T::Namespace
                    || (p.tok.kind.is_keyword() && !p.tok.kind.is_reserved())
                    || p.lexer.is_xhp_class_name_at(p.tok.start as usize)
                {
                    hints.push(p.parse_hint()?);
                    p.c_bare_name(item);
                } else {
                    p.missing_name();
                }
                Ok(())
            },
        )?;
        self.c_list(types);
        Ok(hints.into_bump_slice())
    }

    pub(crate) fn parse_class(&mut self, first: Token, attrs: &'a [Attribute<'a>]) -> R<Def<'a>> {
        if !attrs.is_empty() {
            self.check_declaration_attrs(attrs);
        }
        let mut flags = 0u8;
        let attrs_end = self.prev_end;
        let mut m = Modifiers::default();
        // The modifiers are a list; `xhp`, which follows them, is not one.
        let modifiers = self.c_mark();
        loop {
            match self.tok.kind {
                T::Abstract => {
                    flags |= class_flags::ABSTRACT;
                    m.is_abstract = true;
                }
                T::Final => {
                    flags |= class_flags::FINAL;
                    m.is_final = true;
                }
                T::Internal => {
                    flags |= class_flags::INTERNAL;
                    m.has_internal = true;
                }
                T::Public => {}
                T::Xhp => {
                    // The last word before `class`.
                    self.c_list(modifiers);
                    flags |= class_flags::XHP_KEYWORD | class_flags::XHP;
                    self.bump();
                    if !self.options.enable_xhp_class_modifier {
                        self.report_missing("Encountered unexpected token `XHP`.");
                    }
                    break;
                }
                _ => break,
            }
            if (m.count as usize) < MAX_MODIFIERS {
                m.toks[m.count as usize] = (self.tok.kind, self.tok.start);
            }
            m.count += 1;
            self.bump();
        }
        if flags & class_flags::XHP_KEYWORD == 0 {
            self.c_list(modifiers);
            if self.at(T::Enum) {
                return self.parse_enum_after_modifiers(first, attrs, m);
            }
            self.c_missing();
        }
        let has_keyword = matches!(self.tok.kind, T::Class | T::Interface | T::Trait);
        let kind = match self.tok.kind {
            T::Interface => ClassKind::Interface,
            T::Trait => ClassKind::Trait,
            _ => ClassKind::Class,
        };
        if !has_keyword {
            self.missing_class_keyword();
        }
        let keyword_end = self.tok.end as usize;
        // The name is lexed specially for XHP classes, so look at the raw
        // source before letting the lexer produce the next token.
        let name = if flags & class_flags::XHP_KEYWORD != 0 {
            // `xhp class foo:bar` — colons but no dashes. Without the
            // keyword the name is whatever word stands in its place.
            let p = if has_keyword {
                self.skip_plain_trivia(keyword_end)
            } else {
                self.tok.start as usize
            };
            if p >= self.bytes.len() {
                return self.fail("expected XHP class name");
            }
            if has_keyword {
                // The keyword is still the current token.
                self.c_push_current();
            }
            // HHVM's lexer balks at a colon in front and takes anything
            // else for the first character of the name.
            let mut end = p;
            if self.lexer.at(p) == b':' {
                self.report_syntax(p as u32, p as u32, "A name is expected here.");
                self.c_token(TokenKind::ErrorToken, p as u32, p as u32);
            } else {
                loop {
                    end += 1;
                    while crate::lexer::is_name_char(self.lexer.at(end)) {
                        end += 1;
                    }
                    if self.lexer.at(end) == b':' && is_name_start(self.lexer.at(end + 1)) {
                        continue;
                    }
                    break;
                }
                self.c_token(TokenKind::XHPClassName, p as u32, end as u32);
            }
            self.resync(end);
            Id {
                span: Span::new(p as u32, end as u32),
                name: &self.src[p..end],
            }
        } else {
            if has_keyword {
                self.bump();
            }
            if self.retokenize_xhp_class_name() {
                flags |= class_flags::XHP;
                let tok = self.take();
                self.id(tok)
            } else {
                self.expect_name()?
            }
        };
        let tparams = self.parse_type_params_opt()?;
        self.check_reified_shadowing(tparams, &[]);
        if self.at(T::Eq) {
            let (has_attrs, has_modifiers) = (!attrs.is_empty(), m.count != 0);
            return self.parse_class_alias(first, has_attrs, has_modifiers, kind, name, tparams);
        }
        if kind != ClassKind::Interface {
            // The name is taken from here on, before anything in the body
            // mentions it.
            self.check_declared_type_name(name);
        }
        let mut extends_kw = None;
        let mut extends_span = (0, 0);
        let extends = if self.at(T::Extends) {
            extends_kw = Some((self.tok.start, self.tok.end));
            self.bump();
            let start = self.tok.start;
            let list = self.parse_hint_list()?;
            extends_span = (start, self.prev_end);
            list
        } else {
            self.c_missing_n(2);
            &[]
        };
        let has_implements = self.at(T::Implements);
        let implements = if self.eat(T::Implements) {
            self.parse_hint_list()?
        } else {
            self.c_missing_n(2);
            &[]
        };
        self.require(T::LBrace);
        self.class = Some(ClassInfo {
            kind,
            name: name.name,
            has_reified: tparams.iter().any(|t| t.reified),
            tparams,
            enum_class_attr: !attrs.is_empty() && has_attr(attrs, "__EnumClass"),
        });
        self.class_members.clear();
        self.class_requires.clear();
        self.class_has_abstract_method = false;
        self.ctor_promotion = None;
        let mut members = self.vec();
        let elements = self.c_mark();
        let scope = self.enter_scope(Expected::VISIBILITY);
        while !self.at(T::RBrace) && !self.at(T::Eof) {
            if self.recovering {
                let snap = self.snapshot();
                let member_start = self.tok.start as usize;
                let kept = members.len();
                let failed = self.parse_class_member(&mut members).is_err();
                if std::mem::take(&mut self.salvage_failed)
                    || failed
                    || self.tok.start as usize == member_start
                {
                    members.truncate(kept);
                    self.restore(snap);
                    self.resync_member(member_start);
                }
                continue;
            }
            let member_start = self.tok.start;
            self.parse_class_member(&mut members)?;
            if self.tok.start == member_start {
                // A member that consumed nothing (see `repair.rs`) ends
                // the list.
                break;
            }
        }
        self.leave_scope(scope);
        self.c_list(elements);
        if !(self.recovering && self.at(T::Eof)) {
            self.require(T::RBrace);
        }
        self.c_node(K::ClassishBody);
        self.c_node(K::ClassishDeclaration);
        let members = members.into_bump_slice();
        self.check_class(&ClassDecl {
            span: (first.start, self.prev_end),
            m: &m,
            has_xhp_keyword: flags & class_flags::XHP_KEYWORD != 0,
            kind,
            name,
            attrs,
            attrs_span: (first.start, attrs_end),
            extends_kw,
            extends_span,
            extends_count: extends.len(),
            has_implements,
            members,
        });
        self.class = None;
        Ok(Def::Class(self.alloc(Class {
            span: self.span_from(first.start),
            name,
            kind,
            flags,
            attrs,
            tparams,
            extends,
            implements,
            enum_base: None,
            enum_constraint: None,
            members,
            doc_comment: self.doc_before(first),
        })))
    }

    /// `= Original<T>;` of a class alias; everything before the `=` has
    /// been parsed as the start of a class declaration.
    fn parse_class_alias(
        &mut self,
        first: Token,
        has_attrs: bool,
        has_modifiers: bool,
        kind: ClassKind,
        name: Id<'a>,
        tparams: &'a [Tparam<'a>],
    ) -> R<Def<'a>> {
        self.require(T::Eq);
        // HHVM says what is wrong and reads the alias all the same.
        if has_attrs {
            self.report_missing("Attributes on class alias declarations are not allowed.");
        }
        if has_modifiers {
            self.report_missing("Modifiers on class alias declarations are not allowed.");
        }
        if kind == ClassKind::Trait {
            self.report_missing("Trait aliasing is not supported.");
        }
        let original = if self.retokenize_xhp_class_name() {
            let tok = self.take();
            self.id(tok)
        } else {
            self.expect_name()?
        };
        let original_tparams = self.parse_type_params_opt()?;
        self.expect_semicolon()?;
        self.c_node(K::ClassAliasDeclaration);
        self.require_feature(first.start, self.prev_end, Feature::ClassAliasesEverywhere);
        Ok(Def::ClassAlias(self.alloc(ClassAlias {
            name,
            tparams,
            original,
            original_tparams,
        })))
    }

    /// Skips whitespace only (no comments) from `pos`.
    fn skip_plain_trivia(&mut self, pos: usize) -> usize {
        self.lexer.pos = pos;
        self.lexer.skip_trivia();
        self.lexer.pos
    }

    fn parse_class_member(&mut self, members: &mut BVec<'a, Member<'a>>) -> R<()> {
        // HHVM translates each member on its own.
        self.end_unit();
        self.begin_unit();
        let r = self.parse_class_member_inner(members);
        if !self.pending_patterns.is_empty() {
            self.report_pending_patterns(0);
        }
        self.end_unit();
        r
    }

    fn parse_class_member_inner(&mut self, members: &mut BVec<'a, Member<'a>>) -> R<()> {
        self.check_case()?;
        let first = self.tok;
        match self.tok.kind {
            T::Children => self.parse_xhp_children(members),
            T::Category => self.parse_xhp_category(members),
            T::Use => {
                self.bump();
                let hints = self.parse_trait_list()?;
                self.expect_semicolon()?;
                self.c_node(K::TraitUse);
                members.push(Member::Use(hints));
                Ok(())
            }
            T::Require => {
                self.bump();
                let kind = match self.tok.kind {
                    T::Extends => RequireKind::Extends,
                    T::Implements => RequireKind::Implements,
                    T::Class => RequireKind::Class,
                    T::This => RequireKind::ThisAs,
                    _ => return self.fail("unsupported require clause"),
                };
                self.bump();
                if kind == RequireKind::ThisAs {
                    self.require(T::As);
                }
                // As for the traits of a `use`, except that an XHP class
                // name is a type here.
                let target = self.c_mark();
                let is_type = matches!(self.tok.kind, T::Name | T::Backslash)
                    || self.lexer.is_xhp_class_name_at(self.tok.start as usize);
                let hint = self.parse_hint()?;
                if !is_type {
                    self.c_bare_name(target);
                }
                // HHVM's parser takes a class here, type arguments and all.
                if kind == RequireKind::Class
                    && !matches!(hint.kind, HintKind::Name(_) | HintKind::Generic(_))
                {
                    return self.fail_at(hint.span.start, "invalid `require class` target");
                }
                self.expect_semicolon()?;
                if kind == RequireKind::ThisAs {
                    self.c_node(K::RequireClauseConstraint);
                    if !matches!(hint.kind, HintKind::Name(_)) {
                        self.report_lowering(
                            hint.span.start,
                            hint.span.end,
                            "Require this <: t cannot be used with a generic class",
                        );
                    }
                } else {
                    self.c_node(K::RequireClause);
                    self.check_require(first.start, kind, &hint);
                    if kind == RequireKind::Class && matches!(hint.kind, HintKind::Generic(_)) {
                        self.report_lowering(
                            hint.span.start,
                            hint.span.end,
                            "Require class cannot be used with a generic class",
                        );
                    }
                }
                members.push(Member::Require(self.alloc(Require { kind, hint })));
                Ok(())
            }
            T::Attribute => self.parse_xhp_attributes(members),
            T::Function => {
                let doc = self.doc_before(first);
                // Neither attributes nor modifiers.
                self.c_missing_n(2);
                let (name, fun) =
                    self.parse_function_like(first.start, &[], &Modifiers::default(), true, doc)?;
                members.push(Member::Method(self.alloc(Method {
                    name,
                    fun,
                    visibility: None,
                    flags: 0,
                })));
                Ok(())
            }
            T::Const
            | T::Abstract
            | T::Public
            | T::Protected
            | T::Private
            | T::Readonly
            | T::Internal
            | T::Static
            | T::Async
            | T::Final
            | T::LtLt => {
                let attrs = self.parse_attributes_opt()?;
                let after_attrs = self.tok;
                let m = self.parse_modifiers();
                if !matches!(self.tok.kind, T::Const | T::Function | T::LParen)
                    && matches!(self.peek_kind(), T::Function | T::Async)
                {
                    self.skip_token_before_function();
                }
                match self.tok.kind {
                    T::Const => self.parse_class_const(first, after_attrs, attrs, m, members),
                    T::Function => {
                        let doc = self.doc_before(first);
                        self.check_member_modifiers(&m);
                        let (name, fun) =
                            self.parse_function_like(first.start, attrs, &m, true, doc)?;
                        let mut fun = fun;
                        if m.is_readonly {
                            fun.flags |= fun_flags::READONLY_THIS;
                        }
                        let mut flags = 0u8;
                        if m.is_static {
                            flags |= method_flags::STATIC;
                        }
                        if m.is_abstract {
                            flags |= method_flags::ABSTRACT;
                        }
                        if m.is_final {
                            flags |= method_flags::FINAL;
                        }
                        members.push(Member::Method(self.alloc(Method {
                            name,
                            fun,
                            visibility: m.visibility,
                            flags,
                        })));
                        Ok(())
                    }
                    _ => self.parse_property(first, attrs, m, members),
                }
            }
            // A token the constructs around the class are waiting for
            // ends the members (see `repair.rs`).
            T::Class | T::Trait | T::Interface | T::Semicolon | T::RParen
                if self.expects(first.kind) =>
            {
                self.missing_class_member();
                Ok(())
            }
            _ => {
                // A property without modifiers (`int $x;`), if it parses as
                // one.
                let kept = members.len();
                let property = self.speculate(|p| {
                    p.c_missing_n(2);
                    p.parse_property(first, &[], Modifiers::default(), members)
                });
                if property.is_none() {
                    members.truncate(kept);
                    self.unexpected_class_member();
                }
                Ok(())
            }
        }
    }

    fn parse_property(
        &mut self,
        first: Token,
        attrs: &'a [Attribute<'a>],
        m: Modifiers,
        members: &mut BVec<'a, Member<'a>>,
    ) -> R<()> {
        let hint = if self.at(T::Variable) {
            self.c_missing();
            None
        } else {
            Some(self.parse_hint_ex(false)?)
        };
        let mut items = self.vec();
        let declarators = self.c_mark();
        loop {
            if matches!(self.tok.kind, T::Comma | T::Semicolon | T::Eof) {
                // No property where one is due.
                let goes_on = self.missing_list_item("A variable name is expected here.");
                self.missing_syntax("property declarator");
                if goes_on {
                    continue;
                }
                break;
            }
            let name = self.expect_variable()?;
            let value = if self.eat(T::Eq) {
                let value = self.parse_initializer()?;
                self.c_node(K::SimpleInitializer);
                self.check_constant_expr(&value, false);
                Some(value)
            } else {
                self.c_missing();
                None
            };
            self.c_node(K::PropertyDeclarator);
            items.push(PropItem {
                span: self.span_from(name.span.start),
                name,
                value,
            });
            if !self.eat_separator(T::Comma) {
                break;
            }
        }
        self.c_list(declarators);
        self.expect_semicolon()?;
        self.c_node(K::PropertyDeclaration);
        self.check_property(first.start, attrs, &m, &items);
        let mut flags = 0u8;
        if m.is_static {
            flags |= prop_flags::STATIC;
        }
        if m.is_abstract {
            flags |= prop_flags::ABSTRACT;
        }
        if m.is_final {
            flags |= prop_flags::FINAL;
        }
        if m.is_readonly {
            flags |= prop_flags::READONLY;
        }
        members.push(Member::Property(self.alloc(PropertyDecl {
            attrs,
            flags,
            visibility: m.visibility.unwrap_or(Visibility::Public),
            hint,
            items: items.into_bump_slice(),
            doc_comment: self.doc_before(first),
        })));
        Ok(())
    }

    /// `const ...;` inside a class. The current token is `const`.
    fn parse_class_const(
        &mut self,
        first: Token,
        after_attrs: Token,
        attrs: &'a [Attribute<'a>],
        m: Modifiers,
        members: &mut BVec<'a, Member<'a>>,
    ) -> R<()> {
        // The attributes and the modifiers have been recorded.
        let attributes = self.c_mark().wrapping_sub(2);
        self.require(T::Const);
        let doc_comment = self.doc_before(first);
        let next = self.peek_kind();
        if self.at(T::Type) && next != T::Semicolon && next != T::Eq {
            self.check_member_modifiers(&m);
            self.check_modifiers(&m, "Type constants", false, |k| k == T::Abstract);
            // `const type T = int;`
            self.bump();
            let name = self.require_name_non_reserved();
            let type_params_error = self.parse_const_type_params(first.start)?;
            let constraints = self.parse_bounds(false)?;
            let value = if self.eat(T::Eq) {
                Some(self.parse_hint()?)
            } else {
                self.c_missing_n(2);
                None
            };
            self.expect_semicolon()?;
            self.c_node(K::TypeConstDeclaration);
            self.end_lowering_error(type_params_error, self.prev_end);
            self.class_members.push(MemberName {
                name: name.name,
                kind: MemberName::CONST,
                span: (first.start, self.prev_end),
            });
            if !m.is_abstract && value.is_none() {
                self.const_without_value(first.start, "type");
                return Ok(());
            }
            members.push(Member::TypeConst(self.alloc(TypeConst {
                span: self.span_from(first.start),
                attrs,
                name,
                is_abstract: m.is_abstract,
                value: TypeConstValue::Type(value),
                constraints,
                doc_comment,
            })));
            return Ok(());
        }
        if self.at(T::Ctx) && next != T::Eq {
            // `const ctx C = [io];`
            self.bump();
            let name = self.require_name_non_reserved();
            let type_params_error = self.parse_const_type_params(after_attrs.start)?;
            // HHVM's node for a context constant has no place for
            // attributes.
            self.c_remove(attributes);
            let mut constraints = self.vec();
            let constraints_start = self.tok.start;
            let constraint_list = self.c_mark();
            loop {
                let kind = match self.tok.kind {
                    T::As => ConstraintKind::As,
                    T::Super => ConstraintKind::Super,
                    _ => break,
                };
                self.bump();
                let Some(ctxs) = self.parse_contexts_opt()? else {
                    return self.fail("expected context list");
                };
                self.c_node(K::ContextConstraint);
                constraints.push((
                    kind,
                    Hint {
                        span: ctxs.span,
                        kind: HintKind::Intersection(ctxs.hints),
                    },
                ));
            }
            self.c_list(constraint_list);
            let constraints_end = self.prev_end;
            let value = if self.eat(T::Eq) {
                let Some(ctxs) = self.parse_contexts_opt()? else {
                    return self.fail("expected context list");
                };
                Some(ctxs)
            } else {
                self.c_missing_n(2);
                None
            };
            self.expect_semicolon()?;
            self.c_node(K::ContextConstDeclaration);
            self.end_lowering_error(type_params_error, self.prev_end);
            let constraints = constraints.into_bump_slice();
            for (kind, keyword) in [(ConstraintKind::Super, "super"), (ConstraintKind::As, "as")] {
                if constraints.iter().filter(|(k, _)| *k == kind).count() > 1 {
                    self.report_lowering(
                        constraints_start,
                        constraints_end,
                        format!(
                            "Multiple `{keyword}` constraints on a ctx constant are not allowed"
                        ),
                    );
                }
            }
            // HHVM's node for a context constant starts after any
            // attributes, which it does not keep.
            self.class_members.push(MemberName {
                name: name.name,
                kind: MemberName::CONST,
                span: (after_attrs.start, self.prev_end),
            });
            if !m.is_abstract {
                if value.is_none() {
                    self.const_without_value(after_attrs.start, "context");
                    return Ok(());
                }
                if !constraints.is_empty() {
                    self.report_lowering(
                        after_attrs.start,
                        self.prev_end,
                        "Constraints on a context constant requires it to be abstract",
                    );
                }
            }
            members.push(Member::TypeConst(self.alloc(TypeConst {
                span: self.span_from(after_attrs.start),
                attrs: &[],
                name,
                is_abstract: m.is_abstract,
                value: TypeConstValue::Ctx(value),
                constraints,
                doc_comment: self.doc_before(after_attrs),
            })));
            return Ok(());
        }
        self.check_member_modifiers(&m);
        self.check_modifiers(&m, "Constants", false, |k| k == T::Abstract);
        let (hint, items) = self.parse_const_declarators(m.is_abstract)?;
        self.c_node(K::ConstDeclaration);
        for item in items {
            self.class_members.push(MemberName {
                name: item.name.name,
                kind: MemberName::CONST,
                span: (first.start, self.prev_end),
            });
        }
        members.push(Member::Const(self.alloc(ClassConstDecl {
            span: self.span_from(first.start),
            attrs,
            hint,
            is_abstract: m.is_abstract,
            items,
            doc_comment,
        })));
        Ok(())
    }

    /// The type parameters HHVM's parser takes after the name of a type or
    /// context constant. Its translation to the AST does not read them: it
    /// says, about the whole member (which starts at `start`), that they
    /// should not be there. The result goes to
    /// [`end_lowering_error`](Self::end_lowering_error) when the member
    /// has ended.
    fn parse_const_type_params(&mut self, start: u32) -> R<Option<usize>> {
        if !self.at_type_lt() {
            self.c_missing();
            return Ok(None);
        }
        let untranslated = self.lowering_state();
        self.parse_type_params_opt()?;
        self.restore_lowering_state(untranslated);
        Ok(self.begin_lowering_error(
            start,
            "Type parameters are not allowed on class type constants",
        ))
    }

    /// A type or context constant (`kind`) that is not abstract and has no
    /// value, which started at `start`. HHVM's translation to the AST
    /// leaves it out, and says why unless it has complained about
    /// something already.
    #[cold]
    fn const_without_value(&mut self, start: u32, kind: &str) {
        if !self.unit_aborted && !self.lowering_errors {
            self.first_only_at = self.lowering_list.len();
            let text = &self.src[start as usize..self.prev_end as usize];
            self.report_lowering(
                start,
                self.prev_end,
                format!(
                    "Encountered unexpected text `{text}`, was expecting a value for the {kind} constant."
                ),
            );
        }
    }

    /// `[type] NAME = value, ...;` after `const`.
    fn parse_const_declarators(
        &mut self,
        allow_missing_value: bool,
    ) -> R<(Option<Hint<'a>>, &'a [ConstItem<'a>])> {
        // A type is present when a type followed by a name parses.
        let hint = self.speculate(|p| {
            let hint = p.parse_hint()?;
            if !p.tok.kind.is_name_like() {
                return p.fail("no type");
            }
            Ok(hint)
        });
        if hint.is_none() {
            self.c_missing();
        }
        let mut items = self.vec();
        let declarators = self.c_mark();
        // Only a `class` reports a constant declared twice at the second
        // declarator (in addition to the report on the whole member).
        let in_class = matches!(self.class, Some(c) if c.kind == ClassKind::Class);
        loop {
            let name = self.expect_name_allow_keywords()?;
            let value = if self.eat(T::Eq) {
                let value = self.parse_initializer()?;
                self.c_node(K::SimpleInitializer);
                self.check_constant_expr(&value, false);
                if allow_missing_value {
                    self.require_feature(
                        name.span.start,
                        self.prev_end,
                        Feature::ClassConstDefault,
                    );
                }
                Some(value)
            } else {
                // A constant without a value is only legal when abstract or
                // in a declaration (`.hhi`) file.
                if !allow_missing_value && !self.hhi {
                    let at = self.missing_pos();
                    self.report(
                        at,
                        at,
                        "A concrete constant declaration must have an initializer.",
                    );
                }
                if !allow_missing_value && !self.decl_mode {
                    self.missing_initializer();
                }
                self.c_missing();
                None
            };
            self.c_node(K::ConstantDeclarator);
            if in_class {
                self.class_members.push(MemberName {
                    name: name.name,
                    kind: MemberName::CONST_ITEM,
                    span: (name.span.start, self.prev_end),
                });
            }
            items.push(ConstItem { name, value });
            if !self.eat_separator(T::Comma) {
                break;
            }
        }
        self.c_list(declarators);
        self.expect_semicolon()?;
        Ok((hint, items.into_bump_slice()))
    }

    // ------------------------------------------------------------------
    // XHP class members

    /// `attribute int foo = 1 @required, :other:element;`
    fn parse_xhp_attributes(&mut self, members: &mut BVec<'a, Member<'a>>) -> R<()> {
        let start = self.expect(T::Attribute)?.start;
        let mut decls = self.vec();
        let attributes = self.c_mark();
        loop {
            if matches!(self.tok.kind, T::Comma | T::Semicolon | T::Eof) {
                // No attribute where one is due.
                let goes_on = self.missing_list_item("A name is expected here.");
                self.missing_syntax("XHP attribute");
                if goes_on {
                    continue;
                }
                break;
            }
            if self.retokenize_xhp_class_name()
                && matches!(self.peek_kind(), T::Comma | T::Semicolon)
            {
                // Inherit the attributes of another element.
                let tok = self.take();
                self.c_node(K::SimpleTypeSpecifier);
                self.c_node(K::XHPSimpleClassAttribute);
                decls.push(XhpAttrDecl::Use(Hint {
                    span: Self::tok_span(tok),
                    kind: HintKind::Name(self.tok_text(tok)),
                }));
            } else {
                let ty = if self.at(T::Enum) || (self.at(T::Tilde) && self.peek_kind() == T::Enum) {
                    let ty_start = self.tok.start;
                    let like = if self.at(T::Tilde) {
                        Some(Self::tok_span(self.take()))
                    } else {
                        self.c_missing();
                        None
                    };
                    self.require(T::Enum);
                    self.require(T::LBrace);
                    let mut values = self.vec();
                    let value_list = self.c_mark();
                    while !self.at(T::RBrace) {
                        values.push(self.parse_expr()?);
                        if !self.eat_separator(T::Comma) {
                            break;
                        }
                    }
                    self.c_list(value_list);
                    self.check_xhp_enum_values(&values);
                    // The attribute name follows the `}` and is not an
                    // ordinary token, so do not let the lexer look at it.
                    if !self.at(T::RBrace) {
                        return self.fail("A right brace `}` is expected here.");
                    }
                    self.c_push_current();
                    self.c_node(K::XHPEnumType);
                    let end = self.tok.end as usize;
                    self.prev_end = end as u32;
                    let ty = XhpAttrType::Enum {
                        span: Span::new(ty_start, end as u32),
                        like,
                        values: values.into_bump_slice(),
                    };
                    self.xhp_attr_name_at(end).map(|name| (ty, name))?
                } else {
                    // Parse the type, then re-read whatever follows it as an
                    // XHP attribute name (`data-foo`, `aria:label`).
                    let type_start = self.tok.start;
                    let (hint, end) = self.parse_hint_then_stop()?;
                    if let HintKind::Access(..) = hint.kind
                        && self.features & Feature::XhpTypeConstants.bit() == 0
                    {
                        self.report_lowering(
                            type_start,
                            end as u32,
                            "Type constants are not allowed on xhp class attributes",
                        );
                    }
                    self.xhp_attr_name_at(end)
                        .map(|name| (XhpAttrType::Hint(hint), name))?
                };
                let (ty, name) = ty;
                let (default, default_span) = if self.at(T::Eq) {
                    let eq = self.take();
                    let term = self.parse_expr_term()?;
                    self.c_node(K::SimpleInitializer);
                    self.check_constant_expr(&term.e, true);
                    (Some(term.e), Span::new(eq.start, term.end))
                } else {
                    self.c_missing();
                    (None, Span::NONE)
                };
                let tag = if self.at(T::At) {
                    let tag = match self.peek_kind() {
                        T::Required => Some(XhpAttrTag::Required),
                        T::Lateinit => Some(XhpAttrTag::LateInit),
                        _ => None,
                    };
                    match tag {
                        Some(tag) => {
                            self.bump();
                            self.bump();
                            self.c_node(match tag {
                                XhpAttrTag::Required => K::XHPRequired,
                                XhpAttrTag::LateInit => K::XHPLateinit,
                            });
                        }
                        None => {
                            // HHVM reads the `@` and whatever follows it,
                            // and keeps neither.
                            self.drop_token();
                            self.drop_token();
                            self.report_missing(
                                "The `required` or `lateinit` keyword is expected here.",
                            );
                            self.c_missing();
                        }
                    }
                    tag
                } else {
                    self.c_missing();
                    None
                };
                self.c_node(K::XHPClassAttribute);
                decls.push(XhpAttrDecl::Attr(self.alloc(XhpAttr {
                    ty,
                    name,
                    default,
                    default_span,
                    tag,
                })));
            }
            if !self.eat_separator(T::Comma) {
                break;
            }
        }
        self.c_list(attributes);
        self.expect_semicolon()?;
        self.c_node(K::XHPClassAttributeDeclaration);
        for decl in decls.iter() {
            if let XhpAttrDecl::Attr(attr) = decl {
                self.class_members.push(MemberName {
                    name: attr.name.name,
                    kind: MemberName::XHP_ATTRIBUTE,
                    span: (start, self.prev_end),
                });
            }
        }
        members.push(Member::XhpAttributes(decls.into_bump_slice()));
        Ok(())
    }

    /// Parses a type and returns it with the position where it ended,
    /// without committing to how the following token is lexed.
    fn parse_hint_then_stop(&mut self) -> R<(Hint<'a>, usize)> {
        let hint = if self.at(T::Var) {
            // Only here is `var` a type.
            let tok = self.take();
            self.c_node(K::SimpleTypeSpecifier);
            Hint {
                span: Self::tok_span(tok),
                kind: HintKind::Name(self.tok_text(tok)),
            }
        } else {
            self.parse_hint_ex(false)?
        };
        Ok((hint, self.prev_end as usize))
    }

    /// Reads an XHP attribute name starting after `pos` and resumes normal
    /// lexing after it.
    fn xhp_attr_name_at(&mut self, pos: usize) -> R<Id<'a>> {
        let p = self.skip_plain_trivia(pos);
        if !is_name_start(self.lexer.at(p)) {
            // Whatever stands there instead is read as a token.
            self.resync(pos);
            return Ok(self.missing_name());
        }
        let end = self.lexer.xhp_element_name_end(p, false);
        self.c_token(TokenKind::XHPElementName, p as u32, end as u32);
        self.resync(end);
        Ok(Id {
            span: Span::new(p as u32, end as u32),
            name: &self.src[p..end],
        })
    }

    /// `category %foo, %bar;`
    fn parse_xhp_category(&mut self, members: &mut BVec<'a, Member<'a>>) -> R<()> {
        let start = self.tok.start;
        let mut pos = self.tok.end as usize;
        let mut categories = self.vec();
        // The names are scanned from the source; the keyword and the commas
        // between them are current tokens that are never consumed.
        self.c_push_current();
        let category_list = self.c_mark();
        loop {
            let p = self.skip_plain_trivia(pos);
            if self.lexer.at(p) != b'%' || !is_name_start(self.lexer.at(p + 1)) {
                return self.fail_at(p as u32, "expected XHP category");
            }
            let end = self.lexer.xhp_category_name_end(p);
            categories.push(Id {
                span: Span::new(p as u32, end as u32),
                name: &self.src[p..end],
            });
            self.c_token(TokenKind::XHPCategoryName, p as u32, end as u32);
            self.resync(end);
            if self.at(T::Comma) {
                pos = self.tok.end as usize;
                self.c_push_current();
                self.c_list_item(true);
                continue;
            }
            self.c_list_item(false);
            break;
        }
        self.c_list(category_list);
        self.expect_semicolon()?;
        self.c_node(K::XHPCategoryDeclaration);
        members.push(Member::XhpCategory(self.alloc(XhpCategoryDecl {
            span: self.span_from(start),
            categories: categories.into_bump_slice(),
        })));
        Ok(())
    }

    /// `children (a, b*, (c | d)+);`
    fn parse_xhp_children(&mut self, members: &mut BVec<'a, Member<'a>>) -> R<()> {
        let start = self.tok.start;
        let pos = self.tok.end as usize;
        // The expression is scanned from the source; the keyword is a
        // current token that is never consumed.
        self.c_push_current();
        if self.options.disable_xhp_children_declarations {
            let (start, end) = self.missing_span_after(start, pos as u32);
            self.report_syntax(
                start,
                end,
                "XHP children declarations are no longer supported.",
            );
        }
        let empty = self.peek();
        let (child, end) = if empty.kind == T::Empty && !self.options.enable_xhp_class_modifier {
            // Where names are written the `xhp class` way this is read as
            // one of them first.
            self.c_token(TokenKind::Empty, empty.start, empty.end);
            (XhpChild::Name(self.id(empty)), empty.end as usize)
        } else {
            self.parse_xhp_child_expr(pos)?
        };
        if matches!(child, XhpChild::Name(id) if id.name == "empty") {
            self.c_retag(TokenKind::Empty);
        }
        self.resync(end);
        self.expect_semicolon()?;
        self.c_node(K::XHPChildrenDeclaration);
        members.push(Member::XhpChildren(self.alloc(XhpChildrenDecl {
            span: self.span_from(start),
            child,
        })));
        Ok(())
    }

    /// One `|`-separated alternation of children terms starting after `pos`.
    fn parse_xhp_child_expr(&mut self, pos: usize) -> R<(XhpChild<'a>, usize)> {
        let (mut left, mut p) = self.parse_xhp_child_term(pos)?;
        loop {
            let q = self.skip_plain_trivia(p);
            if self.lexer.at(q) == b'|' {
                self.c_token(TokenKind::Bar, q as u32, q as u32 + 1);
                let (right, after) = self.parse_xhp_child_term(q + 1)?;
                self.c_node(K::BinaryExpression);
                left = XhpChild::Binary(self.alloc(left), self.alloc(right));
                p = after;
            } else {
                return Ok((left, p));
            }
        }
    }

    fn parse_xhp_child_term(&mut self, pos: usize) -> R<(XhpChild<'a>, usize)> {
        self.enter()?;
        let p = self.skip_plain_trivia(pos);
        let c = self.lexer.at(p);
        let (mut child, mut end) = if c == b'(' {
            let mut items = self.vec();
            self.c_token(TokenKind::LeftParen, p as u32, p as u32 + 1);
            let children = self.c_mark();
            // The items are scanned from the source, the commas and the
            // parentheses read as tokens.
            self.resync(p + 1);
            (self.prev_start, self.prev_end) = (p as u32, p as u32 + 1);
            self.separated_list(
                T::Comma,
                ListKind::NoTrailing,
                |kind| kind == T::RParen,
                "A right parenthesis `)` is expected here.",
                |_| {},
                |parser| {
                    let (item, after) = parser.parse_xhp_child_expr(parser.prev_end as usize)?;
                    items.push(item);
                    parser.resync(after);
                    Ok(())
                },
            )?;
            self.c_list(children);
            self.require(T::RParen);
            self.c_node(K::XHPChildrenParenthesizedList);
            (
                XhpChild::List(items.into_bump_slice()),
                self.prev_end as usize,
            )
        } else {
            let colon_name = c == b':' && is_name_start(self.lexer.at(p + 1));
            let name = if c == b'%' && is_name_start(self.lexer.at(p + 1)) {
                Some((
                    TokenKind::XHPCategoryName,
                    self.lexer.xhp_category_name_end(p),
                ))
            } else if self.options.enable_xhp_class_modifier && is_name_start(c) {
                // `xhp class` style names may contain colons.
                let mut e = p;
                loop {
                    e += 1;
                    while crate::lexer::is_name_char(self.lexer.at(e)) {
                        e += 1;
                    }
                    if self.lexer.at(e) == b':' && is_name_start(self.lexer.at(e + 1)) {
                        continue;
                    }
                    break;
                }
                Some((TokenKind::XHPClassName, e))
            } else if !self.options.enable_xhp_class_modifier && colon_name {
                let end = self.lexer.xhp_element_name_end(p, false);
                Some((TokenKind::XHPClassName, end))
            } else {
                // Any other token: fine if it is a plain name where names
                // are not written the `xhp class` way.
                self.resync(pos);
                self.check_case()?;
                (!self.options.enable_xhp_class_modifier && self.at(T::Name))
                    .then_some((TokenKind::Name, self.tok.end as usize))
            };
            let Some((kind, name_end)) = name else {
                // HHVM takes the token for the name without reading past
                // it, so what follows starts with the same token.
                let (start, end) = self.missing_span_after(pos as u32, pos as u32);
                self.report_syntax(start, end, "An XHP name or category name is expected here.");
                let tok = self.tok;
                let read = self.cst.as_ref().map(|cst| cst.full_end);
                self.c_push_current();
                if let (Some(cst), Some(read)) = (&mut self.cst, read) {
                    cst.full_end = read;
                }
                self.leave();
                return Ok((XhpChild::Name(self.id(tok)), pos));
            };
            self.c_token(kind, p as u32, name_end as u32);
            (
                XhpChild::Name(Id {
                    span: Span::new(p as u32, name_end as u32),
                    name: &self.src[p..name_end],
                }),
                name_end,
            )
        };
        let q = self.skip_plain_trivia(end);
        let op = match self.lexer.at(q) {
            b'*' => Some(XhpChildOp::Star),
            b'+' => Some(XhpChildOp::Plus),
            b'?' => Some(XhpChildOp::Question),
            _ => None,
        };
        if let Some(op) = op {
            self.c_token(
                match op {
                    XhpChildOp::Star => TokenKind::Star,
                    XhpChildOp::Plus => TokenKind::Plus,
                    XhpChildOp::Question => TokenKind::Question,
                },
                q as u32,
                q as u32 + 1,
            );
            self.c_node(K::PostfixUnaryExpression);
            child = XhpChild::Unary(self.alloc(child), op);
            end = q + 1;
        }
        self.leave();
        Ok((child, end))
    }

    // ------------------------------------------------------------------
    // Enums

    fn parse_enum(&mut self, first: Token, attrs: &'a [Attribute<'a>]) -> R<Def<'a>> {
        if !attrs.is_empty() {
            self.check_declaration_attrs(attrs);
        }
        let m = self.parse_modifiers();
        self.parse_enum_after_modifiers(first, attrs, m)
    }

    /// An enum or enum class from the `enum` keyword on.
    fn parse_enum_after_modifiers(
        &mut self,
        first: Token,
        attrs: &'a [Attribute<'a>],
        m: Modifiers,
    ) -> R<Def<'a>> {
        self.require(T::Enum);
        let doc_comment = self.doc_before(first);
        let mut flags = 0u8;
        if m.has_internal {
            flags |= class_flags::INTERNAL;
        }
        if self.at(T::Class) {
            self.bump();
            if m.is_abstract {
                flags |= class_flags::ABSTRACT;
            }
            let name = self.require_plain_name();
            self.check_type_name(name);
            self.check_modifiers(&m, "Enum classes", true, |k| {
                matches!(k, T::Abstract | T::Internal | T::Public)
            });
            self.require(T::Colon);
            let base = self.parse_hint()?;
            let extends = if self.eat(T::Extends) {
                self.parse_hint_list()?
            } else {
                self.c_missing_n(2);
                &[]
            };
            self.require(T::LBrace);
            let mut members = self.vec();
            let elements = self.c_mark();
            while !self.at(T::RBrace) && !self.at(T::Eof) {
                let member_first = self.tok;
                let member = self.c_mark();
                // Attributes may come first. HHVM then looks at the next
                // tokens to tell a type constant, which may have them,
                // from a constant of the enum class, which may not.
                let before_attrs = (self.errors.len(), self.lowering_state());
                let has_attrs = self.at(T::LtLt);
                let attrs = self.parse_attributes_opt()?;
                let after_attrs = self.tok;
                let next = self.peek_kind();
                let type_constant = match self.tok.kind {
                    T::Const => next == T::Type,
                    T::Abstract if next == T::Const => {
                        let snap = self.snapshot();
                        self.bump();
                        let found = self.peek_kind() == T::Type;
                        self.restore(snap);
                        found
                    }
                    _ => false,
                };
                if type_constant {
                    let mm = self.parse_modifiers();
                    let mut tmp = self.vec();
                    self.parse_class_const(member_first, after_attrs, attrs, mm, &mut tmp)?;
                    match tmp.first() {
                        Some(Member::TypeConst(_)) => members.push(tmp[0]),
                        // HHVM left it out.
                        None => {}
                        _ => return self.fail_at(member_first.start, "invalid enum class member"),
                    }
                    if self.tok.start == member_first.start {
                        break;
                    }
                    continue;
                }
                // HHVM says that the attributes should not be there and
                // forgets them.
                if has_attrs {
                    self.report_missing("Attributes on enum class enumerators are not allowed");
                    self.errors.truncate(before_attrs.0);
                    self.restore_lowering_state(before_attrs.1);
                }
                self.c_remove(member);
                let mm = self.parse_modifiers();
                let hint = self.parse_hint_ex(false)?;
                let name = self.expect_name_allow_keywords()?;
                let value = if self.eat(T::Eq) {
                    let outer_strict = std::mem::replace(&mut self.strict_co, true);
                    let value = self.parse_initializer();
                    self.strict_co = outer_strict;
                    let value = value?;
                    self.c_node(K::SimpleInitializer);
                    Some(value)
                } else {
                    if !mm.is_abstract {
                        self.missing_initializer();
                    }
                    self.c_missing();
                    None
                };
                self.expect_semicolon()?;
                self.c_node(K::EnumClassEnumerator);
                self.check_enum_class_member(member_first.start, &mm, name, value.is_some());
                members.push(Member::EnumClassMember(self.alloc(EnumClassMember {
                    name,
                    hint,
                    value,
                    is_abstract: mm.is_abstract,
                })));
                if self.tok.start == member_first.start {
                    break;
                }
            }
            self.c_list(elements);
            self.require(T::RBrace);
            self.c_node(K::EnumClassDeclaration);
            return Ok(Def::Class(self.alloc(Class {
                span: self.span_from(first.start),
                name,
                kind: ClassKind::EnumClass,
                flags,
                attrs,
                tparams: &[],
                extends,
                implements: &[],
                enum_base: Some(base),
                enum_constraint: None,
                members: members.into_bump_slice(),
                doc_comment,
            })));
        }
        let name = self.require_plain_name();
        self.check_declared_type_name(name);
        self.check_type_name(name);
        self.check_modifiers(&m, "Enums", true, |k| matches!(k, T::Internal | T::Public));
        let base = if self.at(T::LBrace) {
            self.missing_enum_base(name)
        } else {
            self.require(T::Colon);
            self.parse_hint()?
        };
        let constraint = if self.eat(T::As) {
            let constraint = self.parse_hint()?;
            self.c_node(K::TypeConstraint);
            Some(constraint)
        } else {
            self.c_missing();
            None
        };
        self.require(T::LBrace);
        let mut members = self.vec();
        let use_clauses = self.c_mark();
        while self.at(T::Use) {
            self.bump();
            let hints = self.parse_hint_list()?;
            self.expect_semicolon()?;
            self.c_node(K::EnumUse);
            members.push(Member::Use(hints));
        }
        self.c_list(use_clauses);
        let enumerators = self.c_mark();
        while !self.at(T::RBrace) && !self.at(T::Eof) {
            let name_tok = self.tok;
            let name = self.expect_name_allow_keywords()?;
            self.require(T::Eq);
            let outer_strict = std::mem::replace(&mut self.strict_co, true);
            let value = self.parse_initializer();
            self.strict_co = outer_strict;
            let value = value?;
            self.check_constant_expr(&value, false);
            self.expect_semicolon()?;
            self.c_node(K::Enumerator);
            if name.name.eq_ignore_ascii_case("class") {
                self.report_from(name_tok.start, "Enum element cannot be named `class`");
            }
            members.push(Member::Enumerator(self.alloc(Enumerator {
                name,
                value,
                doc_comment: self.doc_before(name_tok),
            })));
            if self.tok.start == name_tok.start {
                break;
            }
        }
        self.c_list(enumerators);
        self.require(T::RBrace);
        self.c_node(K::EnumDeclaration);
        if !attrs.is_empty() && has_attr(attrs, "__Const") {
            self.report_from(
                first.start,
                "Classes, interfaces, traits and enums may not be declared `__Const`",
            );
        }
        Ok(Def::Class(self.alloc(Class {
            span: self.span_from(first.start),
            name,
            kind: ClassKind::Enum,
            flags,
            attrs,
            tparams: &[],
            extends: &[],
            implements: &[],
            enum_base: Some(base),
            enum_constraint: constraint,
            members: members.into_bump_slice(),
            doc_comment,
        })))
    }

    // ------------------------------------------------------------------
    // Aliases, constants, `use`

    fn parse_type_alias(&mut self, first: Token, attrs: &'a [Attribute<'a>]) -> R<Def<'a>> {
        if !attrs.is_empty() {
            self.check_declaration_attrs(attrs);
        }
        let m = self.parse_modifiers();
        if self.at(T::Case) {
            return self.parse_case_type(first, attrs, m);
        }
        let module_newtype = self.at(T::Module);
        if module_newtype {
            if !self.in_module {
                self.report(
                    self.tok.start,
                    self.tok.end,
                    "`module newtype` type aliases cannot be used outside of a module. Please define a module for this file.",
                );
            }
            self.bump();
        } else {
            self.c_missing();
        }
        let kind = match self.tok.kind {
            T::Type if !module_newtype => TypedefKind::Transparent,
            T::Newtype if module_newtype => TypedefKind::OpaqueModule,
            T::Newtype => TypedefKind::Opaque,
            _ => return self.fail("expected `type` or `newtype`"),
        };
        let keyword = self.take();
        self.check_modifiers(&m, "Type aliases", true, |k| {
            matches!(k, T::Internal | T::Public)
        });
        let name = self.require_name_non_reserved();
        let tparams = self.parse_type_params_opt()?;
        let constraints = self.parse_bounds(true)?;
        self.require(T::Eq);
        let hint = self.parse_hint()?;
        self.expect_semicolon()?;
        self.c_node(K::AliasDeclaration);
        for tparam in tparams {
            if tparam.reified {
                self.report_lowering(first.start, self.prev_end, REIFY_MISPLACED);
            }
            if kind == TypedefKind::Transparent && !tparam.constraints.is_empty() {
                self.report_lowering(
                    first.start,
                    self.prev_end,
                    "A type alias declaration cannot both use `type` and have a constraint on its type parameters. Did you mean `newtype`?",
                );
            }
        }
        if !constraints.is_empty() {
            self.check_alias_bounds(keyword, constraints);
        }
        if let HintKind::Access(_) = hint.kind {
            self.report(
                hint.span.start,
                hint.span.end,
                "Type aliases to type constants are not supported",
            );
        }
        self.check_type_name(name);
        self.check_declared_type_name(name);
        Ok(Def::Typedef(self.alloc(Typedef {
            span: self.span_from(first.start),
            name,
            kind,
            attrs,
            tparams,
            constraints,
            hint,
            variants: &[],
            internal: m.has_internal,
            doc_comment: self.doc_before(first),
        })))
    }

    /// `case type Name as Bound = A | B;`
    fn parse_case_type(
        &mut self,
        first: Token,
        attrs: &'a [Attribute<'a>],
        m: Modifiers,
    ) -> R<Def<'a>> {
        self.require(T::Case);
        self.require(T::Type);
        let name = self.require_plain_name();
        self.check_modifiers(&m, "Case types", true, |k| {
            matches!(k, T::Internal | T::Public)
        });
        self.check_type_name(name);
        self.check_declared_type_name(name);
        let tparams = self.parse_type_params_opt()?;
        // HHVM's translation to the AST takes the bounds and the variants
        // one by one: a type it cannot translate is reported (if nothing
        // has been yet) and left out, and the declaration goes on.
        let unit_aborted = self.unit_aborted;
        let mut constraints = self.vec();
        if self.eat(T::As) {
            let bounds = self.c_mark();
            self.separated_list(
                T::Comma,
                ListKind::NoTrailing,
                |kind| kind == T::Eq,
                TYPE_EXPECTED,
                |p| p.missing_syntax_tolerated("type hint"),
                |p| {
                    constraints.push((ConstraintKind::As, p.parse_hint_ex(false)?));
                    p.unit_aborted = unit_aborted;
                    Ok(())
                },
            )?;
            self.c_list(bounds);
        } else {
            self.c_missing_n(2);
        }
        self.require(T::Eq);
        let variants_start = self.tok.start;
        let mut variants = self.vec();
        let mut hints = self.vec();
        // How many variants HHVM can translate.
        let mut translated = 0;
        let variant_list = self.c_mark();
        if self.at(T::Semicolon) {
            // No variants at all.
            self.report_missing(TYPE_EXPECTED);
        } else {
            // Each variant starts with the `|` in front of it, which the
            // first need not have. HHVM's list ends at a variant that is
            // not there.
            let mut first_variant = true;
            while !matches!(self.tok.kind, T::Semicolon | T::Eof) {
                let item = self.c_mark();
                if first_variant {
                    first_variant = false;
                    if !self.eat(T::Bar) {
                        self.c_missing();
                    }
                } else if !self.at(T::Bar) {
                    // Without the `|` a `;` is probably what is missing.
                    if !self.require_missing(
                        T::Bar,
                        "Either the token `|` or `;` is expected here. Use `|` to specify additional variant types for this case type or use `;` to terminate the declaration.",
                    ) {
                        break;
                    }
                } else {
                    self.bump();
                }
                if !self.at_hint_start() || self.at(T::LtLt) {
                    // No type: HHVM says so about the token that is there
                    // and takes the whole variant for missing, `|`
                    // included.
                    self.whole_token(false);
                    self.report_syntax(self.tok.start, self.tok.end, TYPE_EXPECTED);
                    if let Some(cst) = &mut self.cst {
                        cst.stack.truncate(item);
                        cst.missing();
                    }
                    break;
                }
                let variant = self.parse_hint_ex(false)?;
                if let HintKind::Access(_) = variant.kind {
                    self.report(
                        variant.span.start,
                        variant.span.end,
                        "Type aliases to type constants are not supported",
                    );
                }
                hints.push(variant);
                let where_start = self.tok.start;
                let where_constraints = self.parse_where_opt()?;
                if self.prev_end > where_start {
                    self.require_feature(where_start, self.prev_end, Feature::CaseTypeWhereClauses);
                }
                variants.push(CaseTypeVariant {
                    hint: variant,
                    where_constraints,
                });
                self.c_node(K::CaseTypeVariant);
                if self.unit_aborted == unit_aborted {
                    translated += 1;
                }
                self.unit_aborted = unit_aborted;
            }
        }
        self.c_list(variant_list);
        let variants_span = self.span_from(variants_start);
        self.expect_semicolon()?;
        if translated == 0 {
            // Nothing for the name to stand for.
            let (start, end) = (first.start, self.prev_end.max(first.start));
            let text = &self.src[start as usize..end as usize];
            self.abort_lowering(
                start,
                end,
                format!("Encountered unexpected text `{text}`, was expecting a case type variant."),
            );
        }
        self.c_node(K::CaseTypeDeclaration);
        for _ in tparams.iter().filter(|tparam| tparam.reified) {
            self.report_lowering(first.start, self.prev_end, REIFY_MISPLACED);
        }
        let hint = match hints.len() {
            1 => hints[0],
            // None that could be read (see `repair.rs`).
            0 => Hint {
                span: variants_span,
                kind: HintKind::Name(""),
            },
            _ => Hint {
                span: variants_span,
                kind: HintKind::Union(hints.into_bump_slice()),
            },
        };
        Ok(Def::Typedef(self.alloc(Typedef {
            span: self.span_from(first.start),
            name,
            kind: TypedefKind::CaseType,
            attrs,
            tparams,
            constraints: constraints.into_bump_slice(),
            hint,
            variants: variants.into_bump_slice(),
            internal: m.has_internal,
            doc_comment: self.doc_before(first),
        })))
    }

    fn parse_use_kind(&mut self) -> UseKind {
        let kind = match self.tok.kind {
            T::Namespace => UseKind::Namespace,
            T::Type => UseKind::Type,
            T::Function => UseKind::Function,
            T::Const => UseKind::Const,
            _ => {
                self.c_missing();
                return UseKind::Default;
            }
        };
        self.bump();
        kind
    }

    /// Whether the `use` declaration at hand has the `use Prefix\{..}`
    /// form. HHVM decides by looking at what follows the first name.
    fn is_group_use(&mut self) -> bool {
        // No group without a `{` ahead of the first `,` or `;`.
        let rest = &self.bytes[self.tok.end as usize..];
        let stop = rest.iter().position(|b| matches!(b, b'{' | b',' | b';'));
        if stop.is_none_or(|at| rest[at] != b'{') {
            return false;
        }
        let snap = self.snapshot();
        self.bump();
        if matches!(
            self.tok.kind,
            T::Type | T::Namespace | T::Function | T::Const
        ) {
            self.bump();
        }
        let group = match self.tok.kind {
            T::Backslash | T::Name => {
                let leading_backslash = self.at(T::Backslash);
                match self.parse_qualified_name() {
                    Ok((_, trailing)) => (leading_backslash && trailing) || self.at(T::LBrace),
                    Err(_) => false,
                }
            }
            _ => false,
        };
        self.restore(snap);
        group
    }

    fn parse_use(&mut self) -> R<Def<'a>> {
        let is_group = self.is_group_use();
        self.require(T::Use);
        let decl_kind = self.parse_use_kind();
        let has_decl_kind = decl_kind != UseKind::Default;
        let mut clauses = self.vec();
        if is_group {
            // The prefix: a name that should end in a backslash.
            let prefix = if matches!(self.tok.kind, T::Name | T::Namespace | T::Backslash)
                || (self.tok.kind.is_keyword() && !self.tok.kind.is_reserved())
            {
                let name_mark = self.c_mark();
                // A trailing backslash is what a prefix is meant to have.
                let errors = self.errors.len();
                let (name, trailing) = self.parse_qualified_name()?;
                self.errors.truncate(errors);
                self.c_name_unless_reserved(name_mark);
                if !trailing {
                    self.report(
                        name.span.start,
                        name.span.end,
                        "Expected group `use` prefix to end with `\\`",
                    );
                }
                name
            } else {
                self.c_missing();
                self.missing_id()
            };
            self.require(T::LBrace);
            let group = self.c_mark();
            if !self.at(T::RBrace) {
                self.parse_use_clauses(
                    decl_kind,
                    Some((prefix.name, has_decl_kind)),
                    T::RBrace,
                    "A right brace `}` is expected here.",
                    &mut clauses,
                )?;
            }
            self.c_list(group);
            self.require(T::RBrace);
        } else {
            let clause_list = self.c_mark();
            self.parse_use_clauses(
                decl_kind,
                None,
                T::Semicolon,
                "A name is expected here.",
                &mut clauses,
            )?;
            self.c_list(clause_list);
        }
        self.expect_semicolon()?;
        self.c_node(if is_group {
            K::NamespaceGroupUseDeclaration
        } else {
            K::NamespaceUseDeclaration
        });
        Ok(Def::Use(clauses.into_bump_slice()))
    }

    /// The clauses of a `use` declaration up to `close`: a comma-separated
    /// list that may end in a comma, where `missing` is what to say about
    /// a missing clause. `group` is the prefix of a group use and whether
    /// the declaration says what kind of names it imports.
    fn parse_use_clauses(
        &mut self,
        decl_kind: UseKind,
        group: Option<(&'a str, bool)>,
        close: T,
        missing: &'static str,
        clauses: &mut BVec<'a, UseClause<'a>>,
    ) -> R<()> {
        self.separated_list(
            T::Comma,
            ListKind::TrailingAllowed,
            |kind| kind == close,
            missing,
            |_| {},
            |p| p.parse_use_clause(decl_kind, group, clauses),
        )
    }

    fn parse_use_clause(
        &mut self,
        decl_kind: UseKind,
        group: Option<(&'a str, bool)>,
        clauses: &mut BVec<'a, UseClause<'a>>,
    ) -> R<()> {
        let clause_start = self.tok.start;
        let own_kind = self.parse_use_kind();
        let kind = if own_kind == UseKind::Default {
            decl_kind
        } else {
            own_kind
        };
        let name = if self.retokenize_xhp_class_name() {
            let tok = self.take();
            self.id(tok)
        } else if matches!(self.tok.kind, T::Name | T::Namespace | T::Backslash)
            || (self.tok.kind.is_keyword() && !self.tok.kind.is_reserved())
        {
            let name_mark = self.c_mark();
            let (name, _) = self.parse_qualified_name()?;
            self.c_name_unless_reserved(name_mark);
            name
        } else {
            self.missing_name()
        };
        let alias = if self.eat(T::As) {
            Some(if self.retokenize_xhp_class_name() {
                let tok = self.take();
                self.id(tok)
            } else {
                self.require_id(T::Name)
            })
        } else {
            self.c_missing_n(2);
            None
        };
        self.c_node(K::NamespaceUseClause);
        if let Some((_, has_decl_kind)) = group {
            // In a group, a clause says what it imports only where the
            // declaration does not, and then only `function` or `const`.
            let invalid = if has_decl_kind {
                own_kind != UseKind::Default
            } else {
                matches!(own_kind, UseKind::Namespace | UseKind::Type)
            };
            if invalid {
                self.report_from(
                    clause_start,
                    "A namespace `use` clause may not specify the kind here.",
                );
            }
        }
        if name.name.is_empty() {
            return Ok(());
        }
        if group.is_some() && name.name.starts_with('\\') {
            // The prefix says where the name is.
            self.report(
                name.span.start,
                name.span.end,
                "Unnecessary preceding backslash",
            );
        }
        let prefix = group.map(|(prefix, _)| prefix);
        self.check_use_clause(
            kind,
            prefix,
            name,
            alias.filter(|alias| !alias.name.is_empty()),
        );
        if let Some(alias) = alias
            && alias.name.contains([':', '-'])
        {
            self.report_lowering(
                alias.span.start,
                alias.span.end,
                "Cannot use namespace alias with xhp-style names (contains `:` or `-`)",
            );
        }
        let name = match prefix {
            Some(prefix) => Id {
                span: name.span,
                name: bumpalo::format!(in self.arena, "{}{}", prefix, name.name).into_bump_str(),
            },
            None => name,
        };
        if name.name.contains([':', '-']) {
            self.report_lowering(
                clause_start,
                self.prev_end,
                "Cannot use namespace with xhp-style names (contains `:` or `-`)",
            );
        }
        clauses.push(UseClause { kind, name, alias });
        Ok(())
    }

    fn parse_global_const(&mut self, first: Token) -> R<Def<'a>> {
        // Neither attributes nor modifiers.
        self.c_missing_n(2);
        self.require(T::Const);
        let (hint, items) = self.parse_const_declarators(false)?;
        self.c_node(K::ConstDeclaration);
        for item in items {
            self.check_declared_value_name(item.name, false);
        }
        Ok(Def::Const(self.alloc(ConstDecl {
            span: self.span_from(first.start),
            hint,
            items,
        })))
    }

    // ------------------------------------------------------------------
    // Top level

    /// Parses definitions until `end`, folding the ones that follow a
    /// `namespace Foo;` declaration into that namespace.
    pub(crate) fn parse_defs_until(&mut self, end: T, out: &mut BVec<'a, Def<'a>>) -> R<()> {
        // The open `namespace Foo;` (if any) and the definitions collected
        // for it so far.
        let mut open: Option<(Id<'a>, BVec<'a, Def<'a>>)> = None;
        loop {
            if self.at(end) || self.at(T::Eof) {
                break;
            }
            let is_namespace_decl = self.at(T::Namespace) && {
                let next = self.peek();
                !(next.kind == T::Backslash)
            };
            if is_namespace_decl {
                if let Some((name, defs)) = open.take() {
                    out.push(Def::Namespace(self.alloc(Namespace {
                        name,
                        defs: defs.into_bump_slice(),
                    })));
                }
                let keyword_start = self.tok.start;
                let keyword_end = self.tok.end;
                self.bump();
                self.check_case()?;
                let name_mark = self.c_mark();
                let (name, _) = if self.at(T::LBrace) {
                    self.c_missing();
                    // `namespace { ... }`: HHVM gives the missing name an
                    // empty position right after the keyword's trivia.
                    let at = self.leading_trivia_start(keyword_end) as u32;
                    (
                        Id {
                            span: Span::new(at, at + 1),
                            name: "",
                        },
                        false,
                    )
                } else if self.at(T::Name) {
                    self.parse_qualified_name()?
                } else if self.tok.kind.is_keyword() && !self.tok.kind.is_reserved() {
                    // A keyword HHVM does not reserve is a name here, but
                    // no more than that one word.
                    let tok = self.take();
                    (self.id(tok), false)
                } else {
                    (self.missing_name(), false)
                };
                self.c_name_unless_reserved(name_mark);
                self.c_node(K::NamespaceDeclarationHeader);
                if self.eat(T::Semicolon) {
                    self.c_node(K::NamespaceEmptyBody);
                    self.c_node(K::NamespaceDeclaration);
                    self.note_namespace_statement(keyword_start, name.name);
                    self.file.in_open_namespace = true;
                    open = Some((name, self.vec()));
                } else {
                    // Without a `;` or a `{` HHVM takes the body for an
                    // empty one that is missing its braces.
                    let lbrace_start = self.tok.start;
                    let has_body = self.eat(T::LBrace);
                    if !has_body {
                        self.report_missing(
                            "A semicolon `;` or a namespace body is expected here.",
                        );
                        self.c_missing();
                    }
                    let mut inner = self.vec();
                    let block = self.enter_namespace_block(name.name);
                    if has_body {
                        let declarations = self.c_mark();
                        self.enter()?;
                        self.parse_defs_until(T::RBrace, &mut inner)?;
                        self.leave();
                        self.c_list(declarations);
                        self.require(T::RBrace);
                    } else {
                        self.c_missing_n(2);
                    }
                    self.c_node(K::NamespaceBody);
                    self.c_node(K::NamespaceDeclaration);
                    self.leave_namespace_block(block, keyword_start, lbrace_start);
                    if inner.is_empty() {
                        // HHVM treats an empty block like `namespace Foo;`.
                        self.file.in_open_namespace = true;
                        open = Some((name, inner));
                    } else {
                        out.push(Def::Namespace(self.alloc(Namespace {
                            name,
                            defs: inner.into_bump_slice(),
                        })));
                    }
                }
                continue;
            }
            let target = match &mut open {
                Some((_, defs)) => defs,
                None => &mut *out,
            };
            if self.recovering {
                let snap = self.snapshot();
                let decl_start = self.tok.start as usize;
                let kept = target.len();
                let failed = self.parse_def(target).is_err();
                if std::mem::take(&mut self.salvage_failed)
                    || failed
                    || self.tok.start as usize == decl_start
                {
                    target.truncate(kept);
                    self.restore(snap);
                    self.resync_top_level(decl_start);
                }
                continue;
            }
            self.parse_def(target)?;
        }
        if let Some((name, defs)) = open.take() {
            out.push(Def::Namespace(self.alloc(Namespace {
                name,
                defs: defs.into_bump_slice(),
            })));
        }
        Ok(())
    }

    fn parse_def(&mut self, out: &mut BVec<'a, Def<'a>>) -> R<()> {
        let scope = self.enter_scope(Expected::CLASSISH);
        let r = self.parse_def_inner(out);
        self.leave_scope(scope);
        r
    }

    fn parse_def_inner(&mut self, out: &mut BVec<'a, Def<'a>>) -> R<()> {
        self.check_case_of_statement();
        let first = self.tok;
        self.begin_unit();
        let kind = self.tok.kind;
        let def = match kind {
            T::Include | T::IncludeOnce | T::Require | T::RequireOnce => {
                let e = self.parse_expr()?;
                self.expect_semicolon()?;
                self.c_node(K::InclusionDirective);
                let stmt = Stmt {
                    span: self.span_from(first.start),
                    kind: StmtKind::Expr(self.alloc(e)),
                };
                Def::Stmt(self.alloc(stmt))
            }
            T::Type | T::Newtype
                if {
                    let next = self.peek();
                    next.kind == T::Classname || Self::is_plain_name(next)
                } =>
            {
                self.c_missing();
                self.parse_type_alias(first, &[])?
            }
            T::Module if self.peek_kind() == T::Newtype => {
                self.c_missing();
                self.parse_type_alias(first, &[])?
            }
            T::Case => {
                self.c_missing();
                self.parse_type_alias(first, &[])?
            }
            T::Newctx => {
                self.c_missing();
                self.parse_ctx_alias(first, &[])?
            }
            T::Module => self.parse_module_membership()?,
            T::New if self.peek_kind() == T::Module => {
                self.c_missing();
                self.parse_module_decl(first, &[])?
            }
            T::Enum => {
                self.c_missing();
                self.parse_enum(first, &[])?
            }
            T::Use => self.parse_use()?,
            T::Trait | T::Interface | T::Class | T::Abstract | T::Final | T::Xhp => {
                self.c_missing();
                self.parse_class(first, &[])?
            }
            T::Const => self.parse_global_const(first)?,
            T::LtLt | T::Internal | T::Public => {
                if self.at(T::LtLt) && self.peek_kind() == T::File {
                    // `<<file: Attr>>`
                    let snap = self.snapshot();
                    self.bump();
                    self.bump();
                    if self.eat(T::Colon) {
                        let mut attrs = self.vec();
                        self.parse_attribute_list(&mut attrs, true, first.start)?;
                        self.c_node(K::FileAttributeSpecification);
                        out.push(Def::FileAttributes(attrs.into_bump_slice()));
                        return Ok(());
                    }
                    self.restore(snap);
                }
                let before_attrs = self.c_mark();
                let attrs = self.parse_attributes_opt()?;
                let has_visibility = matches!(self.tok.kind, T::Internal | T::Public);
                let after = if has_visibility {
                    self.peek_kind()
                } else {
                    self.tok.kind
                };
                match after {
                    T::Enum => self.parse_enum(first, attrs)?,
                    T::Type | T::Newtype | T::Case => self.parse_type_alias(first, attrs)?,
                    T::Module if !has_visibility => self.parse_type_alias(first, attrs)?,
                    T::Newctx => self.parse_ctx_alias(first, attrs)?,
                    T::New if !has_visibility && self.peek_kind() == T::Module => {
                        self.parse_module_decl(first, attrs)?
                    }
                    T::Async | T::Function => self.parse_function_def(first, attrs)?,
                    T::Abstract | T::Final | T::Interface | T::Trait | T::Xhp | T::Class => {
                        self.parse_class(first, attrs)?
                    }
                    _ => {
                        // Nothing that attributes or a visibility can be
                        // in front of. HHVM forgets the attributes and
                        // makes an error of the next token on its own.
                        if let Some(cst) = &mut self.cst {
                            cst.stack.truncate(before_attrs);
                        }
                        let token = self.tok;
                        if self.at(T::Eof) {
                            self.c_push_current();
                        } else {
                            self.bump();
                        }
                        let (start, end) = self.missing_span();
                        let text = self.tok_text(token);
                        self.report_syntax(
                            start,
                            end,
                            format!("Encountered unexpected token `{text}`."),
                        );
                        self.c_node(K::ErrorSyntax);
                        let stmt = self.declaration_statement(token.start);
                        Def::Stmt(self.alloc(stmt))
                    }
                }
            }
            T::Async | T::Function => {
                // Distinguish a declaration from an expression statement
                // that starts with a closure or async block.
                let k1 = self.peek_kind();
                let is_expression = match (self.tok.kind, k1) {
                    (T::Function, T::LParen) | (T::Async, T::LParen) | (T::Async, T::LBrace) => {
                        true
                    }
                    (T::Async, T::Function) => {
                        let snap = self.snapshot();
                        self.bump();
                        let k2 = self.peek_kind();
                        self.restore(snap);
                        k2 == T::LParen
                    }
                    _ => false,
                };
                if is_expression {
                    let stmt = self.parse_statement()?;
                    Def::Stmt(self.alloc(stmt))
                } else {
                    self.c_missing();
                    self.parse_function_def(first, &[])?
                }
            }
            _ => {
                let stmt = self.parse_statement()?;
                if matches!(stmt.kind, StmtKind::Noop) {
                    return Ok(());
                }
                Def::Stmt(self.alloc(stmt))
            }
        };
        if !self.pending_patterns.is_empty() {
            self.report_pending_patterns(0);
        }
        self.note_def(&def);
        self.check_def(&def);
        out.push(def);
        Ok(())
    }
}
