//! Module declarations and context aliases: rare top-level forms that mostly
//! appear in HHVM's own declaration files.

use super::repair::ListKind;
use super::{Parser, R};
use crate::ast::*;
use crate::checks::Feature;
use crate::cst::SyntaxKind as K;
use crate::lexer::Token;
use crate::token::T;

const DEFAULT_IN_MODULE_NAME: &str =
    "The `default` module name cannot appear in a compound module name.";

impl<'a> Parser<'a> {
    /// `a.b.c` after `module` / `new module`. The whole dotted name is one
    /// identifier.
    fn parse_module_name(&mut self) -> R<Id<'a>> {
        let start = self.tok.start;
        let parts = self.c_mark();
        let mut first = true;
        loop {
            // `default` names the module of code that declares none, and
            // is no part of any other name.
            let part = self.c_mark();
            let is_default = self.at(T::Default);
            if is_default {
                if !first {
                    self.report_missing(DEFAULT_IN_MODULE_NAME);
                }
                self.bump();
                self.c_name();
            } else if self.expect_name()?.name.is_empty() {
                // The name ends at the last part that is there.
                self.c_remove(part);
                break;
            }
            if !self.at(T::Dot) {
                self.c_list_item(false);
                break;
            }
            // HHVM joins the parts without the trivia between them; a name
            // written across comments or spaces is not worth reproducing.
            if self.tok.trivia_start != self.tok.start {
                return self.fail("unsupported module name");
            }
            self.bump();
            self.c_list_item(true);
            if first && is_default {
                self.report_missing(DEFAULT_IN_MODULE_NAME);
            }
            first = false;
            // The same after the dot, where it matters only if the name
            // goes on.
            if self.tok.trivia_start != self.tok.start
                && (self.tok.kind == T::Name || self.tok.kind.is_keyword())
            {
                return self.fail("unsupported module name");
            }
        }
        self.c_list(parts);
        self.c_node(K::ModuleName);
        let span = self.span_from(start);
        Ok(Id {
            span,
            name: &self.src[span.start as usize..span.end as usize],
        })
    }

    /// `module a.b;`
    pub(crate) fn parse_module_membership(&mut self) -> R<Def<'a>> {
        self.require(T::Module);
        let name = self.parse_module_name()?;
        self.require(T::Semicolon);
        self.c_node(K::ModuleMembershipDeclaration);
        Ok(Def::SetModule(name))
    }

    /// `new module a.b {}`
    pub(crate) fn parse_module_decl(
        &mut self,
        first: Token,
        attrs: &'a [Attribute<'a>],
    ) -> R<Def<'a>> {
        self.require(T::New);
        self.require(T::Module);
        let name = self.parse_module_name()?;
        if name.span.start == name.span.end {
            // A module that has no name at all: HHVM's translation to the
            // AST gives up.
            self.missing_syntax("module name");
        }
        self.require(T::LBrace);
        self.require(T::RBrace);
        self.c_node(K::ModuleDeclaration);
        Ok(Def::Module(self.alloc(ModuleDecl {
            span: self.span_from(first.start),
            attrs,
            name,
            doc_comment: self.doc_before(first),
        })))
    }

    /// `newctx Name as [a, b] = [c];`
    ///
    /// Represented as an opaque alias of the intersection of the contexts.
    pub(crate) fn parse_ctx_alias(
        &mut self,
        first: Token,
        attrs: &'a [Attribute<'a>],
    ) -> R<Def<'a>> {
        if !attrs.is_empty() {
            self.check_declaration_attrs(attrs);
        }
        self.require(T::Newctx);
        let name = self.require_plain_name();
        // No type parameters.
        self.c_missing();
        let mut constraints = self.vec();
        let (mut has_as, mut has_super) = (false, false);
        let constraint_list = self.c_mark();
        loop {
            let kind = match self.tok.kind {
                T::As => ConstraintKind::As,
                T::Super => ConstraintKind::Super,
                _ => break,
            };
            let bound_start = self.tok.start;
            self.bump();
            let Some(ctxs) = self.parse_contexts_opt()? else {
                return self.fail("expected context list");
            };
            self.c_node(K::ContextConstraint);
            let seen = if kind == ConstraintKind::As {
                &mut has_as
            } else {
                &mut has_super
            };
            if *seen {
                let which = if kind == ConstraintKind::As {
                    "as"
                } else {
                    "super"
                };
                self.report(
                    bound_start,
                    self.prev_end,
                    format!("Multiple `{which}` constraints on a ctx constant are not allowed"),
                );
                continue;
            }
            *seen = true;
            constraints.push((
                kind,
                Hint {
                    span: ctxs.span,
                    kind: HintKind::Intersection(ctxs.hints),
                },
            ));
        }
        self.c_list(constraint_list);
        if name.name.chars().next().is_some_and(char::is_lowercase) {
            self.report(
                name.span.start,
                name.span.end,
                format!(
                    "Context {} should begin with an uppercase letter",
                    name.name
                ),
            );
        }
        if !has_as {
            self.report(
                name.span.start,
                name.span.end,
                format!(
                    "Context {} must declare a context constraint e.g. `as [write_props]`",
                    name.name
                ),
            );
        }
        let has_value = self.at(T::Eq);
        let hint = if self.eat(T::Eq) {
            let Some(ctxs) = self.parse_contexts_opt()? else {
                return self.fail("expected context list");
            };
            Hint {
                span: ctxs.span,
                kind: HintKind::Intersection(ctxs.hints),
            }
        } else {
            // No right-hand side: an alias of the default contexts.
            self.c_missing_n(2);
            Hint {
                span: name.span,
                kind: HintKind::Name("defaults"),
            }
        };
        self.require(T::Semicolon);
        self.c_node(K::ContextAliasDeclaration);
        // Both forms are gated, each by a feature of its own.
        let feature = if has_value {
            Feature::ContextAliasDeclaration
        } else {
            Feature::ContextAliasDeclarationShort
        };
        self.require_feature(first.start, self.prev_end, feature);
        Ok(Def::Typedef(self.alloc(Typedef {
            span: self.span_from(first.start),
            name,
            kind: TypedefKind::CtxAlias,
            attrs,
            tparams: &[],
            constraints: constraints.into_bump_slice(),
            hint,
            variants: &[],
            internal: false,
            doc_comment: self.doc_before(first),
        })))
    }

    /// `with { type A = int; ctx C as [io]; }` after a type. One refinement
    /// after another is an error, which HHVM reports and reads as a
    /// refinement of the refinement.
    pub(crate) fn parse_refinement(&mut self, start: u32, base: Hint<'a>) -> R<Hint<'a>> {
        let mut hint = base;
        loop {
            hint = self.parse_one_refinement(start, hint)?;
            if !self.at(T::With) {
                return Ok(hint);
            }
            self.report_missing(
                "A type refinement cannot be chained (e.g., `Cls with { ... } with { type T2 ... }`), consider combining members into a single with-block.",
            );
        }
    }

    fn parse_one_refinement(&mut self, start: u32, base: Hint<'a>) -> R<Hint<'a>> {
        const MEMBER_EXPECTED: &str = "Expected `type` or `ctx` member(s) within the refinement that ends with `}`, e.g.: `type T as U`, `type T = X`, `ctx C super [defaults]`.";
        self.require(T::With);
        self.require(T::LBrace);
        let mut members = self.vec();
        let member_list = self.c_mark();
        if matches!(self.tok.kind, T::Type | T::Ctx) {
            self.separated_list(
                T::Semicolon,
                ListKind::TrailingAllowed,
                |kind| kind == T::RBrace,
                MEMBER_EXPECTED,
                |p| p.missing_syntax("refinement member"),
                |p| p.parse_refinement_member(&mut members, &base),
            )?;
        } else if !self.at(T::RBrace) {
            // HHVM does not look for members behind anything else.
            self.report_missing(MEMBER_EXPECTED);
        }
        self.c_list(member_list);
        self.require(T::RBrace);
        self.c_node(K::TypeRefinement);
        let refinement = self.alloc(RefinementHint {
            base,
            members: members.into_bump_slice(),
        });
        Ok(Hint {
            span: self.span_from(start),
            kind: HintKind::Refinement(refinement),
        })
    }

    /// One member of a refinement, or nothing where there is none.
    fn parse_refinement_member(
        &mut self,
        members: &mut super::List<'a, RefinementMember<'a>>,
        base: &Hint<'a>,
    ) -> R<()> {
        let member_start = self.tok.start;
        let is_ctx = match self.tok.kind {
            T::Type => false,
            T::Ctx => true,
            _ => {
                // No member: HHVM's translation to the AST gives up there.
                self.c_missing();
                self.missing_syntax("refinement member");
                return Ok(());
            }
        };
        self.bump();
        let name = self.expect_name()?;
        // HHVM's parser takes type parameters here, and nothing ever
        // looks at them.
        let untranslated = self.lowering_state();
        self.parse_type_params_opt()?;
        self.restore_lowering_state(untranslated);
        let (bounds, exact);
        if is_ctx {
            let mut out = self.vec();
            let constraint_list = self.c_mark();
            let constraints_start = self.tok.start;
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
                out.push((
                    kind,
                    Hint {
                        span: ctxs.span,
                        kind: HintKind::Intersection(ctxs.hints),
                    },
                ));
            }
            self.c_list(constraint_list);
            let constraints_end = self.prev_end;
            bounds = out.into_bump_slice();
            exact = if self.eat(T::Eq) {
                let Some(ctxs) = self.parse_contexts_opt()? else {
                    return self.fail("expected context list");
                };
                Some(Hint {
                    span: ctxs.span,
                    kind: HintKind::Intersection(ctxs.hints),
                })
            } else {
                self.c_missing_n(2);
                None
            };
            self.c_node(K::CtxInRefinement);
            // HHVM only looks at the bounds when there is no `=`.
            if exact.is_none() {
                for (kind, keyword) in
                    [(ConstraintKind::Super, "super"), (ConstraintKind::As, "as")]
                {
                    if bounds.iter().filter(|(k, _)| *k == kind).count() > 1 {
                        self.report_lowering(
                            constraints_start,
                            constraints_end,
                            format!(
                                "Multiple `{keyword}` constraints on a ctx constant are not allowed"
                            ),
                        );
                    }
                }
            }
        } else {
            bounds = self.parse_bounds(false)?;
            exact = if self.eat(T::Eq) {
                Some(self.parse_hint()?)
            } else {
                self.c_missing_n(2);
                None
            };
            self.c_node(K::TypeInRefinement);
        }
        let member_end = self.prev_end;
        if members
            .iter()
            .any(|m: &RefinementMember<'a>| m.is_ctx == is_ctx && m.name.name == name.name)
        {
            self.report(
                member_start,
                member_end,
                format!(
                    "Member {} is involved in multiple refinement constraints. Members must be constrained at most once, consider fusing constraints (e.g., `Cls with {{ type T as A as B super C; }}`)",
                    name.name
                ),
            );
        }
        // Either `=` or bounds, not both.
        if exact.is_some() != bounds.is_empty() {
            self.report(
                member_start,
                member_end,
                format!(
                    "A member that belongs to a refinement of type {} must be constrained either by an exact bound (`=`) or by at least one `as`/`super` bound.",
                    base.span.text(self.src)
                ),
            );
        }
        members.push(RefinementMember {
            is_ctx,
            name,
            exact,
            bounds,
        });
        Ok(())
    }
}
