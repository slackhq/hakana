//! Statements.

use super::{Expected, Parser, R};
use crate::ast::*;
use crate::checks::{LvalRoot, Nesting};
use crate::cst::SyntaxKind as K;
use crate::span::Span;
use crate::token::T;

impl<'a> Parser<'a> {
    /// `{ ... }`. Empty statements inside are dropped.
    pub(crate) fn parse_compound_statement(&mut self) -> R<Block<'a>> {
        if self.at(T::Semicolon) {
            // HHVM takes a `;` here for the whole block.
            self.bump();
            return Ok(&[]);
        }
        let lbrace_start = self.tok.start;
        self.require(T::LBrace);
        self.enter()?;
        self.enter_block(lbrace_start);
        let mut stmts = self.vec();
        let statements = self.c_mark();
        while !self.at(T::RBrace) && !self.at(T::Eof) {
            let stmt_start = self.tok.start;
            let stmt = self.parse_statement()?;
            if !matches!(stmt.kind, StmtKind::Noop) {
                stmts.push(stmt);
            }
            if self.tok.start == stmt_start {
                // A statement that consumed nothing (see `repair.rs`) ends
                // the list.
                break;
            }
        }
        self.c_list(statements);
        self.leave_block(self.tok.start);
        self.leave();
        self.require(T::RBrace);
        self.c_node(K::CompoundStatement);
        // Tell function-scoped `using` statements which block they cover.
        let block_span = self.span_from(lbrace_start);
        for stmt in stmts.iter_mut() {
            if let StmtKind::Using(using) = stmt.kind
                && using.body.is_none()
            {
                stmt.kind = StmtKind::Using(self.alloc(Using {
                    enclosing_block: block_span,
                    ..*using
                }));
            }
        }
        Ok(stmts.into_bump_slice())
    }

    /// The body of a control-flow statement: a braced block, or a single
    /// statement.
    fn parse_body(&mut self) -> R<Block<'a>> {
        if self.at(T::LBrace) {
            // The block is still a statement of its own as far as doc
            // comments are concerned, but nothing inside can observe that.
            self.parse_compound_statement()
        } else {
            let stmt = self.parse_statement()?;
            Ok(self.arena.alloc_slice_copy(&[stmt]))
        }
    }

    fn parse_paren_expr(&mut self) -> R<Expr<'a>> {
        self.require(T::LParen);
        let e = self.parse_expr()?;
        self.require(T::RParen);
        Ok(e)
    }

    /// The condition of an `if` or the subject of a `switch`.
    fn parse_paren_root(&mut self) -> R<Expr<'a>> {
        self.require(T::LParen);
        let e = self.parse_root_expr()?;
        self.require(T::RParen);
        Ok(e)
    }

    #[inline]
    fn stmt(&self, start: u32, kind: StmtKind<'a>) -> Stmt<'a> {
        Stmt {
            span: self.span_from(start),
            kind,
        }
    }

    pub(crate) fn parse_statement(&mut self) -> R<Stmt<'a>> {
        // A statement in another is two levels down.
        self.enter()?;
        self.enter()?;
        let saved_doc = self.stmt_doc;
        self.stmt_doc = Some((self.tok.trivia_start, self.tok.start));
        let (outer_interest, outer_assigns) = (self.interest, self.assigns);
        let outer_marks = std::mem::take(&mut self.marks);
        self.interest = false;
        self.assigns = 0;
        self.await_ok_at = u32::MAX;
        let patterns = self.pending_patterns.len();
        let in_literal = std::mem::replace(&mut self.in_expression_tree, false);
        let r = self.parse_statement_inner();
        self.leave();
        self.leave();
        self.in_expression_tree = in_literal;
        self.stmt_doc = saved_doc;
        if self.pending_patterns.len() > patterns {
            self.report_pending_patterns(patterns);
        }
        if let Ok(stmt) = &r {
            self.note_statement(stmt);
        }
        // A statement that needs checking makes the ones around it need
        // walking too.
        self.interest |= outer_interest;
        self.assigns = outer_assigns;
        self.marks |= outer_marks;
        r
    }

    fn parse_statement_inner(&mut self) -> R<Stmt<'a>> {
        self.check_case_of_statement();
        let start = self.tok.start;
        let kind = self.tok.kind;
        match kind {
            T::LBrace => {
                let block = self.parse_compound_statement()?;
                Ok(self.stmt(start, StmtKind::Block(block)))
            }
            T::If => self.parse_if(),
            T::While => {
                self.bump();
                let cond = self.parse_paren_expr()?;
                let body = self.parse_body()?;
                self.c_node(K::WhileStatement);
                Ok(self.stmt(start, StmtKind::While(self.alloc(While { cond, body }))))
            }
            T::Do => {
                self.bump();
                let body = self.parse_body()?;
                self.require(T::While);
                let cond = self.parse_paren_expr()?;
                self.expect_semicolon()?;
                self.c_node(K::DoStatement);
                Ok(self.stmt(start, StmtKind::Do(self.alloc(While { cond, body }))))
            }
            T::For => self.parse_for(),
            T::Foreach => self.parse_foreach(),
            T::Switch => self.parse_switch(),
            T::Match => self.parse_match(),
            T::Try => self.parse_try(),
            T::Using => {
                self.c_missing();
                self.parse_using(start, false)
            }
            T::Await if self.peek_kind() == T::Using => {
                self.bump();
                self.parse_using(start, true)
            }
            T::Break => {
                self.bump();
                self.expect_semicolon()?;
                self.c_node(K::BreakStatement);
                Ok(self.stmt(start, StmtKind::Break))
            }
            T::Continue => {
                self.bump();
                self.expect_semicolon()?;
                self.c_node(K::ContinueStatement);
                Ok(self.stmt(start, StmtKind::Continue))
            }
            T::Return => {
                self.bump();
                let value = if self.at(T::Semicolon) {
                    self.c_missing();
                    None
                } else {
                    let value_start = self.tok.start;
                    let e = self.parse_root_expr()?;
                    if matches!(e.kind, ExprKind::List(_)) && e.span.start == value_start {
                        self.report(
                            e.span.start,
                            e.span.end,
                            "`list()` can only be used as an lvar. Did you mean to use `tuple()`?",
                        );
                    }
                    Some(self.alloc(e))
                };
                self.expect_semicolon()?;
                self.c_node(K::ReturnStatement);
                Ok(self.stmt(start, StmtKind::Return(value)))
            }
            T::Throw => {
                self.bump();
                let e = self.parse_root_expr()?;
                self.expect_semicolon()?;
                self.c_node(K::ThrowStatement);
                Ok(self.stmt(start, StmtKind::Throw(self.alloc(e))))
            }
            T::Yield if self.peek_kind() == T::Break => {
                self.bump();
                self.bump();
                self.expect_semicolon()?;
                // A `yield break` makes the function a generator too.
                self.yield_seen += 1;
                self.yield_ok += 1;
                self.c_node(K::YieldBreakStatement);
                Ok(self.stmt(start, StmtKind::YieldBreak))
            }
            T::Echo => {
                let keyword = Self::tok_span(self.take());
                let mut args = self.vec();
                let expressions = self.c_mark();
                loop {
                    args.push(self.parse_root_expr()?);
                    if !self.eat_separator(T::Comma) {
                        break;
                    }
                }
                self.c_list(expressions);
                self.expect_semicolon()?;
                self.c_node(K::EchoStatement);
                let echo = Echo {
                    keyword,
                    args: args.into_bump_slice(),
                };
                Ok(self.stmt(start, StmtKind::Echo(self.alloc(echo))))
            }
            T::Unset => {
                let keyword = Self::tok_span(self.take());
                self.require(T::LParen);
                let mut args = self.vec();
                let variables = self.c_mark();
                while !self.at(T::RParen) {
                    let arg = self.parse_expr()?;
                    self.check_unset_target(&arg);
                    args.push(arg);
                    if !self.eat_separator(T::Comma) {
                        break;
                    }
                }
                self.c_list(variables);
                self.require(T::RParen);
                self.expect_semicolon()?;
                self.c_node(K::UnsetStatement);
                let unset = Echo {
                    keyword,
                    args: args.into_bump_slice(),
                };
                Ok(self.stmt(start, StmtKind::Unset(self.alloc(unset))))
            }
            T::Concurrent => {
                self.bump();
                // HHVM's parser takes any statement; one that is not a
                // block is for its checks to reject.
                let block = self.parse_body()?;
                self.c_node(K::ConcurrentStatement);
                Ok(self.stmt(start, StmtKind::Concurrent(block)))
            }
            T::Semicolon => {
                // An expression statement without an expression.
                self.c_missing();
                self.bump();
                self.c_node(K::ExpressionStatement);
                Ok(self.stmt(start, StmtKind::Noop))
            }
            // Declarations and constructs HHVM handles specially (or rejects)
            // in statement position.
            T::Abstract | T::Final | T::Interface | T::Trait | T::Xhp | T::Class => {
                self.parse_nested_class()
            }
            T::Fallthrough if self.peek_kind() == T::Semicolon => {
                self.report_syntax(
                    self.tok.start,
                    self.tok.end,
                    "A fallthrough directive can only appear at the end of a `switch` section.",
                );
                self.bump();
                self.bump();
                self.c_node(K::SwitchFallthrough);
                Ok(self.declaration_statement(start))
            }
            T::Case | T::Default => {
                // A label on its own, which HHVM parses and then has no
                // statement to translate to.
                let message = if self.take().kind == T::Case {
                    let untranslated = self.begin_untranslated();
                    let label = self.parse_expr();
                    self.end_untranslated(untranslated);
                    label?;
                    "A `case` statement may only appear directly inside a `switch`."
                } else {
                    "A `default` statement may only appear directly inside a `switch`."
                };
                if !self.eat(T::Semicolon) {
                    self.require(T::Colon);
                }
                self.c_node(if kind == T::Case {
                    K::CaseLabel
                } else {
                    K::DefaultLabel
                });
                self.report_missing(message);
                Ok(self.declaration_statement(start))
            }
            T::Function if self.peek_kind() != T::LParen => self.parse_nested_function(),
            T::RParen | T::Public | T::Protected | T::Private | T::Internal
                if self.expects(kind) =>
            {
                Ok(self.missing_statement())
            }
            T::Async => {
                // `async function name()` would be a nested declaration.
                let snap = self.snapshot();
                self.bump();
                let nested = self.at(T::Function) && self.peek_kind() != T::LParen;
                self.restore(snap);
                if nested {
                    return self.parse_nested_function();
                }
                self.parse_expression_statement(start)
            }
            _ => self.parse_expression_statement(start),
        }
    }

    fn parse_expression_statement(&mut self, start: u32) -> R<Stmt<'a>> {
        // `Foo $x = ...;` looks like a typed local variable.
        let type_like = matches!(
            self.tok.kind,
            T::Name
                | T::Darray
                | T::Dict
                | T::Int_
                | T::Keyset
                | T::Shape
                | T::String_
                | T::Varray
                | T::Vec
        );
        let scope = self.enter_scope(Expected::SEMICOLON);
        let e = self.parse_root_expr()?;
        if type_like && self.at(T::Variable) {
            self.missing_semicolon("Local variables cannot have type annotations in Hack.")?;
        } else {
            self.expect_semicolon()?;
        }
        self.leave_scope(scope);
        self.c_node(K::ExpressionStatement);
        if self.absent(&e) {
            return Ok(self.stmt(start, StmtKind::Noop));
        }
        // `yield` is only valid as the whole statement or as the right-hand
        // side of a plain assignment that is the whole statement.
        match e.kind {
            ExprKind::Yield(_) => self.accept_yield(e.span.start),
            ExprKind::Assign(a) if a.op.is_none() && matches!(a.rhs.kind, ExprKind::Yield(_)) => {
                self.accept_yield(a.rhs.span.start)
            }
            _ => {}
        }
        Ok(self.stmt(start, StmtKind::Expr(self.alloc(e))))
    }

    fn parse_if(&mut self) -> R<Stmt<'a>> {
        let start = self.tok.start;
        self.require(T::If);
        let cond = self.parse_paren_root()?;
        let then = self.parse_body()?;
        let otherwise = if self.eat(T::Else) {
            if self.at(T::If) {
                // `else if`: the nested `if` is the sole statement of the
                // else branch and starts at its own `if` keyword.
                let saved_doc = self.stmt_doc;
                self.stmt_doc = Some((self.tok.trivia_start, self.tok.start));
                self.enter()?;
                let (outer_interest, outer_assigns) = (self.interest, self.assigns);
                let outer_marks = std::mem::take(&mut self.marks);
                self.interest = false;
                self.assigns = 0;
                let nested = self.parse_if();
                if let Ok(stmt) = &nested {
                    self.note_statement(stmt);
                }
                self.interest |= outer_interest;
                self.assigns = outer_assigns;
                self.marks |= outer_marks;
                self.leave();
                self.stmt_doc = saved_doc;
                let nested = nested?;
                self.c_node(K::ElseClause);
                Some(&*self.arena.alloc_slice_copy(&[nested]))
            } else {
                let body = self.parse_body()?;
                self.c_node(K::ElseClause);
                Some(body)
            }
        } else {
            self.c_missing();
            None
        };
        self.c_node(K::IfStatement);
        let node = If {
            cond,
            then,
            otherwise,
        };
        Ok(self.stmt(start, StmtKind::If(self.alloc(node))))
    }

    fn parse_for(&mut self) -> R<Stmt<'a>> {
        let start = self.tok.start;
        self.bump();
        self.require(T::LParen);
        let mut init = self.vec();
        let initializer = self.c_mark();
        while !self.at(T::Semicolon) {
            init.push(self.parse_root_expr()?);
            if !self.eat_separator(T::Comma) {
                break;
            }
        }
        self.c_list(initializer);
        self.expect_semicolon()?;
        let cond = if self.at(T::Semicolon) {
            self.c_missing();
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect_semicolon()?;
        let mut step = self.vec();
        let end_of_loop = self.c_mark();
        while !self.at(T::RParen) {
            step.push(self.parse_expr()?);
            if !self.eat_separator(T::Comma) {
                break;
            }
        }
        self.c_list(end_of_loop);
        self.require(T::RParen);
        let body = self.parse_body()?;
        self.c_node(K::ForStatement);
        let node = For {
            init: init.into_bump_slice(),
            cond,
            step: step.into_bump_slice(),
            body,
        };
        Ok(self.stmt(start, StmtKind::For(self.alloc(node))))
    }

    fn parse_foreach(&mut self) -> R<Stmt<'a>> {
        let start = self.tok.start;
        self.bump();
        self.require(T::LParen);
        let scope = self.enter_scope(Expected::RIGHT_PAREN);
        self.await_ok_at = self.tok.start;
        let collection = self.parse_expr_no_as()?;
        let await_span = if self.at(T::Await) {
            Some(Self::tok_span(self.take()))
        } else {
            self.c_missing();
            None
        };
        self.require(T::As);
        let targets = self.c_mark();
        let first = self.parse_expr_term()?;
        let (key, value) = if self.eat(T::FatArrow) {
            (Some(first.e), self.parse_expr_term()?)
        } else if self.at(T::RParen) {
            // No key and no arrow in front of the value.
            self.c_insert_missing_n(targets, 2);
            (None, first)
        } else {
            // HHVM takes whatever stands there for the arrow.
            self.report_missing("An arrow `=>` or right parenthesis `)` is expected here.");
            self.bump();
            self.c_node(K::ErrorSyntax);
            (Some(first.e), self.parse_expr_term()?)
        };
        if let Some(key) = &key {
            self.check_pattern(key, false);
        }
        let direct = value.start == value.e.span.start && value.end == value.e.span.end;
        let value = value.e;
        if matches!(value.kind, ExprKind::List(items) if items.is_empty()) {
            self.report(value.span.start, value.span.end, "Cannot use empty list");
        }
        self.check_pattern(&value, direct);
        self.check_lvalue(&value, LvalRoot::Foreach, Nesting::None);
        if let Some(key) = &key {
            self.check_lvalue(key, LvalRoot::Foreach, Nesting::None);
        }
        self.require(T::RParen);
        self.leave_scope(scope);
        let body = self.parse_body()?;
        self.c_node(K::ForeachStatement);
        let node = Foreach {
            collection,
            await_span,
            key,
            value,
            body,
        };
        Ok(self.stmt(start, StmtKind::Foreach(self.alloc(node))))
    }

    fn parse_switch(&mut self) -> R<Stmt<'a>> {
        let start = self.tok.start;
        self.bump();
        let subject = self.parse_paren_root()?;
        self.require(T::LBrace);
        let mut cases = self.vec::<Case<'a>>();
        let mut seen_default = false;
        // A label follows a `default`.
        let mut misplaced_default = false;
        let sections = self.c_mark();
        while !self.at(T::RBrace) && !self.at(T::Eof) {
            // One section: labels, statements, optional fallthrough marker.
            let section_start = self.tok.start;
            let first_label = cases.len();
            let mut unlabelled = false;
            let labels = self.c_mark();
            loop {
                let label_start = self.tok.start;
                let label = match self.tok.kind {
                    T::Case => {
                        self.bump();
                        Some(self.parse_expr()?)
                    }
                    T::Default => {
                        self.bump();
                        None
                    }
                    _ => break,
                };
                if self.at(T::Semicolon) {
                    // `case X;` is accepted with a complaint.
                    let semi = self.take();
                    self.report(semi.start, semi.end, "A colon `:` is expected here.");
                } else {
                    self.require(T::Colon);
                }
                self.c_node(if label.is_some() {
                    K::CaseLabel
                } else {
                    K::DefaultLabel
                });
                if seen_default {
                    misplaced_default = true;
                }
                seen_default |= label.is_none();
                cases.push(Case {
                    label,
                    label_span: self.span_from(label_start),
                    body: &[],
                    fallthrough: true,
                });
            }
            if cases.len() == first_label {
                // The statements make a section of their own.
                self.report_missing(
                    "The first statement inside a `switch` statement must be a `case` or `default` label statement.",
                );
                unlabelled = true;
                cases.push(Case {
                    label: None,
                    label_span: Span::new(section_start, section_start),
                    body: &[],
                    fallthrough: true,
                });
            }
            self.c_list(labels);
            let mut stmts = self.vec();
            let statements = self.c_mark();
            while !matches!(self.tok.kind, T::Case | T::Default | T::RBrace | T::Eof) {
                if self.at(T::Fallthrough) && self.peek_kind() == T::Semicolon {
                    break;
                }
                let stmt_start = self.tok.start;
                stmts.push(self.parse_statement()?);
                if self.tok.start == stmt_start {
                    break;
                }
            }
            self.c_list(statements);
            let fallthrough = if self.at(T::Fallthrough) && self.peek_kind() == T::Semicolon {
                self.bump();
                self.bump();
                self.c_node(K::SwitchFallthrough);
                true
            } else {
                // A `// FALLTHROUGH` comment stands for the statement, which
                // then has neither keyword nor semicolon.
                if self.tok.leading_fallthrough {
                    self.c_missing_n(2);
                    self.c_node(K::SwitchFallthrough);
                } else {
                    self.c_missing();
                }
                self.tok.leading_fallthrough
            };
            self.c_node(K::SwitchSection);
            let last = cases.last_mut().unwrap();
            last.body = stmts.into_bump_slice();
            last.fallthrough = fallthrough;
            if unlabelled {
                // HHVM's translation to the AST has nowhere to put the
                // statements.
                let (start, end) = if self.prev_end > section_start {
                    (section_start, self.prev_end)
                } else {
                    let at = self.missing_pos();
                    (at, at + 1)
                };
                self.report_lowering(start, end, "Malformed block result");
            }
            if self.tok.start == section_start {
                break;
            }
        }
        self.c_list(sections);
        if cases.is_empty() {
            self.report_missing(
                "`switch` statements need to have at least one `case` or a `default` block",
            );
        }
        self.require(T::RBrace);
        self.c_node(K::SwitchStatement);
        if misplaced_default {
            self.check_switch_defaults(&cases);
        }
        let node = Switch {
            subject,
            cases: cases.into_bump_slice(),
        };
        Ok(self.stmt(start, StmtKind::Switch(self.alloc(node))))
    }

    fn parse_try(&mut self) -> R<Stmt<'a>> {
        let start = self.tok.start;
        self.bump();
        let body = self.parse_compound_statement()?;
        let mut catches = self.vec();
        let catch_clauses = self.c_mark();
        while self.at(T::Catch) {
            let catch_start = self.take().start;
            self.require(T::LParen);
            // Any type may be written; the exception is caught by whatever
            // class is called the way the type is spelled.
            let hint = self.parse_hint_ex(false)?;
            let class = Id {
                span: hint.span,
                name: match hint.kind {
                    HintKind::Name(name) => name,
                    _ => hint.span.text(self.src),
                },
            };
            let var = self.expect_variable()?;
            self.require(T::RParen);
            let body = self.parse_compound_statement()?;
            self.c_node(K::CatchClause);
            if var.name == "$this" {
                self.report_from(
                    catch_start,
                    "You cannot use this syntax where a `catch` variable is expected.",
                );
            }
            catches.push(Catch { class, var, body });
        }
        self.c_list(catch_clauses);
        let finally = if self.eat(T::Finally) {
            let block = self.parse_compound_statement()?;
            self.c_node(K::FinallyClause);
            for stmt in block {
                let word = match stmt.kind {
                    StmtKind::Break => "break",
                    StmtKind::Continue => "continue",
                    _ => continue,
                };
                self.report_lowering(
                    stmt.span.start,
                    stmt.span.end,
                    format!(
                        "`{};` is not allowed as a top-level statement in a `finally` block",
                        word
                    ),
                );
            }
            Some(block)
        } else {
            self.c_missing();
            None
        };
        self.c_node(K::TryStatement);
        if catches.is_empty() && finally.is_none() {
            self.report_from(
                start,
                "A `try` statement requires a `catch` or a `finally` clause.",
            );
        }
        let node = Try {
            body,
            catches: catches.into_bump_slice(),
            finally,
        };
        Ok(self.stmt(start, StmtKind::Try(self.alloc(node))))
    }

    /// `using` statement; the `await`, if any, has been consumed.
    fn parse_using(&mut self, start: u32, has_await: bool) -> R<Stmt<'a>> {
        self.require(T::Using);
        // `using <expr>;` applies to the rest of the enclosing block. What
        // is in parentheses is the whole expression or else the list of a
        // `using` with a body.
        // HHVM reads an expression, syntax errors and all, and looks at
        // what follows it.
        let snap = self.snapshot();
        let term = if self.at(T::LParen) {
            self.parse_cast_or_parenthesized_or_lambda()
        } else {
            self.parse_expr_term()
        };
        let function_scoped = match term {
            Ok(term) if self.at(T::Semicolon) => Some(term),
            _ => {
                self.restore(snap);
                None
            }
        };
        if let Some(term) = function_scoped {
            self.expect_semicolon()?;
            self.c_node(K::UsingStatementFunctionScoped);
            let node = Using {
                has_await,
                exprs_span: Span::new(term.start, term.end),
                exprs: self.arena.alloc_slice_copy(&[term.e]),
                body: None,
                enclosing_block: Span::NONE,
            };
            return Ok(self.stmt(start, StmtKind::Using(self.alloc(node))));
        }
        self.require(T::LParen);
        let mut exprs = self.vec();
        let mut exprs_span = Span::new(self.tok.start, self.tok.start);
        let expressions = self.c_mark();
        loop {
            if matches!(self.tok.kind, T::Comma | T::RParen | T::Eof) {
                // No expression where one is due.
                let goes_on = self.missing_list_item("An expression is expected here.");
                self.missing_expression();
                if goes_on {
                    continue;
                }
                break;
            }
            let term = self.parse_expr_term()?;
            exprs_span.end = term.end;
            exprs.push(term.e);
            if !self.eat_separator(T::Comma) {
                break;
            }
        }
        self.c_list(expressions);
        self.require(T::RParen);
        let body = self.parse_body()?;
        self.c_node(K::UsingStatementBlockScoped);
        let node = Using {
            has_await,
            exprs_span,
            exprs: exprs.into_bump_slice(),
            body: Some(body),
            enclosing_block: Span::NONE,
        };
        Ok(self.stmt(start, StmtKind::Using(self.alloc(node))))
    }
}
