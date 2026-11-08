//! Carrying on after a syntax error, the way HHVM's parser does.
//!
//! HHVM's parser never gives up. Where something is missing it says so,
//! leaves an empty node in its place and continues with the token it is at;
//! a token that fits nowhere is reported and then either dropped or put in
//! the tree where an expression was wanted. Its later passes run over the
//! tree that results. Reporting what HHVM reports for a broken file, and
//! building the lossless tree it builds, takes making the same choices, so
//! what is here follows HHVM's recovery step by step. All of it is off the
//! hot path: valid code never gets here.
//!
//! The tree built for analysis is of no use once this has happened (a file
//! with a syntax error is parsed again for its declarations, see
//! `recovery.rs`), but the checks that run while parsing still look at it:
//! an expression that is not there is [`ExprKind::Omitted`] with an empty
//! span, and a token standing in for one is the identifier HHVM makes of
//! it.

use super::expr::{Shape, Term};
use super::{Parser, R};
use crate::ast::*;
use crate::cst::SyntaxKind as K;
use crate::span::Span;
use crate::token::T;

const EXPRESSION_EXPECTED: &str = "An expression is expected here.";

/// What a list that [`Parser::separated_list`] reads may look like.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListKind {
    /// No separator after the last item.
    NoTrailing,
    /// A separator may follow the last item.
    TrailingAllowed,
}

impl<'a> Parser<'a> {
    /// An expression that is not there. HHVM puts up with that in a few
    /// places (see [`absent`](Self::absent)); anywhere else its translation
    /// to the AST complains, unless it has complained about something
    /// already.
    #[cold]
    pub(crate) fn missing_term(&mut self) -> Term<'a> {
        let at = self.missing_pos();
        self.c_missing();
        self.missing_expression();
        Term {
            e: Expr {
                span: Span::new(at, at),
                kind: ExprKind::Omitted,
            },
            start: at,
            end: at,
            shape: Shape::Other,
        }
    }

    /// The complaint of HHVM's translation to the AST about an expression
    /// that is not there, where the next token is.
    #[cold]
    pub(crate) fn missing_expression(&mut self) {
        let at = self.missing_pos();
        self.missing_reported = None;
        if !self.unit_aborted && !self.lowering_errors {
            self.first_only_at = self.lowering_list.len();
            self.missing_reported = Some(self.lowering_list.len());
            self.report_lowering(
                at,
                at + 1,
                "Encountered unexpected text ``, was expecting a expression.",
            );
        }
    }

    /// Whether `e` is an expression that is not there, for the places where
    /// HHVM takes that for an optional part left out: nothing is wrong with
    /// it then as far as the translation to the AST goes.
    #[inline]
    pub(crate) fn absent(&mut self, e: &Expr<'a>) -> bool {
        matches!(e.kind, ExprKind::Omitted) && e.span.start == e.span.end && {
            self.forgive_missing();
            true
        }
    }

    /// Takes back the complaint about the missing expression parsed last.
    #[cold]
    fn forgive_missing(&mut self) {
        if let Some(at) = self.missing_reported.take()
            && at + 1 == self.lowering_list.len()
        {
            self.lowering_list.pop();
            self.lowering_errors = false;
            if self.first_only_at >= at {
                self.first_only_at = usize::MAX;
            }
        }
    }

    /// Takes the current token for an expression. HHVM's AST has the
    /// identifier it spells there.
    #[cold]
    pub(crate) fn token_term(&mut self) -> Term<'a> {
        self.whole_token(true);
        let tok = self.tok;
        if self.at(T::Eof) {
            // The end of the file stays where it is, but what is missing
            // after it is missing there.
            self.c_push_current();
            (self.prev_start, self.prev_end) = (tok.start, tok.end);
        } else {
            self.bump();
        }
        Term {
            e: Expr {
                span: Self::tok_span(tok),
                kind: ExprKind::Id(self.tok_text(tok)),
            },
            start: tok.start,
            end: tok.end,
            shape: Shape::Token,
        }
    }

    /// What HHVM makes of a token no expression starts with. One that a
    /// construct being parsed is waiting for stays, and the expression is
    /// missing; any other becomes the expression.
    #[cold]
    pub(crate) fn parse_unexpected_term(&mut self) -> Term<'a> {
        let kind = self.tok.kind;
        if self.expects(kind) {
            self.report_missing(EXPRESSION_EXPECTED);
            return self.missing_term();
        }
        let term = self.token_term();
        self.report_missing(if kind == T::Eq {
            "An expression cannot start with `=`"
        } else {
            EXPRESSION_EXPECTED
        });
        term
    }

    /// What HHVM makes of a token no type starts with: an error node in
    /// the type's place. Its translation to the AST gives up on the
    /// declaration at that node.
    #[cold]
    pub(crate) fn unexpected_hint(&mut self) -> Hint<'a> {
        self.whole_token(true);
        let tok = self.tok;
        let text = self.tok_text(tok);
        self.report_syntax(tok.start, tok.end, "A type specifier is expected here.");
        if self.at(T::Eof) {
            self.c_push_current();
            (self.prev_start, self.prev_end) = (tok.start, tok.end);
        } else {
            self.bump();
        }
        self.c_node(K::ErrorSyntax);
        self.abort_lowering(
            tok.start,
            tok.end,
            format!("Encountered unexpected text `{text}`, was expecting a type hint."),
        );
        Hint {
            span: Self::tok_span(tok),
            kind: HintKind::Name(text),
        }
    }

    /// A class member that is not there: the current token is one a
    /// construct around the class is waiting for, so HHVM leaves it alone
    /// and ends the members.
    #[cold]
    pub(crate) fn missing_class_member(&mut self) {
        let at = self.missing_pos();
        self.c_missing();
        if !self.unit_aborted && !self.lowering_errors {
            self.first_only_at = self.lowering_list.len();
            self.report_lowering(
                at,
                at + 1,
                "Encountered unexpected text ``, was expecting a class element.",
            );
        }
    }

    /// What HHVM makes of a token that starts no class member: an error
    /// node among the members.
    #[cold]
    pub(crate) fn unexpected_class_member(&mut self) {
        self.whole_token(true);
        let tok = self.tok;
        let text = self.tok_text(tok);
        self.bump();
        self.report_missing(
            "A class member, method, type, trait usage, trait require, xhp attribute, xhp use, or xhp category is expected here.",
        );
        self.c_node(K::ErrorSyntax);
        if !self.unit_aborted && !self.lowering_errors {
            self.first_only_at = self.lowering_list.len();
            self.report_lowering(
                tok.start,
                tok.end,
                format!("Encountered unexpected text `{text}`, was expecting a class element."),
            );
        }
    }

    /// Reads a list of items with `separator` between them, up to a token
    /// `close` accepts (which stays), the way HHVM's parser does. Where an
    /// item is due and the list closes, the file ends or a separator
    /// stands, the item is missing, which `missing` says. After an item
    /// that neither a separator nor the close follows, the list just ends:
    /// whoever wants the close reports it. `item` records the item it
    /// reads; the items go on the stack of the lossless tree one by one,
    /// for the caller to wrap. `gap` is called for each missing item.
    pub(crate) fn separated_list(
        &mut self,
        separator: T,
        kind: ListKind,
        close: impl Fn(T) -> bool,
        missing: &'static str,
        mut gap: impl FnMut(&mut Self),
        mut item: impl FnMut(&mut Self) -> R<()>,
    ) -> R<()> {
        loop {
            if close(self.tok.kind) || self.at(T::Eof) {
                self.report_missing(missing);
                self.c_missing();
                gap(self);
                self.c_list_item(false);
                return Ok(());
            }
            if self.at(separator) {
                self.c_missing();
                self.bump();
                self.report_missing(missing);
                gap(self);
                self.c_list_item(true);
                continue;
            }
            item(self)?;
            if !self.eat_separator(separator) {
                return Ok(());
            }
            if kind == ListKind::TrailingAllowed && close(self.tok.kind) {
                return Ok(());
            }
        }
    }

    /// `self` as a type has to be followed by `::`. Without it HHVM puts
    /// the keyword in an error node, which its translation to the AST
    /// gives up on.
    #[cold]
    pub(crate) fn unqualified_self_hint(&mut self, tok: crate::lexer::Token) -> Hint<'a> {
        let text = self.tok_text(tok);
        self.report_missing("A scope resolution operator `::` is expected here.");
        self.c_node(K::ErrorSyntax);
        self.abort_lowering(
            tok.start,
            tok.end,
            format!("Encountered unexpected text `{text}`, was expecting a type hint."),
        );
        Hint {
            span: Self::tok_span(tok),
            kind: HintKind::Name(text),
        }
    }

    /// The base type of an `enum` is not there (`enum E {`): HHVM's
    /// checks say so at the name, and its translation to the AST gives up.
    #[cold]
    pub(crate) fn missing_enum_base(&mut self, name: Id<'a>) -> Hint<'a> {
        self.report(
            name.span.start,
            name.span.end,
            "Enums must have a base type, such as `arraykey` or `int`.",
        );
        // Neither `:` nor type.
        self.c_missing_n(2);
        self.missing_hint();
        let at = self.missing_pos();
        Hint {
            span: Span::new(at, at),
            kind: HintKind::Name(""),
        }
    }

    /// A type that is not there, where HHVM's translation to the AST needs
    /// one: it gives up on the declaration.
    #[cold]
    pub(crate) fn missing_hint(&mut self) {
        self.missing_syntax("type hint");
    }

    /// Something HHVM's translation to the AST cannot do without is not
    /// there (`what`, in its words): it gives up on the declaration.
    #[cold]
    pub(crate) fn missing_syntax(&mut self, what: &str) {
        let at = self.missing_pos();
        self.abort_lowering(
            at,
            at + 1,
            format!("Encountered unexpected text ``, was expecting a {what}."),
        );
    }

    /// Like [`missing_syntax`](Self::missing_syntax) where HHVM's
    /// translation to the AST does without what is missing and carries on
    /// with the declaration: it only says so, and only if it has said
    /// nothing yet.
    #[cold]
    pub(crate) fn missing_syntax_tolerated(&mut self, what: &str) {
        let at = self.missing_pos();
        if !self.unit_aborted && !self.lowering_errors {
            self.first_only_at = self.lowering_list.len();
            self.report_lowering(
                at,
                at + 1,
                format!("Encountered unexpected text ``, was expecting a {what}."),
            );
        }
    }

    /// The value of a constant that has to have one is not there: HHVM's
    /// translation to the AST gives up on the declaration.
    #[cold]
    pub(crate) fn missing_initializer(&mut self) {
        self.missing_syntax("simple initializer");
    }

    /// In a comma-separated list, where an item is due but a comma stands
    /// or the file ends: HHVM takes the item for missing, which `missing`
    /// says. Returns whether the list goes on (after the comma).
    #[cold]
    pub(crate) fn missing_list_item(&mut self, missing: &'static str) -> bool {
        self.c_missing();
        if self.at(T::Comma) {
            self.bump();
            self.report_missing(missing);
            self.c_list_item(true);
            true
        } else {
            self.report_missing(missing);
            self.c_list_item(false);
            false
        }
    }

    /// What HHVM makes of a token that cannot name a function: an error
    /// node in the name's place.
    #[cold]
    pub(crate) fn unexpected_function_name(&mut self) -> Id<'a> {
        self.whole_token(true);
        let tok = self.take();
        self.report_missing("A name or `__construct` keyword is expected here.");
        self.c_node(K::ErrorSyntax);
        self.id(tok)
    }

    /// Among the modifiers of a class member, a token that is none,
    /// right in front of `function`: HHVM takes it for a slip of the
    /// keyboard and drops it, unless a blank line separates the two.
    #[cold]
    pub(crate) fn skip_token_before_function(&mut self) {
        let next = self.peek();
        let end = self.tok.end as usize;
        let leading_start = end + crate::cst::trivia::hhvm_trailing_width(self.bytes, end);
        let leading = &self.bytes[leading_start.min(next.start as usize)..next.start as usize];
        if leading.contains(&b'\n') || leading.contains(&b'\r') {
            return;
        }
        let (start, end) = (self.tok.start, self.tok.end);
        let text = &self.src[start as usize..end as usize];
        self.report_syntax(
            start,
            end,
            format!("This token `{text}` is not valid as part of a function declaration."),
        );
        self.skip_token();
    }

    /// Neither a body nor a `;` after the header of a method: HHVM takes
    /// the token that is there for the body, in an error node, which its
    /// translation to the AST then finds no expression in.
    #[cold]
    pub(crate) fn unexpected_method_body(&mut self) {
        self.whole_token(true);
        let tok = self.take();
        let text = self.tok_text(tok);
        self.report_missing("A function body or a semicolon `;` is expected here.");
        self.c_node(K::ErrorSyntax);
        // No `;` either.
        self.c_missing();
        if !self.unit_aborted && !self.lowering_errors {
            self.first_only_at = self.lowering_list.len();
            self.report_lowering(
                tok.start,
                tok.end,
                format!("Encountered unexpected text `{text}`, was expecting a expression."),
            );
        }
    }

    /// A function declared where a statement goes. HHVM parses the
    /// declaration and then says that it has no business there; its
    /// translation to the AST finds no statement in it.
    #[cold]
    pub(crate) fn parse_nested_function(&mut self) -> R<Stmt<'a>> {
        let first = self.tok;
        // No attributes.
        self.c_missing();
        let outer = std::mem::replace(&mut self.nested_function, true);
        let untranslated = self.begin_untranslated();
        let parsed = self.parse_function_def(first, &[]);
        self.end_untranslated(untranslated);
        self.nested_function = outer;
        parsed?;
        self.report_missing("Inline function definitions are not supported in Hack");
        Ok(self.declaration_statement(first.start))
    }

    /// A class declared where a statement goes: HHVM says so, and parses
    /// it all the same.
    #[cold]
    pub(crate) fn parse_nested_class(&mut self) -> R<Stmt<'a>> {
        let first = self.tok;
        self.report_missing("Declarations are not supported outside global scope");
        // No attributes.
        self.c_missing();
        // What is known about the class this statement is inside of.
        let outer = (
            self.class.take(),
            std::mem::take(&mut self.class_members),
            std::mem::take(&mut self.class_requires),
            self.class_has_abstract_method,
            self.ctor_promotion.take(),
        );
        let untranslated = self.begin_untranslated();
        let parsed = self.parse_class(first, &[]);
        self.end_untranslated(untranslated);
        (
            self.class,
            self.class_members,
            self.class_requires,
            self.class_has_abstract_method,
            self.ctor_promotion,
        ) = outer;
        parsed?;
        Ok(self.declaration_statement(first.start))
    }

    /// Starts something HHVM never translates to its AST, so that nothing
    /// in it is an error to that translation or to the checks that run on
    /// the AST. The result goes to
    /// [`end_untranslated`](Self::end_untranslated).
    pub(crate) fn begin_untranslated(&self) -> (usize, usize, bool, bool) {
        (
            self.tree_errors.len(),
            self.lowering_list.len(),
            self.lowering_errors,
            self.unit_aborted,
        )
    }

    pub(crate) fn end_untranslated(
        &mut self,
        (tree_errors, lowering_list, lowering_errors, unit_aborted): (usize, usize, bool, bool),
    ) {
        self.tree_errors.truncate(tree_errors);
        self.lowering_list.truncate(lowering_list);
        if self.first_only_at >= lowering_list {
            self.first_only_at = usize::MAX;
        }
        self.lowering_errors = lowering_errors;
        self.unit_aborted = unit_aborted;
    }

    /// The statement a declaration (or anything else that is none) that
    /// started at `start` stands for: none, which HHVM's translation to the
    /// AST complains about unless it has complained about something already.
    pub(crate) fn declaration_statement(&mut self, start: u32) -> Stmt<'a> {
        let span = self.span_from(start);
        if !self.unit_aborted && !self.lowering_errors {
            self.first_only_at = self.lowering_list.len();
            let text = span.text(self.src);
            self.report_lowering(
                span.start,
                span.end,
                format!("Encountered unexpected text `{text}`, was expecting a statement."),
            );
        }
        Stmt {
            span,
            kind: StmtKind::Noop,
        }
    }

    /// Neither `class` nor `interface` nor `trait` where one of them is
    /// due. A name one letter off is taken for a misspelling and dropped;
    /// anything else stays. HHVM's translation to the AST cannot tell what
    /// is being declared and gives up.
    #[cold]
    pub(crate) fn missing_class_keyword(&mut self) {
        let (start, end) = (self.tok.start, self.tok.end);
        let text = &self.src[start as usize..end as usize];
        let meant = ["class", "trait", "interface"]
            .into_iter()
            .find(|keyword| self.at(T::Name) && super::one_byte_apart(keyword, text));
        match meant {
            Some(keyword) => {
                self.report_syntax(
                    start,
                    end,
                    format!("Encountered unexpected token `{text}`. Did you mean `{keyword}`?"),
                );
                self.skip_token();
            }
            None => self.report_missing("The `class` keyword is expected here."),
        }
        self.c_missing();
        let at = self.missing_pos();
        self.abort_lowering(
            at,
            at + 1,
            "Encountered unexpected text ``, was expecting a class kind.",
        );
    }

    /// A statement that is not there: the current token is one a construct
    /// being parsed is waiting for, so HHVM leaves it alone. The lists of
    /// statements end at such a statement.
    #[cold]
    pub(crate) fn missing_statement(&mut self) -> Stmt<'a> {
        let at = self.missing_pos();
        self.c_missing();
        if !self.unit_aborted && !self.lowering_errors {
            self.first_only_at = self.lowering_list.len();
            self.report_lowering(
                at,
                at + 1,
                "Encountered unexpected text ``, was expecting a statement.",
            );
        }
        Stmt {
            span: Span::new(at, at),
            kind: StmtKind::Noop,
        }
    }
}
