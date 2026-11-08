//! Recursive-descent parser producing [`crate::ast`] directly from source.
//!
//! The grammar decisions (operator precedence, where `<` starts generic
//! arguments, what counts as a lambda, ...) follow HHVM's parser so that the
//! resulting tree agrees with what HHVM's lowerer produces. So does what
//! happens at a syntax error: HHVM's parser reports it and carries on, and
//! `repair.rs` does the same where it knows how, which is what keeps the
//! errors reported for a broken file (and its lossless tree) the same as
//! HHVM's. Where it does not, the parse fails at that point.

use bumpalo::Bump;
use smallvec::SmallVec;

use crate::ast::*;
use crate::checks::{ClassInfo, Feature, FileState, MemberName, Names};
use crate::cst::{SyntaxKind as K, TokenKind};
use crate::lexer::{Lexer, Token};
use crate::span::Span;
use crate::token::T;
use cst::Trailing;

mod cst;
pub(crate) mod decl;
mod destructure;
mod expr;
mod expr_trees;
mod modules;
mod patterns;
mod recovery;
pub(crate) mod repair;
mod stmt;
mod strings;
mod types;
mod xhp;

/// A list under construction. Elements collect on the stack (spilling to the
/// heap for long lists) and are copied into the arena once, at their final
/// size, so the tree holds no slack and no abandoned growth buffers.
pub(crate) struct List<'a, T> {
    items: SmallVec<[T; 8]>,
    arena: &'a Bump,
}

impl<'a, T: Copy> List<'a, T> {
    #[inline]
    pub(crate) fn push(&mut self, item: T) {
        self.items.push(item);
    }

    /// Drops the items after the first `len`.
    pub(crate) fn truncate(&mut self, len: usize) {
        self.items.truncate(len);
    }

    #[inline]
    pub(crate) fn into_bump_slice(self) -> &'a [T] {
        if self.items.is_empty() {
            &[]
        } else {
            self.arena.alloc_slice_copy(&self.items)
        }
    }
}

impl<T> std::ops::Deref for List<'_, T> {
    type Target = [T];
    #[inline]
    fn deref(&self) -> &[T] {
        &self.items
    }
}

impl<T> std::ops::DerefMut for List<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.items
    }
}

/// What a file's extension says about how HHVM reads it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum FileKind {
    /// No extension HHVM has rules for.
    #[default]
    Other,
    /// `.php`: the file has to start with `<?hh`.
    Php,
    /// `.hack` (or `.hackpartial`, the extension kept here): no `<?hh`.
    Hack(&'static str),
    /// `.hhi`: a declaration file.
    Hhi,
}

impl FileKind {
    pub(crate) fn of(path: &str) -> FileKind {
        match path.rsplit_once('.').map(|(_, extension)| extension) {
            Some("php") => FileKind::Php,
            Some("hack") => FileKind::Hack("hack"),
            Some("hackpartial") => FileKind::Hack("hackpartial"),
            Some("hhi") => FileKind::Hhi,
            _ => FileKind::Other,
        }
    }
}

/// The kinds of token HHVM's parser keeps track of as wanted later by a
/// construct it is in the middle of, as bits.
pub(crate) struct Expected;

impl Expected {
    /// `class`, `trait`, `interface`: by any declaration.
    pub(crate) const CLASSISH: u8 = 1;
    /// `;`: by an expression statement.
    pub(crate) const SEMICOLON: u8 = 2;
    /// `)`: by the head of a `foreach`.
    pub(crate) const RIGHT_PAREN: u8 = 4;
    /// `public`, `protected`, `private`, `internal`: by a class body.
    pub(crate) const VISIBILITY: u8 = 8;

    fn bit(kind: T) -> u8 {
        match kind {
            T::Class | T::Trait | T::Interface => Self::CLASSISH,
            T::Semicolon => Self::SEMICOLON,
            T::RParen => Self::RIGHT_PAREN,
            T::Public | T::Protected | T::Private | T::Internal => Self::VISIBILITY,
            _ => 0,
        }
    }
}

/// Marker for "this file needs the fallback parser". Details are kept on the
/// parser so that `Result<T, Fail>` stays as small as `T`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Fail;

pub(crate) type R<T> = Result<T, Fail>;

pub(crate) struct Parser<'a> {
    pub(crate) arena: &'a Bump,
    pub(crate) src: &'a str,
    pub(crate) bytes: &'a [u8],
    pub(crate) lexer: Lexer<'a>,
    /// The current token.
    pub(crate) tok: Token,
    /// End offset of the most recently consumed token.
    pub(crate) prev_end: u32,
    /// Start offset of that token (the same as `prev_end` after text the
    /// lexer does not scan itself).
    pub(crate) prev_start: u32,
    /// The token after `tok` (and the lexer position after it), if peeked.
    peeked: Option<(Token, usize)>,
    pub(crate) error: Option<(u32, &'static str)>,
    /// Where the text that error is about ends.
    pub(crate) error_end: u32,

    /// Binding power of the operator whose operand is being parsed.
    pub(crate) precedence: u8,
    /// `as` is not an operator inside `foreach (... as ...)`.
    pub(crate) allow_as: bool,

    /// Number of `yield`s seen in the function body being parsed.
    pub(crate) yield_seen: u32,
    /// Number of those sitting in a position HHVM accepts.
    pub(crate) yield_ok: u32,
    /// The `yield` expressions of the functions being parsed that have not
    /// turned out to be in such a position (yet): their extent, and where
    /// among the errors of the translation to the AST a complaint about
    /// them belongs (`None` if HHVM gave up on the declaration before).
    pub(crate) yields: Vec<(u32, u32, Option<usize>)>,
    /// How many of those there were when the declaration (or class member)
    /// being parsed started.
    pub(crate) unit_yields: usize,

    /// Nesting depth of compound statements, and the source ranges of the
    /// outermost ones. `HH_FIXME`s only count inside a compound statement.
    block_depth: u32,
    block_start: u32,
    pub(crate) block_ranges: Vec<(u32, u32)>,

    /// `(trivia_start, token_start)` of the first token of the statement
    /// being parsed; anonymous functions inherit its doc comment.
    pub(crate) stmt_doc: Option<(u32, u32)>,

    /// Errors that do not stop the parse (see [`crate::SyntaxError`]).
    pub(crate) errors: Vec<crate::SyntaxError>,

    /// The file uses `readonly` somewhere HHVM cares about (declaration
    /// modifiers, parameters, return types, `readonly` expressions,
    /// `HH\Readonly\as_mut`). HHVM only runs its readonly analysis on such
    /// files.
    pub(crate) uses_readonly: bool,

    /// The unstable features (see [`Feature`]) the file has enabled so far.
    pub(crate) features: u32,

    /// The lossless syntax tree under construction, when recording one
    /// (see `cst.rs`).
    pub(crate) cst: Option<Box<crate::cst::Builder>>,
    /// Parsing a file that already failed to parse once, to salvage its
    /// declarations (see `recovery.rs`).
    pub(crate) recovering: bool,
    /// While salvaging: the declaration (or class member) being parsed has
    /// a syntax error, so it is to be dropped.
    pub(crate) salvage_failed: bool,
    /// Trying out a reading of the code that is given up at the first
    /// syntax error (see [`speculate`](Self::speculate)).
    speculating: bool,
    /// The file has an `.hhi` extension; HHVM relaxes a few rules there.
    pub(crate) hhi: bool,
    /// What the file's extension is.
    pub(crate) kind: FileKind,
    pub(crate) options: crate::ParserOptions,
    /// The file's path, for the messages that name it.
    pub(crate) path: &'a str,
    /// An `.hhi` file with a `<?hh` header. HHVM does not look inside
    /// function bodies in such a file, so nothing in them is an error.
    pub(crate) decl_mode: bool,
    /// A `module name;` declaration has been seen.
    pub(crate) in_module: bool,
    /// Parsing a function declared inside a function body (see
    /// `repair.rs`).
    pub(crate) nested_function: bool,
    /// The `reify` type parameters in the type parameter list parsed last:
    /// their place in the list and their extent.
    pub(crate) reified_tparams: Vec<(usize, u32, u32)>,
    /// The extents of the `__SimpliHack` attributes in the attribute list
    /// parsed last.
    pub(crate) simplihack_attrs: Vec<(u32, u32)>,
    /// The class, interface or trait whose members are being parsed.
    pub(crate) class: Option<ClassInfo<'a>>,
    /// Extent of the parameter list parsed last (first parameter up to the
    /// last one and any trailing comma).
    pub(crate) params_span: (u32, u32),
    /// The names declared so far in the class being parsed, for finding
    /// redeclarations once the class is complete.
    pub(crate) class_members: Vec<MemberName<'a>>,
    /// The `require` clauses of the class being parsed: the type as
    /// written and the kind of requirement.
    pub(crate) class_requires: Vec<(&'a str, RequireKind)>,
    /// The class being parsed has an `abstract` method.
    pub(crate) class_has_abstract_method: bool,
    /// The promoted parameters of the constructor of the class being parsed
    /// and the extent of its parameter list.
    pub(crate) ctor_promotion: Option<((u32, u32), &'a [Param<'a>])>,
    /// The enclosing namespace as HHVM's error checker tracks it: `\` at the
    /// top level, `\Foo\Bar` inside `namespace Foo\Bar`.
    pub(crate) ns_name: String,
    /// What has been seen at the top level of the file so far.
    pub(crate) file: FileState,
    /// The names declared and imported in the current namespace.
    pub(crate) names: Names<'a>,

    /// Set while parsing a statement (or initializer) when it contains
    /// something the checks that run after parsing have to look at.
    pub(crate) interest: bool,
    /// Assignments and `++` / `--` seen directly in the statement being
    /// parsed.
    pub(crate) assigns: u32,
    /// Whether the statement being parsed, the statements nested in it
    /// included, has an `await` (`MARK_AWAIT`) or an `inout` argument
    /// (`MARK_INOUT`).
    pub(crate) marks: u8,
    /// Where an `await` may start without its position needing a look: the
    /// start of an expression HHVM lets one be the root of.
    pub(crate) await_ok_at: u32,
    /// What the expression being checked is part of, innermost last.
    pub(crate) ancestors: Vec<crate::checks::Ancestor<'a>>,
    /// The statements around the one being checked that an `await` is
    /// counted as part of.
    pub(crate) await_roots: Vec<Stmt<'a>>,
    /// How many `concurrent` blocks the statement being checked is in.
    pub(crate) concurrent_depth: u32,
    /// Start offsets of the statements and initializers found interesting
    /// in the definition being parsed.
    pub(crate) flagged: Vec<u32>,
    /// Inside a function with a context list (or an enum body), where
    /// property writes and static property accesses may be errors.
    pub(crate) strict_co: bool,
    /// Inside a static method.
    pub(crate) fn_static: bool,
    /// Parsing the right-hand side of a pipe, where `$$` is defined.
    pub(crate) pipe_ok: bool,
    /// Parsing the expression between the backticks of an expression tree
    /// literal, where a name in front of a backtick is not the start of
    /// another literal. Blocks and parameter lists in there do not count.
    pub(crate) in_expression_tree: bool,
    /// How many expression tree literals enclose what is being parsed, and
    /// whether it is the code of the innermost one (as opposed to a splice
    /// in it). Closures outside such code start afresh.
    pub(crate) et_depth: u32,
    pub(crate) et_active: bool,
    /// Inside an expression tree literal, however deep in closures.
    pub(crate) et_enclosed: bool,
    /// The `shape(...)` and `tuple(...)` expressions of the statement being
    /// parsed that are only valid as destructuring patterns and have not
    /// been used as one yet (see `destructure.rs`).
    pub(crate) pending_patterns: Vec<Expr<'a>>,

    /// HHVM turns the syntax tree into its AST one declaration (or class
    /// member) at a time and gives up on a declaration at the first thing
    /// it cannot translate. These fields track that, because it decides
    /// which of the errors found there HHVM reports: see
    /// [`report_lowering`](Self::report_lowering).
    ///
    /// The parser has recovered from a syntax error and reported it.
    pub(crate) syntax_error_reported: bool,
    /// The errors HHVM's parser reports while recovering, in the order it
    /// finds them. Of these HHVM only passes on the first by position:
    /// [`settle_syntax_errors`](Self::settle_syntax_errors) adds that one
    /// to `errors`.
    pub(crate) syntax_errors: Vec<crate::SyntaxError>,
    /// Where that first error is and what it says (meaningful once the
    /// errors are settled).
    pub(crate) syntax_error: (u32, &'static str),
    /// Which of `syntax_errors` (by index) are those of HHVM's lexer rather
    /// than its parser: of errors about the same text, HHVM puts the
    /// lexer's first.
    pub(crate) lexical_errors: Vec<u32>,
    /// Where the token consumed last ends, if HHVM reads that token
    /// without what trails it (`u32::MAX` otherwise): its lexer has not
    /// gone past the token then, which decides where an error about
    /// something missing after it goes.
    pub(crate) no_trailing: u32,
    /// Where in `lowering_list` the error is that HHVM's translation to
    /// the AST only reports because it has reported nothing before
    /// (what makes it give up on a declaration, see
    /// [`abort_lowering`](Self::abort_lowering), and what it finds
    /// missing), `usize::MAX` if there is none. It goes if an error turns
    /// out to come before it.
    pub(crate) first_only_at: usize,
    /// The kinds of token HHVM's parser knows to be wanted further on by
    /// one of the constructs being parsed (see [`Expected`]). Recovery
    /// leaves such tokens alone.
    pub(crate) expected: u8,
    /// An error of that translation step has been reported for this file.
    pub(crate) lowering_errors: bool,
    /// HHVM has given up on the declaration being parsed.
    pub(crate) unit_aborted: bool,
    /// Where the declaration being parsed starts.
    pub(crate) unit_start: u32,
    /// The declarations HHVM gave up on.
    pub(crate) aborted_units: Vec<(u32, u32)>,
    /// Errors HHVM finds by inspecting its AST, with the rank of the pass
    /// that finds them; they are dropped for the declarations it gave up
    /// on.
    pub(crate) tree_errors: Vec<(u8, crate::SyntaxError)>,
    /// Errors of the translation to the AST, in the order found. HHVM
    /// reports them after all others.
    pub(crate) lowering_list: Vec<crate::SyntaxError>,
    /// Where in that list the complaint about the missing expression
    /// parsed last is, if it led to one (see `repair.rs`).
    pub(crate) missing_reported: Option<usize>,

    /// How deep in the tree the parser is, by the count [`MAX_DEPTH`]
    /// limits.
    depth: u32,
    /// What a level counts for at the moment (see [`MAX_DEPTH`]).
    pub(crate) depth_step: u32,
}

pub(crate) struct Snapshot {
    tok: Token,
    prev_start: u32,
    prev_end: u32,
    peeked: Option<(Token, usize)>,
    lexer_pos: usize,
    precedence: u8,
    allow_as: bool,
    yield_seen: u32,
    yield_ok: u32,
    yields_len: usize,
    block_depth: u32,
    block_start: u32,
    block_ranges_len: usize,
    depth: u32,
    stmt_doc: Option<(u32, u32)>,
    errors_len: usize,
    class_members_len: usize,
    interest: bool,
    assigns: u32,
    flagged_len: usize,
    lowering_errors: bool,
    syntax_error_reported: bool,
    syntax_errors_len: usize,
    no_trailing: u32,
    expected: u8,
    unit_aborted: bool,
    tree_errors_len: usize,
    lowering_list_len: usize,
    cst_len: usize,
    cst_full_end: u32,
    pending_patterns_len: usize,
}

/// See [`Parser::lowering_state`].
#[derive(Clone, Copy)]
pub(crate) struct LoweringState {
    tree_errors: usize,
    lowering_list: usize,
    lowering_errors: bool,
    unit_aborted: bool,
}

/// How deep the tree of a file may be before the parser gives up on it:
/// what works on the tree afterwards - the checks here, the translation to
/// the analysis tree, the analysis itself - recurses once per level, and a
/// thread's stack has room for only so many. Everything that puts one node
/// inside another counts, except for what [`MAX_OPERANDS`] is about; what
/// takes more stack than most counts more than once (statements, lambdas
/// and function types twice or more, anything in an expression tree
/// literal double). The limit keeps the deepest tree there can be within
/// half of the 2 MB a thread has by default.
const MAX_DEPTH: u32 = 400;

/// How many operands a run of left-associative binary operators (`$a . $b
/// . $c . ...`) may have. Such runs get much longer than anything nests
/// deep, and make a tree that leans left: one level per operator.
const MAX_OPERANDS: u32 = 2_000;

impl<'a> Parser<'a> {
    pub(crate) fn new(arena: &'a Bump, src: &'a str) -> Self {
        Parser {
            arena,
            src,
            bytes: src.as_bytes(),
            lexer: Lexer::new(src.as_bytes()),
            tok: Token::DUMMY,
            prev_end: 0,
            prev_start: 0,
            peeked: None,
            error: None,
            error_end: 0,
            precedence: 0,
            allow_as: true,
            yield_seen: 0,
            yield_ok: 0,
            yields: Vec::new(),
            unit_yields: 0,
            block_depth: 0,
            block_start: 0,
            block_ranges: Vec::new(),
            stmt_doc: None,
            errors: Vec::new(),
            uses_readonly: false,
            features: 0,
            cst: None,
            recovering: false,
            salvage_failed: false,
            speculating: false,
            hhi: false,
            kind: FileKind::Other,
            options: crate::ParserOptions::default(),
            path: "",
            decl_mode: false,
            in_module: false,
            nested_function: false,
            reified_tparams: Vec::new(),
            simplihack_attrs: Vec::new(),
            class: None,
            params_span: (0, 0),
            class_members: Vec::new(),
            class_requires: Vec::new(),
            class_has_abstract_method: false,
            ctor_promotion: None,
            ns_name: String::from("\\"),
            file: FileState::default(),
            names: Names::default(),
            interest: false,
            assigns: 0,
            marks: 0,
            await_ok_at: u32::MAX,
            ancestors: Vec::new(),
            await_roots: Vec::new(),
            concurrent_depth: 0,
            flagged: Vec::new(),
            strict_co: false,
            fn_static: false,
            pipe_ok: false,
            in_expression_tree: false,
            et_depth: 0,
            et_active: false,
            et_enclosed: false,
            pending_patterns: Vec::new(),
            syntax_error_reported: false,
            syntax_errors: Vec::new(),
            syntax_error: (0, ""),
            lexical_errors: Vec::new(),
            no_trailing: u32::MAX,
            first_only_at: usize::MAX,
            expected: 0,
            lowering_errors: false,
            unit_aborted: false,
            unit_start: 0,
            aborted_units: Vec::new(),
            tree_errors: Vec::new(),
            lowering_list: Vec::new(),
            missing_reported: None,
            depth: 0,
            depth_step: 1,
        }
    }

    // ------------------------------------------------------------------
    // Token plumbing

    #[inline]
    pub(crate) fn bump(&mut self) {
        self.c_push_current();
        self.advance();
    }

    /// Moves on to the next token.
    #[inline(always)]
    fn advance(&mut self) {
        self.prev_start = self.tok.start;
        self.prev_end = self.tok.end;
        match self.peeked.take() {
            Some((tok, pos)) => {
                self.tok = tok;
                self.lexer.pos = pos;
            }
            None => self.tok = self.lexer.next_token(),
        }
    }

    /// Drops the current token the way HHVM's recovery does: it is not
    /// part of any node, and in the lossless tree its text ends up among
    /// what leads the next token.
    #[cold]
    pub(crate) fn skip_token(&mut self) {
        self.whole_token(true);
        // HHVM's lexer reads it looking for keywords.
        if self.tok.mixed_case_kw || self.tok.mixed_case_soft {
            self.lowercase_keyword();
        }
        self.advance();
    }

    /// For where HHVM takes the current token whatever it is, or looks
    /// past it: the start of a string stands for the token HHVM's lexer
    /// reads there (see `whole_string_token`).
    #[inline]
    pub(crate) fn whole_token(&mut self, consumed: bool) {
        match self.tok.kind {
            T::DoubleQuoted | T::Heredoc | T::Nowdoc => self.whole_string_token(consumed),
            // A character no token starts with, which HHVM's lexer reports
            // as it reads it.
            T::Error if consumed && self.tok.end == self.tok.start + 1 => {
                let at = self.tok.start;
                self.report_lexical(at, at, "This character is invalid.");
            }
            _ => {}
        }
    }

    /// Drops the current token where HHVM reads one and does nothing with
    /// it: the lossless tree has no trace of it, not even among the trivia
    /// of the next token.
    #[cold]
    pub(crate) fn drop_token(&mut self) {
        let end = self.tok.end as usize;
        let full_end = end + crate::cst::trivia::trailing_width(self.bytes, end);
        if let Some(cst) = &mut self.cst {
            cst.full_end = full_end as u32;
        }
        self.advance();
    }

    /// Drops the current token with HHVM's complaint about it.
    #[cold]
    pub(crate) fn skip_unexpected_token(&mut self) {
        self.whole_token(true);
        // HHVM's lexer reads it looking for keywords.
        if self.tok.mixed_case_kw || self.tok.mixed_case_soft {
            self.lowercase_keyword();
        }
        let (start, end) = (self.tok.start, self.tok.end);
        let text = &self.src[start as usize..end as usize];
        self.report_syntax(
            start,
            end,
            format!("Encountered unexpected token `{text}`."),
        );
        self.skip_token();
    }

    /// Consumes a `kind` token where HHVM's parser requires one, and says
    /// whether it is there. If it is not, parsing carries on the way
    /// HHVM's does: see [`require_missing`](Self::require_missing).
    #[inline]
    pub(crate) fn require(&mut self, kind: T) -> bool {
        if self.tok.kind == kind {
            self.bump();
            true
        } else {
            self.require_missing(kind, expected_message(kind))
        }
    }

    /// HHVM's recovery for a required `kind` token that is not there. When
    /// the token after the current one is of that kind, the current one is
    /// dropped as unexpected and the next taken. Otherwise the required
    /// token is missing - `message` says so - and the current token stays,
    /// unless it looks like a misspelling of what is required, in which
    /// case it is dropped. Returns whether a token was consumed as the
    /// required one.
    #[cold]
    pub(crate) fn require_missing(&mut self, kind: T, message: &'static str) -> bool {
        self.whole_token(false);
        let next = self.peek();
        // Where every keyword is one, a name spelled like one is none.
        if next.kind == kind && (kind != T::Name || Self::is_plain_name(next)) {
            self.skip_unexpected_token();
            self.bump();
            return true;
        }
        let (start, end) = (self.tok.start, self.tok.end);
        let text = &self.src[start as usize..end as usize];
        if let Some(required) = spelling(kind).filter(|required| one_byte_apart(required, text)) {
            self.report_syntax(
                start,
                end,
                format!("Encountered unexpected token `{text}`. Did you mean `{required}`?"),
            );
            self.skip_token();
        } else {
            self.report_missing(message);
        }
        self.c_missing();
        false
    }

    /// Enters a construct that wants a token of one of the `expected`
    /// kinds (see [`Expected`]) further on. The result goes to
    /// [`leave_scope`](Self::leave_scope).
    #[inline]
    pub(crate) fn enter_scope(&mut self, expected: u8) -> u8 {
        let outer = self.expected;
        self.expected |= expected;
        outer
    }

    #[inline]
    pub(crate) fn leave_scope(&mut self, outer: u8) {
        self.expected = outer;
    }

    /// Whether a construct being parsed wants a `kind` token later.
    pub(crate) fn expects(&self, kind: T) -> bool {
        self.expected & Expected::bit(kind) != 0
    }

    /// Consumes the current token and returns it.
    #[inline]
    pub(crate) fn take(&mut self) -> Token {
        let tok = self.tok;
        self.bump();
        tok
    }

    #[inline]
    pub(crate) fn at(&self, kind: T) -> bool {
        self.tok.kind == kind
    }

    #[inline]
    pub(crate) fn eat(&mut self, kind: T) -> bool {
        if self.tok.kind == kind {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Like [`eat`](Self::eat) for the separator after a list item: the
    /// item ends here whether or not the separator is there.
    #[inline(always)]
    pub(crate) fn eat_separator(&mut self, kind: T) -> bool {
        let found = self.eat(kind);
        self.c_list_item(found);
        found
    }

    #[inline]
    pub(crate) fn expect(&mut self, kind: T) -> R<Token> {
        if self.tok.kind == kind {
            Ok(self.take())
        } else {
            self.fail_required(kind, expected_message(kind))
        }
    }

    /// Fails for want of a `kind` token, with the error HHVM's parser
    /// reports there: when the token after the current one is of that kind
    /// the current one is what is wrong, and so is one that looks like a
    /// misspelling of what is required; otherwise `message`, on the blank
    /// space in front of the current token.
    #[cold]
    pub(crate) fn fail_required<X>(&mut self, kind: T, message: &'static str) -> R<X> {
        if self.error.is_some() {
            return Err(Fail);
        }
        let (start, end) = (self.tok.start, self.tok.end);
        let text = &self.src[start as usize..end as usize];
        if self.peek_kind() == kind {
            self.report_syntax(
                start,
                end,
                format!("Encountered unexpected token `{text}`."),
            );
            return self.fail_span(start, end, UNEXPECTED_TOKEN);
        }
        if let Some(required) = spelling(kind).filter(|required| one_byte_apart(required, text)) {
            self.report_syntax(
                start,
                end,
                format!("Encountered unexpected token `{text}`. Did you mean `{required}`?"),
            );
            return self.fail_span(start, end, UNEXPECTED_TOKEN);
        }
        self.fail_expected(message)
    }

    /// Fails with `message` where HHVM's parser reports that something is
    /// missing: see [`missing_span`](Self::missing_span).
    #[cold]
    pub(crate) fn fail_expected<X>(&mut self, message: &'static str) -> R<X> {
        let (start, end) = self.missing_span();
        self.fail_span(start, end, message)
    }

    /// The extent HHVM gives an error about something missing in front of
    /// the current token: what its lexer scanned last, which is the tail of
    /// the trivia that follows the previous token on its line. That is the
    /// line break if there is one, nothing at all (the place where that
    /// trivia ends) if there is not, and when a `/* */` comment cuts the
    /// trivia short, the piece in front of the comment - the previous token
    /// itself if nothing lies between them.
    pub(crate) fn missing_span(&self) -> (u32, u32) {
        if self.prev_end == 0 {
            return (0, 0);
        }
        if self.prev_end == self.no_trailing {
            // HHVM's lexer stopped at the end of the token.
            return (self.prev_start, self.prev_end);
        }
        self.missing_span_after(self.prev_start, self.prev_end)
    }

    /// Where HHVM's lexer is once the token consumed last has been read:
    /// past what trails it, unless the token was read without (see
    /// `no_trailing`).
    pub(crate) fn hhvm_lexer_offset(&self) -> usize {
        if self.prev_end == self.no_trailing {
            self.prev_end as usize
        } else {
            self.leading_trivia_start(self.prev_end)
        }
    }

    /// [`missing_span`](Self::missing_span) for where the previous token
    /// is the text from `prev_start` to `prev_end`.
    pub(crate) fn missing_span_after(&self, prev_start: u32, prev_end: u32) -> (u32, u32) {
        use crate::cst::{TriviaKind, trivia};
        let mut start = prev_start as usize;
        let mut at = prev_end as usize;
        loop {
            let Some(piece) = trivia::scan(self.bytes, at) else {
                return (at as u32, at as u32);
            };
            match piece.kind {
                TriviaKind::EndOfLine => return (at as u32, (at + piece.width) as u32),
                // HHVM's trailing trivia, in which a block comment counts
                // (see `trivia::hhvm_trailing_width`).
                TriviaKind::FixMe | TriviaKind::Ignore | TriviaKind::IgnoreError => {
                    return (start as u32, at as u32);
                }
                _ => {
                    start = at;
                    at += piece.width;
                }
            }
        }
    }

    /// Consumes the `;` that ends a statement or declaration. A missing
    /// one is reported and parsing carries on, like HHVM does.
    #[inline]
    pub(crate) fn expect_semicolon(&mut self) -> R<()> {
        if self.tok.kind == T::Semicolon {
            self.bump();
            Ok(())
        } else {
            self.missing_semicolon("A semicolon `;` is expected here.")
        }
    }

    /// Reports a missing `;` with `message` and carries on.
    #[cold]
    pub(crate) fn missing_semicolon(&mut self, message: &'static str) -> R<()> {
        if self.recovering && self.peek_kind() != T::Semicolon {
            // A declaration is worth salvaging without its `;`.
            return Ok(());
        }
        self.require_missing(T::Semicolon, message);
        Ok(())
    }

    /// Records an error of the kind HHVM's parser itself reports while
    /// recovering.
    #[cold]
    pub(crate) fn report_syntax(
        &mut self,
        start: u32,
        end: u32,
        message: impl Into<std::borrow::Cow<'static, str>>,
    ) {
        self.syntax_error_reported = true;
        self.syntax_errors.push(crate::SyntaxError {
            start,
            end,
            message: message.into(),
        });
        if self.recovering || self.speculating {
            // What is being salvaged gets dropped, and what is being tried
            // is not it: there is no point in parsing the rest.
            self.salvage_failed = self.recovering;
            self.jump_to(self.bytes.len());
        }
    }

    /// Records an error of HHVM's lexer (see `lexical_errors`).
    #[cold]
    pub(crate) fn report_lexical(
        &mut self,
        start: u32,
        end: u32,
        message: impl Into<std::borrow::Cow<'static, str>>,
    ) {
        self.lexical_errors.push(self.syntax_errors.len() as u32);
        self.report_syntax(start, end, message);
    }

    /// Records that something is missing in front of the current token:
    /// `message`, where HHVM puts such an error (see
    /// [`missing_span`](Self::missing_span)).
    #[cold]
    pub(crate) fn report_missing(&mut self, message: &'static str) {
        let (start, end) = self.missing_span();
        self.report_syntax(start, end, message);
    }

    /// Orders the syntax errors the way HHVM does - by position, those
    /// found first in front, duplicates dropped - and passes the first on
    /// to `errors`: the others are only seen by whoever asks for the
    /// lossless tree.
    pub(crate) fn settle_syntax_errors(&mut self) {
        if self.syntax_errors.is_empty() {
            return;
        }
        if !self.lexical_errors.is_empty() {
            // The lexer's errors go in front of the parser's.
            let lexical = std::mem::take(&mut self.lexical_errors);
            let (mut ordered, mut parsers) = (Vec::new(), Vec::new());
            for (i, error) in std::mem::take(&mut self.syntax_errors)
                .into_iter()
                .enumerate()
            {
                if lexical.contains(&(i as u32)) {
                    ordered.push(error);
                } else {
                    parsers.push(error);
                }
            }
            ordered.append(&mut parsers);
            self.syntax_errors = ordered;
        }
        self.syntax_errors.sort_by_key(|e| (e.start, e.end));
        self.syntax_errors.dedup();
        let first = self.syntax_errors[0].clone();
        self.syntax_error = (
            first.start,
            match first.message {
                std::borrow::Cow::Borrowed(message) => message,
                std::borrow::Cow::Owned(_) => UNEXPECTED_TOKEN,
            },
        );
        self.errors.push(first);
    }

    /// The token after the current one.
    pub(crate) fn peek(&mut self) -> Token {
        if let Some((tok, _)) = self.peeked {
            return tok;
        }
        let saved = self.lexer.pos;
        let tok = self.lexer.next_token();
        self.peeked = Some((tok, self.lexer.pos));
        self.lexer.pos = saved;
        tok
    }

    #[inline]
    pub(crate) fn peek_kind(&mut self) -> T {
        self.peek().kind
    }

    /// Abandons the current token and continues lexing at `pos`. Used after
    /// scanning something the lexer does not handle itself (string bodies,
    /// XHP).
    pub(crate) fn resync(&mut self, pos: usize) {
        self.prev_start = pos as u32;
        self.prev_end = pos as u32;
        self.peeked = None;
        self.lexer.pos = pos;
        self.tok = self.lexer.next_token();
    }

    /// Replaces the current token by one of a different kind ending at `end`
    /// (used to re-interpret `:foo` as an XHP class name, to split `>>`, ...).
    pub(crate) fn retokenize(&mut self, kind: T, end: usize) {
        self.peeked = None;
        self.tok.kind = kind;
        self.tok.end = end as u32;
        self.tok.mixed_case_kw = false;
        self.tok.mixed_case_soft = false;
        self.lexer.pos = end;
    }

    /// Records an error that does not stop the parse.
    pub(crate) fn report(
        &mut self,
        start: u32,
        end: u32,
        message: impl Into<std::borrow::Cow<'static, str>>,
    ) {
        self.errors.push(crate::SyntaxError {
            start,
            end,
            message: message.into(),
        });
    }

    /// Records an error covering the text from `start` to the end of the
    /// last consumed token.
    #[cold]
    pub(crate) fn report_from(
        &mut self,
        start: u32,
        message: impl Into<std::borrow::Cow<'static, str>>,
    ) {
        self.report(start, self.prev_end, message);
    }

    /// Records an error HHVM reports while translating the syntax tree to
    /// its AST. Nothing is reported for a declaration it has already given
    /// up on.
    #[cold]
    pub(crate) fn report_lowering(
        &mut self,
        start: u32,
        end: u32,
        message: impl Into<std::borrow::Cow<'static, str>>,
    ) {
        if !self.unit_aborted {
            self.lowering_errors = true;
            self.lowering_list.push(crate::SyntaxError {
                start,
                end,
                message: message.into(),
            });
        }
    }

    /// Like [`report_lowering`](Self::report_lowering) for something that
    /// makes HHVM give up on the enclosing declaration. HHVM only reports
    /// such an error when the translation has not reported anything yet.
    #[cold]
    pub(crate) fn abort_lowering(
        &mut self,
        start: u32,
        end: u32,
        message: impl Into<std::borrow::Cow<'static, str>>,
    ) {
        if !self.unit_aborted {
            if !self.lowering_errors {
                self.first_only_at = self.lowering_list.len();
                self.lowering_list.push(crate::SyntaxError {
                    start,
                    end,
                    message: message.into(),
                });
            }
            self.lowering_errors = true;
            self.unit_aborted = true;
        }
    }

    /// [`report_lowering`](Self::report_lowering) for an error about
    /// something that starts at `start` and has not ended yet, which HHVM
    /// reports before it gets to what is still to come. The result goes to
    /// [`end_lowering_error`](Self::end_lowering_error) once the end is
    /// known.
    #[cold]
    pub(crate) fn begin_lowering_error(
        &mut self,
        start: u32,
        message: &'static str,
    ) -> Option<usize> {
        if self.unit_aborted {
            return None;
        }
        self.report_lowering(start, start, message);
        Some(self.lowering_list.len() - 1)
    }

    /// Completes an error begun with
    /// [`begin_lowering_error`](Self::begin_lowering_error) or
    /// [`begin_lowering_abort`](Self::begin_lowering_abort), if it was
    /// reported: what it is about ends at `end`.
    pub(crate) fn end_lowering_error(&mut self, error: Option<usize>, end: u32) {
        if let Some(error) = error.and_then(|at| self.lowering_list.get_mut(at)) {
            error.end = end;
        }
    }

    /// [`abort_lowering`](Self::abort_lowering) for something that starts
    /// at `start` and has not ended yet, where HHVM wanted to see `what`:
    /// it gives up before it gets to what is still to come. The result
    /// goes to [`end_lowering_error`](Self::end_lowering_error) once the
    /// end is known.
    #[cold]
    pub(crate) fn begin_lowering_abort(&mut self, start: u32, what: &'static str) -> Option<usize> {
        let reported = !self.unit_aborted && !self.lowering_errors;
        self.abort_lowering(start, start, what);
        reported.then(|| self.lowering_list.len() - 1)
    }

    /// Completes an error begun with
    /// [`begin_lowering_abort`](Self::begin_lowering_abort), if it was
    /// reported, with the text HHVM found instead: everything from where
    /// the error starts to `end`.
    pub(crate) fn end_lowering_abort(&mut self, error: Option<usize>, end: u32) {
        let src = self.src;
        if let Some(error) = error.and_then(|at| self.lowering_list.get_mut(at)) {
            error.end = end;
            let text = &src[error.start as usize..end as usize];
            error.message = format!(
                "Encountered unexpected text `{text}`, was expecting a {}.",
                error.message
            )
            .into();
        }
    }

    /// Records an error HHVM finds by inspecting its AST.
    #[cold]
    pub(crate) fn report_tree(
        &mut self,
        start: u32,
        end: u32,
        message: impl Into<std::borrow::Cow<'static, str>>,
    ) {
        self.report_tree_pass(0, start, end, message);
    }

    /// Like [`report_tree`](Self::report_tree) for one of the passes HHVM
    /// runs after the first; `pass` orders the errors the way HHVM does.
    #[cold]
    pub(crate) fn report_tree_pass(
        &mut self,
        pass: u8,
        start: u32,
        end: u32,
        message: impl Into<std::borrow::Cow<'static, str>>,
    ) {
        self.tree_errors.push((
            pass,
            crate::SyntaxError {
                start,
                end,
                message: message.into(),
            },
        ));
    }

    /// Puts the errors found in HHVM's order: those of its syntax pass by
    /// position (the parser's own error in front of the others about the
    /// same text), then what its later passes find in the AST, then the
    /// errors of the translation to the AST. The three lists stay apart
    /// for `parse_source` to join, because the errors of one more pass over
    /// the AST (the readonly analysis) belong after the first.
    pub(crate) fn order_errors(&mut self) {
        // HHVM's syntax pass hands its errors over last one first, and they
        // are then sorted by position: of two errors about the same text,
        // the one found later comes first.
        self.errors.reverse();
        self.errors.sort_by_key(|e| (e.start, e.end));
        if !self.tree_errors.is_empty() {
            // Errors found in the AST do not exist for the declarations
            // HHVM could not translate.
            let aborted = &self.aborted_units;
            self.tree_errors.retain(|(_, e)| {
                !aborted
                    .iter()
                    .any(|&(start, end)| start <= e.start && e.start < end)
            });
            self.tree_errors
                .sort_by_key(|(pass, e)| (*pass, e.start, e.end));
        }
    }

    /// What has been reported so far of HHVM's translation to its AST and
    /// of its checks on the AST, for
    /// [`restore_lowering_state`](Self::restore_lowering_state).
    #[inline]
    pub(crate) fn lowering_state(&self) -> LoweringState {
        LoweringState {
            tree_errors: self.tree_errors.len(),
            lowering_list: self.lowering_list.len(),
            lowering_errors: self.lowering_errors,
            unit_aborted: self.unit_aborted,
        }
    }

    /// Forgets what that translation and those checks have been made to
    /// report since `state` was taken: what was parsed since is something
    /// HHVM never translates.
    pub(crate) fn restore_lowering_state(&mut self, state: LoweringState) {
        self.tree_errors.truncate(state.tree_errors);
        self.lowering_list.truncate(state.lowering_list);
        if self.first_only_at >= state.lowering_list {
            self.first_only_at = usize::MAX;
        }
        self.lowering_errors = state.lowering_errors;
        self.unit_aborted = state.unit_aborted;
    }

    /// Starts a new declaration (or class member) as far as HHVM's
    /// translation to its AST is concerned.
    #[inline]
    pub(crate) fn begin_unit(&mut self) {
        self.unit_yields = self.yields.len();
        self.unit_start = self.tok.start;
        self.unit_aborted = self.file.block_aborted;
    }

    /// Ends the declaration started by [`begin_unit`](Self::begin_unit).
    #[inline]
    pub(crate) fn end_unit(&mut self) {
        if self.yields.len() > self.unit_yields {
            // Those outside any function body (in a default value, an
            // initializer, ...) are never where HHVM accepts one.
            self.report_misplaced_yields(self.unit_yields);
        }
        if self.unit_aborted {
            self.aborted_units.push((self.unit_start, self.prev_end));
            self.unit_aborted = false;
        }
    }

    /// Reports syntax that needs `feature` unless the file has enabled it.
    #[inline]
    pub(crate) fn require_feature(&mut self, start: u32, end: u32, feature: Feature) {
        if self.features & feature.bit() == 0 {
            self.report_missing_feature(start, end, feature);
        }
    }

    #[cold]
    fn report_missing_feature(&mut self, start: u32, end: u32, feature: Feature) {
        self.report(
            start,
            end,
            format!("Cannot use unstable feature: `{}`", feature.name()),
        );
    }

    #[cold]
    pub(crate) fn fail<X>(&mut self, msg: &'static str) -> R<X> {
        self.fail_span(self.tok.start, self.tok.start, msg)
    }

    #[cold]
    pub(crate) fn fail_at<X>(&mut self, at: u32, msg: &'static str) -> R<X> {
        self.fail_span(at, at, msg)
    }

    /// Fails with an error that covers `start..end`.
    #[cold]
    pub(crate) fn fail_span<X>(&mut self, start: u32, end: u32, msg: &'static str) -> R<X> {
        if self.error.is_none() {
            self.error = Some((start, msg));
            self.error_end = end;
        }
        Err(Fail)
    }

    /// Moves the parser to `pos`, dropping any lookahead. Error recovery only.
    pub(crate) fn jump_to(&mut self, pos: usize) {
        self.peeked = None;
        self.lexer.pos = pos;
        self.prev_start = pos as u32;
        self.prev_end = pos as u32;
        self.tok = self.lexer.next_token();
    }

    /// Forgets the failure being recovered from.
    pub(crate) fn clear_failure(&mut self) {
        self.error = None;
        self.lexer.error = None;
    }

    pub(crate) fn snapshot(&self) -> Snapshot {
        Snapshot {
            tok: self.tok,
            prev_start: self.prev_start,
            prev_end: self.prev_end,
            peeked: self.peeked,
            lexer_pos: self.lexer.pos,
            precedence: self.precedence,
            allow_as: self.allow_as,
            yield_seen: self.yield_seen,
            yield_ok: self.yield_ok,
            yields_len: self.yields.len(),
            block_depth: self.block_depth,
            block_start: self.block_start,
            block_ranges_len: self.block_ranges.len(),
            depth: self.depth,
            stmt_doc: self.stmt_doc,
            errors_len: self.errors.len(),
            class_members_len: self.class_members.len(),
            interest: self.interest,
            assigns: self.assigns,
            flagged_len: self.flagged.len(),
            lowering_errors: self.lowering_errors,
            syntax_error_reported: self.syntax_error_reported,
            syntax_errors_len: self.syntax_errors.len(),
            no_trailing: self.no_trailing,
            expected: self.expected,
            unit_aborted: self.unit_aborted,
            tree_errors_len: self.tree_errors.len(),
            lowering_list_len: self.lowering_list.len(),
            cst_len: self.cst.as_ref().map_or(0, |cst| cst.stack.len()),
            cst_full_end: self.cst.as_ref().map_or(0, |cst| cst.full_end),
            pending_patterns_len: self.pending_patterns.len(),
        }
    }

    pub(crate) fn restore(&mut self, s: Snapshot) {
        self.tok = s.tok;
        self.prev_start = s.prev_start;
        self.prev_end = s.prev_end;
        self.peeked = s.peeked;
        self.lexer.pos = s.lexer_pos;
        self.precedence = s.precedence;
        self.allow_as = s.allow_as;
        self.yield_seen = s.yield_seen;
        self.yield_ok = s.yield_ok;
        self.yields.truncate(s.yields_len);
        self.block_depth = s.block_depth;
        self.block_start = s.block_start;
        self.block_ranges.truncate(s.block_ranges_len);
        self.depth = s.depth;
        self.stmt_doc = s.stmt_doc;
        self.errors.truncate(s.errors_len);
        self.class_members.truncate(s.class_members_len);
        self.interest = s.interest;
        self.assigns = s.assigns;
        self.flagged.truncate(s.flagged_len);
        self.lowering_errors = s.lowering_errors;
        self.syntax_error_reported = s.syntax_error_reported;
        self.syntax_errors.truncate(s.syntax_errors_len);
        if !self.lexical_errors.is_empty() {
            let kept = s.syntax_errors_len as u32;
            self.lexical_errors.retain(|&i| i < kept);
        }
        self.no_trailing = s.no_trailing;
        self.expected = s.expected;
        self.unit_aborted = s.unit_aborted;
        self.tree_errors.truncate(s.tree_errors_len);
        self.lowering_list.truncate(s.lowering_list_len);
        if self.first_only_at >= s.lowering_list_len {
            self.first_only_at = usize::MAX;
        }
        self.pending_patterns.truncate(s.pending_patterns_len);
        self.error = None;
        self.lexer.error = None;
        if let Some(cst) = &mut self.cst {
            cst.stack.truncate(s.cst_len);
            cst.full_end = s.cst_full_end;
        }
    }

    /// Runs `f`; if it fails, rewinds as though nothing had been consumed.
    pub(crate) fn speculate<X>(&mut self, f: impl FnOnce(&mut Self) -> R<X>) -> Option<X> {
        let snap = self.snapshot();
        let saved_error = self.error;
        let saved_lexer_error = self.lexer.error;
        let syntax_errors = self.syntax_errors.len();
        let outer = std::mem::replace(&mut self.speculating, true);
        let tried = f(self);
        self.speculating = outer;
        match tried {
            // What takes recovering from a syntax error is not what was
            // being tried.
            Ok(x)
                if (self.lexer.error.is_none() || saved_lexer_error.is_some())
                    && self.syntax_errors.len() == syntax_errors =>
            {
                Some(x)
            }
            _ => {
                self.restore(snap);
                self.error = saved_error;
                self.lexer.error = saved_lexer_error;
                None
            }
        }
    }

    pub(crate) fn enter(&mut self) -> R<()> {
        self.depth += self.depth_step;
        if self.depth > MAX_DEPTH {
            return self.fail("nesting too deep");
        }
        Ok(())
    }

    /// Says no to one operand too many (see [`MAX_OPERANDS`]).
    #[inline]
    pub(crate) fn check_operands(&mut self, operands: u32) -> R<()> {
        if operands > MAX_OPERANDS {
            return self.fail("nesting too deep");
        }
        Ok(())
    }

    #[inline]
    pub(crate) fn leave(&mut self) {
        self.depth -= self.depth_step;
    }

    // ------------------------------------------------------------------
    // Small helpers

    #[inline]
    pub(crate) fn alloc<X>(&self, x: X) -> &'a X {
        self.arena.alloc(x)
    }

    #[inline]
    pub(crate) fn vec<X>(&self) -> List<'a, X> {
        List {
            items: SmallVec::new(),
            arena: self.arena,
        }
    }

    #[inline]
    pub(crate) fn tok_text(&self, tok: Token) -> &'a str {
        &self.src[tok.start as usize..tok.end as usize]
    }

    #[inline]
    pub(crate) fn tok_span(tok: Token) -> Span {
        Span::new(tok.start, tok.end)
    }

    #[inline]
    pub(crate) fn id(&self, tok: Token) -> Id<'a> {
        Id {
            span: Span::new(tok.start, tok.end),
            name: self.tok_text(tok),
        }
    }

    /// Span from `start` to the end of the last consumed token.
    #[inline]
    pub(crate) fn span_from(&self, start: u32) -> Span {
        // Nothing may have been consumed since `start` (see `repair.rs`).
        Span::new(start, self.prev_end.max(start))
    }

    /// For where HHVM's lexer looks for keywords: a name spelled like a
    /// reserved keyword in another case is that keyword there, and an
    /// error (see [`lowercase_keyword`](Self::lowercase_keyword)).
    #[inline]
    pub(crate) fn check_case(&mut self) -> R<()> {
        if self.tok.mixed_case_kw {
            self.lowercase_keyword();
        }
        Ok(())
    }

    /// Turns the current token, a name spelled like a keyword in upper or
    /// mixed case, into that keyword, with the error HHVM's lexer reports
    /// when it reads such a name as a keyword.
    #[cold]
    pub(crate) fn lowercase_keyword(&mut self) {
        let (start, end) = (self.tok.start, self.tok.end);
        let text = &self.src[start as usize..end as usize];
        if let crate::token::MixedCase::Keyword(kind) =
            crate::token::mixed_case_keyword(text.as_bytes())
        {
            self.report_lexical(
                start,
                end,
                format!("Keyword `{text}` must be written in lowercase"),
            );
            self.tok.kind = kind;
        }
        self.tok.mixed_case_kw = false;
        self.tok.mixed_case_soft = false;
    }

    /// [`check_case`](Self::check_case) at the start of a statement. HHVM
    /// reads a `static` there that no `::` follows the way it reads names,
    /// in whatever case.
    #[inline]
    pub(crate) fn check_case_of_statement(&mut self) {
        if self.tok.mixed_case_kw && !self.static_as_name() {
            self.lowercase_keyword();
        }
    }

    /// Whether the current token, a name spelled like a reserved keyword in
    /// another case, is `static` without a `::` after it.
    #[cold]
    pub(crate) fn static_as_name(&mut self) -> bool {
        self.tok_text(self.tok).eq_ignore_ascii_case("static") && self.peek_kind() != T::ColonColon
    }

    /// Whether `tok` is a name to HHVM's lexer where it looks for every
    /// keyword: one that is not spelled like a keyword in any case.
    #[inline]
    pub(crate) fn is_plain_name(tok: Token) -> bool {
        tok.kind == T::Name && !tok.mixed_case_kw && !tok.mixed_case_soft
    }

    /// Consumes a name where HHVM's parser requires one that is no keyword
    /// of any kind (see [`require`](Self::require) for what happens when it
    /// is not there, in which case the name comes back empty).
    pub(crate) fn require_plain_name(&mut self) -> Id<'a> {
        if Self::is_plain_name(self.tok) {
            let tok = self.take();
            return self.id(tok);
        }
        if self.require_missing(T::Name, "A name is expected here.") {
            Id {
                span: Span::new(self.prev_start, self.prev_end),
                name: &self.src[self.prev_start as usize..self.prev_end as usize],
            }
        } else {
            self.missing_id()
        }
    }

    /// Consumes a name where HHVM's parser takes the keywords it does not
    /// reserve for names too. One that is missing is reported, comes back
    /// empty, and what stands there stays.
    pub(crate) fn require_name_non_reserved(&mut self) -> Id<'a> {
        let tok = self.tok;
        if (tok.kind == T::Name && !tok.mixed_case_kw)
            || (tok.kind.is_keyword() && !tok.kind.is_reserved())
        {
            self.bump();
            self.c_name();
            self.id(tok)
        } else {
            self.missing_name()
        }
    }

    /// Consumes any name-like token (identifier or keyword) as a name. A
    /// missing name is reported and comes back empty.
    pub(crate) fn expect_name_allow_keywords(&mut self) -> R<Id<'a>> {
        if self.tok.kind.is_name_like() {
            let tok = self.take();
            self.c_name();
            Ok(self.id(tok))
        } else {
            Ok(self.missing_name())
        }
    }

    /// Consumes an identifier, accepting keywords HHVM does not reserve. A
    /// missing name is reported and comes back empty.
    pub(crate) fn expect_name(&mut self) -> R<Id<'a>> {
        self.check_case()?;
        if self.tok.kind == T::Name || (self.tok.kind.is_keyword() && !self.tok.kind.is_reserved())
        {
            let tok = self.take();
            self.c_name();
            Ok(self.id(tok))
        } else {
            Ok(self.missing_name())
        }
    }

    /// Reports that a name is missing in front of the current token, which
    /// stays where it is.
    #[cold]
    pub(crate) fn missing_name(&mut self) -> Id<'a> {
        self.report_missing("A name is expected here.");
        self.c_missing();
        self.missing_id()
    }

    /// The name of something that has none.
    pub(crate) fn missing_id(&self) -> Id<'a> {
        let at = self.missing_pos();
        Id {
            span: Span::new(at, at),
            name: "",
        }
    }

    /// Consumes a token of `kind` that is a name of some sort (see
    /// [`require`](Self::require) for what happens when it is not there, in
    /// which case the name comes back empty).
    pub(crate) fn require_id(&mut self, kind: T) -> Id<'a> {
        if self.tok.kind == kind {
            let tok = self.take();
            return self.id(tok);
        }
        if self.require_missing(kind, expected_message(kind)) {
            Id {
                span: Span::new(self.prev_start, self.prev_end),
                name: &self.src[self.prev_start as usize..self.prev_end as usize],
            }
        } else {
            self.missing_id()
        }
    }

    pub(crate) fn expect_variable(&mut self) -> R<Id<'a>> {
        Ok(self.require_id(T::Variable))
    }

    /// Parses `Foo`, `Foo\Bar`, `\Foo\Bar`, `namespace\Foo`. The current
    /// token must be a name-like token or a backslash. Returns the name and
    /// whether it ended in a backslash (group `use` prefixes).
    pub(crate) fn parse_qualified_name(&mut self) -> R<(Id<'a>, bool)> {
        let start = self.tok.start;
        let mut expect_name = true;
        let mut trailing_backslash = false;
        // Token extents, in case the name has to be reassembled.
        let mut pieces = [(0u32, 0u32); 24];
        let mut count = 0usize;
        // In the lossless tree a name with backslashes is a list of names,
        // each with the backslash after it; a plain name is just its token.
        let parts = self.c_mark();
        loop {
            let is_piece = if expect_name {
                self.tok.kind.is_name_like() || (self.at(T::Backslash) && count == 0)
            } else {
                self.at(T::Backslash)
            };
            if !is_piece {
                break;
            }
            if count == pieces.len() {
                return self.fail("qualified name too long");
            }
            pieces[count] = (self.tok.start, self.tok.end);
            count += 1;
            trailing_backslash = self.at(T::Backslash);
            expect_name = trailing_backslash;
            if trailing_backslash && count == 1 {
                self.c_missing();
            }
            self.bump();
            if trailing_backslash {
                self.c_list_item(true);
            } else if count > 1 {
                self.c_name();
            }
        }
        if count == 0 {
            return self.fail("A name is expected here.");
        }
        if trailing_backslash {
            // Only the prefix of a group `use` may end like this.
            let (start, end) = pieces[count - 1];
            self.report(start, end, "A name is expected here.");
        }
        if (count > 1 || trailing_backslash) && self.c_on() {
            if !trailing_backslash {
                self.c_list_item(false);
            }
            self.c_list(parts);
            self.c_node(K::QualifiedName);
        }
        let span = self.span_from(start);
        let mut name = span.text(self.src);
        let contiguous = pieces[..count].windows(2).all(|w| w[0].1 == w[1].0);
        if !contiguous {
            // HHVM builds the name from its tokens, dropping any comments or
            // whitespace written between them.
            let mut joined = bumpalo::collections::String::new_in(self.arena);
            for &(s, e) in &pieces[..count] {
                joined.push_str(&self.src[s as usize..e as usize]);
            }
            name = joined.into_bump_str();
        }
        Ok((Id { span, name }, trailing_backslash))
    }

    /// If the current token is a `:` that starts an XHP class name
    /// (`:foo:bar`), re-lexes it as one.
    pub(crate) fn retokenize_xhp_class_name(&mut self) -> bool {
        let start = self.tok.start as usize;
        if (self.tok.kind == T::Colon || self.tok.kind == T::ColonColon)
            && self.lexer.is_xhp_class_name_at(start)
        {
            let end = self.lexer.xhp_class_name_end(start);
            self.retokenize(T::XhpClassName, end);
            true
        } else {
            false
        }
    }

    // ------------------------------------------------------------------
    // Doc comments

    /// Where HHVM starts the leading trivia of a token, given the end of
    /// the token before it: past what trails that token, which is the rest
    /// of its line, block comments included.
    pub(crate) fn leading_trivia_start(&self, prev_end: u32) -> usize {
        let prev_end = prev_end as usize;
        if prev_end == 0 {
            return 0;
        }
        prev_end + crate::cst::trivia::hhvm_trailing_width(self.bytes, prev_end)
    }

    /// Finds the doc comment (`/** ... */`) in the trivia in front of a
    /// token. This is a port of the state machine in HHVM's
    /// `extract_docblock`, quirks included.
    pub(crate) fn doc_comment(&self, trivia_start: u32, tok_start: u32) -> Option<Span> {
        if trivia_start == tok_start {
            return None;
        }
        let lead = self.leading_trivia_start(trivia_start);
        let end = tok_start as usize;
        if lead >= end {
            return None;
        }
        let s = &self.bytes[lead..end];
        // Cheap rejection: no doc comment without "/**".
        if !s.windows(3).any(|w| w == b"/**") {
            return None;
        }

        #[derive(Clone, Copy, PartialEq)]
        enum St {
            DocComment,
            EmbeddedCmt,
            EndDoc,
            EndEmbedded,
            Free,
            LineCmt,
            MaybeDoc,
            MaybeDoc2,
            SawSlash,
        }
        use St::*;
        let ws = |c: u8| c == b' ' || c == b'\t' || c == b'\n' || c == b'\r';

        let mut best: Option<(usize, usize)> = None;
        let mut start = 0usize;
        let mut st = Free;
        let mut idx = 0usize;
        loop {
            if idx == s.len() {
                break;
            }
            let next = idx + 1;
            let c = s[idx];
            match (st, c) {
                (LineCmt, b'\n') => {
                    start = next;
                    st = Free;
                    idx = next;
                }
                (EndEmbedded, b'/') => {
                    start = next;
                    st = Free;
                    idx = next;
                }
                (EndDoc, b'/') => {
                    // A later doc comment wins over this one.
                    best = Some((start, idx + 1));
                    start = next;
                    st = Free;
                    idx = next;
                }
                (Free, b'#') => {
                    start = next;
                    st = LineCmt;
                    idx = next;
                }
                (Free, b'/') => {
                    start = idx;
                    st = SawSlash;
                    idx = next;
                }
                (SawSlash, b'/') => {
                    start = next;
                    st = LineCmt;
                    idx = next;
                }
                (SawSlash, b'*') => {
                    st = MaybeDoc;
                    idx = next;
                }
                (MaybeDoc, b'*') => {
                    st = MaybeDoc2;
                    idx = next;
                }
                (MaybeDoc, _) => {
                    st = EmbeddedCmt;
                    idx = next;
                }
                (MaybeDoc2, b'/') => {
                    start = next;
                    st = Free;
                    idx = next;
                }
                (MaybeDoc2, c) if ws(c) => {
                    st = DocComment;
                }
                (MaybeDoc2, _) => {
                    st = EmbeddedCmt;
                    idx = next;
                }
                (DocComment, b'*') => {
                    st = EndDoc;
                    idx = next;
                }
                (DocComment, _) => {
                    idx = next;
                }
                (EndDoc, _) => {
                    st = DocComment;
                    idx = next;
                }
                (EmbeddedCmt, b'*') => {
                    st = EndEmbedded;
                    idx = next;
                }
                (EndEmbedded, b'*') => {
                    idx = next;
                }
                (EndEmbedded, _) => {
                    st = EmbeddedCmt;
                    idx = next;
                }
                (_, c) if ws(c) => {
                    idx = next;
                }
                (LineCmt, _) | (EmbeddedCmt, _) => {
                    idx = next;
                }
                _ => break,
            }
        }
        best.map(|(s, e)| Span::new((lead + s) as u32, (lead + e) as u32))
    }

    /// Doc comment in front of `tok`.
    #[inline]
    pub(crate) fn doc_before(&self, tok: Token) -> Option<Span> {
        self.doc_comment(tok.trivia_start, tok.start)
    }

    // ------------------------------------------------------------------
    // Compound statements

    pub(crate) fn enter_block(&mut self, lbrace_start: u32) {
        if self.block_depth == 0 {
            self.block_start = lbrace_start;
        }
        self.block_depth += 1;
    }

    pub(crate) fn leave_block(&mut self, rbrace_start: u32) {
        self.block_depth -= 1;
        if self.block_depth == 0 {
            self.block_ranges.push((self.block_start, rbrace_start));
        }
    }

    // ------------------------------------------------------------------
    // Entry point

    pub(crate) fn parse_program(&mut self) -> R<Program<'a>> {
        let declarations = self.c_mark();
        let header = self.parse_header()?;
        let mut defs = self.vec();
        self.parse_defs_until(T::Eof, &mut defs)?;
        if !self.at(T::Eof) {
            return self.fail("unexpected token");
        }
        if self.c_on() {
            // The end of the file is a declaration too: it carries the
            // trivia after the last token.
            self.c_push_current();
            self.c_node(K::EndOfFile);
            self.c_list(declarations);
            self.c_node(K::Script);
        }
        if let Some((at, msg)) = self.lexer.error {
            return self.fail_at(at, msg);
        }
        if let Some(start) = self.lexer.unterminated_comment {
            let end = self.bytes.len() as u32;
            self.report_syntax(start, end, "This delimited comment is not terminated.");
        }
        self.settle_syntax_errors();
        self.check_file_end();
        Ok(Program {
            defs: defs.into_bump_slice(),
            header,
        })
    }

    /// Handles an optional `#!` line and `<?hh` header, then primes the
    /// first token. Like HHVM, this looks for both past any blank space.
    fn parse_header(&mut self) -> R<Option<Header<'a>>> {
        let b = self.bytes;
        let skip_blank = |mut pos: usize| {
            while pos < b.len() && matches!(b[pos], b' ' | b'\t' | b'\n' | b'\r') {
                pos += 1;
            }
            pos
        };
        let mut pos = skip_blank(0);
        // The `#!` line, from the start of the file to its line break.
        let mut hashbang = "";
        if b[pos..].starts_with(b"#!") {
            while pos < b.len() && b[pos] != b'\n' && b[pos] != b'\r' {
                pos += 1;
            }
            hashbang = &self.src[..pos];
            pos += crate::cst::trivia::trailing_width(b, pos);
        }
        let after_hashbang = pos;
        let lt = skip_blank(pos);
        if !b[lt..].starts_with(b"<?") {
            if self.kind == FileKind::Php && !b.is_empty() {
                // HHVM reads such a file from the top, `#!` line included.
                self.report_syntax(0, 0, "A .php file must begin with `<?hh`.");
                self.tok = self.lexer.next_token();
                return Ok(None);
            }
            let mut header = None;
            if !hashbang.is_empty() {
                self.lexer.line_start = after_hashbang;
                self.c_token(TokenKind::Hashbang, 0, hashbang.len() as u32);
                self.c_missing();
                self.c_node(K::MarkupSection);
                header = Some(Header {
                    span: Span::new(0, hashbang.len() as u32),
                    hashbang,
                });
                pos = after_hashbang;
            } else {
                pos = 0;
            }
            self.lexer.pos = pos;
            self.prev_end = pos as u32;
            self.tok = self.lexer.next_token();
            return Ok(header);
        }

        let name_start = lt + 2;
        let mut name_end = name_start;
        while name_end < b.len() && crate::lexer::is_name_start(b[name_end]) {
            name_end += 1;
        }
        let name = &self.src[name_start..name_end];
        // HHVM does not look any further at such a file, but its parser
        // reads it like any other: there is a lossless tree for it.
        if (name.is_empty() || name.eq_ignore_ascii_case("php")) && !self.c_on() {
            return self.fail_at(0, crate::NOT_A_HACK_FILE);
        }
        let section_start = if hashbang.is_empty() { lt } else { 0 };
        if let FileKind::Hack(extension) = self.kind {
            let kind = if extension == "hack" {
                "strict"
            } else {
                "partial"
            };
            self.report_lowering(
                section_start as u32,
                name_end as u32,
                format!(
                    "Leading markup and `<?hh` are not permitted in `.{extension}` files, which are always `{kind}`."
                ),
            );
        } else if self.kind == FileKind::Php {
            // Only a line break may separate a `#!` line from the `<?hh`.
            let expected = if hashbang.is_empty() {
                0
            } else {
                hashbang.len() + 1
            };
            if lt != expected {
                self.report_lowering(
                    lt as u32,
                    name_end as u32,
                    "A .php file must begin with `<?hh`.",
                );
            }
        }
        let is_hh = name.eq_ignore_ascii_case("hh");
        if !is_hh {
            self.report_lowering(
                name_start as u32,
                name_end as u32,
                format!(
                    "`<?{name}` is not a valid script marker. Use `<?hh` to start a Hack file."
                ),
            );
        }
        if !hashbang.is_empty() {
            self.lexer.line_start = after_hashbang;
            self.c_token(TokenKind::Hashbang, 0, hashbang.len() as u32);
        }
        if let Some(cst) = &mut self.cst {
            // The header is the first declaration: the `#!` line, if any,
            // and `<?` with the name after it. Blank space in front of the
            // `<?` belongs to no token.
            if hashbang.is_empty() {
                cst.missing();
            }
            cst.full_end = lt as u32;
            let (lt, name) = (lt as u32, name_start as u32);
            self.c_token_with(TokenKind::LessThanQuestion, lt, name, Trailing::None);
            if name == name_end as u32 {
                self.c_missing();
            } else {
                self.c_token(TokenKind::Name, name, name_end as u32);
            }
            self.c_node(K::MarkupSuffix);
            self.c_node(K::MarkupSection);
        }
        self.decl_mode = self.hhi && is_hh;
        self.lexer.pos = name_end;
        self.prev_end = name_end as u32;
        self.tok = self.lexer.next_token();
        Ok(Some(Header {
            span: Span::new(section_start as u32, name_end as u32),
            hashbang,
        }))
    }
}

/// How a parse went: the tree if the parser got to the end of the file, or
/// else where and why it gave up. The parser comes back either way, for
/// what it found until then.
pub(crate) type Parsed<'a> = (Result<Program<'a>, (u32, &'static str)>, Parser<'a>);

pub(crate) fn parse_program<'a>(
    arena: &'a Bump,
    src: &'a str,
    kind: FileKind,
    path: &'a str,
    recovering: bool,
) -> Parsed<'a> {
    let mut parser = Parser::new(arena, src);
    parser.hhi = kind == FileKind::Hhi;
    parser.kind = kind;
    parser.path = path;
    parser.recovering = recovering;
    match parser.parse_program() {
        Ok(program) => (Ok(program), parser),
        Err(Fail) => {
            let error = parser
                .error
                .or(parser.lexer.error)
                .unwrap_or((0, "parse error"));
            (Err(error), parser)
        }
    }
}

/// What [`Parser::syntax_error`] says about a token that should not be
/// there; the error reported for it also quotes the token.
const UNEXPECTED_TOKEN: &str = "Encountered unexpected token";

/// How the tokens of more than one character that the parser may require
/// are written.
fn spelling(kind: T) -> Option<&'static str> {
    Some(match kind {
        T::FatArrow => "=>",
        T::LambdaArrow => "==>",
        T::ColonColon => "::",
        T::LtLt => "<<",
        T::DotDotDot => "...",
        T::As => "as",
        T::Class => "class",
        T::Function => "function",
        T::While => "while",
        _ => return None,
    })
}

/// HHVM's idea of a misspelling: the same length, and at most one byte
/// that differs.
pub(crate) fn one_byte_apart(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).filter(|(x, y)| x != y).count() <= 1
}

/// What to say when `kind` was required and something else was found, in
/// the words HHVM uses for the same mistake.
fn expected_message(kind: T) -> &'static str {
    match kind {
        T::Semicolon => "A semicolon `;` is expected here.",
        T::LParen => "A left parenthesis `(` is expected here.",
        T::RParen => "A right parenthesis `)` is expected here.",
        T::LBrace => "A left brace `{` is expected here.",
        T::RBrace => "A right brace `}` is expected here.",
        T::LBracket => "An opening square bracket `[` is expected here.",
        T::RBracket => "A closing square bracket `]` is expected here.",
        T::Lt => "An opening angle bracket `<` is expected here.",
        T::Gt => "A closing angle bracket `>` is expected here.",
        T::Colon => "A colon `:` is expected here.",
        T::ColonColon => "A scope resolution operator `::` is expected here.",
        T::Comma => "A comma `,` is expected here.",
        T::Eq => "An equals sign `=` is expected here.",
        T::FatArrow => "An arrow `=>` is expected here.",
        T::Name => "A name is expected here.",
        T::Backtick => "A backtick ``` is expected here.",
        T::LambdaArrow => "A lambda arrow `==>` is expected here.",
        T::Type => "The `type` keyword is expected here.",
        T::Variable => "A variable name is expected here.",
        T::As => "An `as` keyword is expected here.",
        T::Class => "The `class` keyword is expected here.",
        T::Function => "The `function` keyword is expected here.",
        T::While => "The `while` keyword is expected here.",
        T::DotDotDot => "`...` is expected here.",
        _ => "unexpected token",
    }
}

/// Parses `src`, recording the lossless syntax tree. Alongside the tree
/// comes the syntax error the parser recovered from, if it had to: the tree
/// then has the shape HHVM's recovery gives it (missing tokens are `Missing`).
pub(crate) fn parse_program_cst(
    src: &str,
    kind: FileKind,
    options: crate::ParserOptions,
) -> Result<(crate::cst::Node, Vec<crate::SyntaxError>), (u32, &'static str)> {
    let arena = Bump::new();
    let mut parser = Parser::new(&arena, src);
    parser.hhi = kind == FileKind::Hhi;
    parser.kind = kind;
    parser.options = options;
    parser.cst = Some(Box::default());
    match parser.parse_program() {
        Ok(_) => {
            let recovered = std::mem::take(&mut parser.syntax_errors);
            let mut cst = parser.cst.take().unwrap();
            match cst.stack.pop() {
                Some(root) if cst.stack.is_empty() && !cst.inconsistent => Ok((root, recovered)),
                // Recovery left the recording in a state that is not a tree.
                _ if parser.syntax_error_reported => Err(parser.syntax_error),
                _ => Err((0, "the lossless tree is incomplete")),
            }
        }
        Err(Fail) => Err(parser
            .error
            .or(parser.lexer.error)
            .unwrap_or((0, "parse error"))),
    }
}
