//! A Hack parser for Hakana.
//!
//! This crate parses Hack source straight into a compact, arena-allocated
//! syntax tree ([`ast`]) designed for analysis in Rust: no concrete syntax
//! tree, no per-node heap allocation, byte-offset positions, and identifiers
//! borrowed from the source text.
//!
//! The parser reports what HHVM's parser and its later passes report, for
//! invalid code too: at a syntax error it carries on the way HHVM does (see
//! `parser/repair.rs`), so that the errors that follow from HHVM's recovery
//! come out the same. A file with a syntax error still yields the
//! declarations that could be salvaged from it
//! ([`ParseOutput::recovered`]).
//!
//! ```ignore
//! let arena = bumpalo::Bump::new();
//! let parsed = hakana_parser::parse(&arena, source)?;
//! for def in parsed.program.defs { /* ... */ }
//! ```

pub mod aast;
mod aast_copy;
pub mod aast_visitor;
pub mod ast;
mod checks;
pub mod cst;
mod expr_tree;
pub mod lexer;
pub mod lower;
mod parser;
pub mod readonly_check;
pub mod source_hash;
pub mod span;
pub mod token;

pub use bumpalo::Bump;
pub use lexer::CommentKind;
pub use source_hash::source_hash;
pub use span::{LineIndex, Span};

use parser::FileKind;

/// A comment found while parsing.
#[derive(Clone, Copy, Debug)]
pub struct Comment {
    pub kind: CommentKind,
    /// Offset of the opening `/`.
    pub start: u32,
    /// Offset just past the comment.
    pub end: u32,
    /// Start of the first token after the comment.
    pub next_token: u32,
    /// Whether that token is inside a `{ ... }` statement block. HHVM only
    /// honours `HH_FIXME` comments there.
    pub in_block: bool,
    /// A `//` comment on the same line as the code before it.
    pub trailing: bool,
    /// A `//` comment that starts the line after the preceding token (or
    /// the file).
    pub first_leading: bool,
}

#[derive(Debug)]
pub struct ParseOutput<'a> {
    pub program: ast::Program<'a>,
    /// Every comment in the file, in source order.
    pub comments: Vec<Comment>,
    /// The file uses `readonly` in one of the places that make HHVM run its
    /// readonly analysis.
    pub uses_readonly: bool,
    /// Errors that did not prevent parsing, in the order HHVM reports
    /// them: what its syntax checks find (by position), then what its
    /// checks over the AST find, then the errors of its translation to the
    /// AST. The first of those checks over the AST, the readonly analysis,
    /// needs the lowered tree: its errors are only part of what
    /// [`lower`](Self::lower) returns.
    pub errors: Vec<SyntaxError>,
    /// The file has a syntax error: this one, which is also among `errors`
    /// (HHVM reports the first syntax error of a file, and whatever its
    /// checks make of the rest). `program` holds the declarations that
    /// could be salvaged, without function bodies.
    pub recovered: Option<ParseError>,
    /// How many of `errors`, from the start, come from the syntax checks.
    /// The errors of the readonly analysis go right after them.
    syntax_check_errors: usize,
    /// The extents of the declarations (or class members) HHVM fails to
    /// translate to its AST. They are missing from the tree its readonly
    /// analysis runs over.
    untranslated: Vec<(u32, u32)>,
    /// How many of `errors`, at the end, come from that translation.
    lowering_errors: usize,
    /// Which of those (counting from the first) HHVM only reports because
    /// its translation has reported nothing before, if any.
    first_only: Option<usize>,
}

impl<'a> ParseOutput<'a> {
    /// Lowers the file to the analysis tree ([`aast`]) and completes its
    /// errors: like HHVM, this runs the readonly analysis
    /// ([`readonly_check`]) over the tree of a file that uses `readonly`.
    /// `src` and `is_hhi` are those the file was parsed with. The analysis
    /// tree is built in `arena` and keeps nothing of the syntax tree: the
    /// arena that was parsed into can be dropped afterwards.
    pub fn lower<'o>(
        self,
        arena: &'o bumpalo::Bump,
        src: &'o str,
        is_hhi: bool,
    ) -> Result<ParsedFile<'o>, lower::LowerError> {
        self.lower_or_errors(arena, src, is_hhi)
            .map_err(|(error, _)| error)
    }

    /// Like [`lower`](Self::lower). When the lowering fails, the errors of
    /// the file come back with the failure: those found so far, and the
    /// failure itself if it is an error HHVM reports. HHVM only does when
    /// its translation to the AST has reported nothing before, and nothing
    /// else for a file it has no errors for is worth reporting: the list
    /// is empty then, and the failure is all there is to say.
    pub fn lower_or_errors<'o>(
        self,
        arena: &'o bumpalo::Bump,
        src: &'o str,
        is_hhi: bool,
    ) -> Result<ParsedFile<'o>, (lower::LowerError, Vec<SyntaxError>)> {
        let lowered = match lower::lower(arena, &self.program, &self.comments, src, is_hhi) {
            Ok(lowered) => lowered,
            Err(error) => {
                let mut errors = self.errors;
                if let Some(end) = error.hhvm_end
                    && self.lowering_errors == 0
                {
                    errors.push(SyntaxError {
                        start: error.offset,
                        end,
                        message: error.message.into(),
                    });
                }
                return Err((error, errors));
            }
        };
        let mut errors = self.errors;
        // What is salvaged from a file with a syntax error has no function
        // bodies left to analyse.
        if self.uses_readonly && self.recovered.is_none() {
            let mut found = readonly_check::check_program(&lowered.program, true);
            // HHVM's analysis never sees the declarations it could not
            // translate.
            found.retain(|e| {
                !self
                    .untranslated
                    .iter()
                    .any(|&(start, end)| start <= e.start && e.start < end)
            });
            let at = self.syntax_check_errors;
            errors.splice(at..at, found);
        }
        // What lowering turns up in a file with a syntax error has been
        // found already, in the whole of what was parsed of it.
        if self.recovered.is_none() {
            let translation = errors.len() - self.lowering_errors;
            merge_lowering_errors(
                &mut errors,
                translation,
                lowered.errors,
                &self.untranslated,
                self.first_only.map(|at| translation + at),
            );
        }
        Ok(ParsedFile {
            program: lowered.program,
            comments: lowered.comments,
            lines: lowered.lines,
            errors,
            has_syntax_error: self.recovered.is_some(),
        })
    }
}

/// Puts the errors only found while lowering (`found`, see
/// [`lower::Lowered::errors`]) where they belong: with the other errors of
/// HHVM's translation to its AST, which are those of `errors` from
/// `translation` on, in the order of the code they are about. There are
/// none for the declarations HHVM gave up on (`untranslated`).
fn merge_lowering_errors(
    errors: &mut Vec<SyntaxError>,
    mut translation: usize,
    found: Vec<(u32, SyntaxError)>,
    untranslated: &[(u32, u32)],
    mut first_only: Option<usize>,
) {
    let mut preceded = false;
    for (after, error) in found {
        if untranslated
            .iter()
            .any(|&(start, end)| start <= error.start && error.start < end)
        {
            continue;
        }
        while translation < errors.len() && errors[translation].start < after {
            translation += 1;
        }
        errors.insert(translation, error);
        if let Some(at) = first_only.as_mut().filter(|at| translation <= **at) {
            *at += 1;
            preceded = true;
        }
        translation += 1;
    }
    // What HHVM only says when it has said nothing before goes unsaid.
    if let Some(at) = first_only.filter(|&at| preceded && at < errors.len()) {
        errors.remove(at);
    }
}

/// An error found in a file that could still be parsed: the kind of thing
/// HHVM reports from its syntax checks ("`async` functions must have an
/// `Awaitable` return type", ...). The tree is still produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    pub start: u32,
    pub end: u32,
    pub message: std::borrow::Cow<'static, str>,
}

/// The [`ParseError::message`] for a file that is not Hack at all (`<?php`).
pub const NOT_A_HACK_FILE: &str = "not a Hack file";

/// The file could not be parsed at all.
#[derive(Clone, Copy, Debug)]
pub struct ParseError {
    pub offset: u32,
    pub message: &'static str,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at offset {}", self.message, self.offset)
    }
}

impl std::error::Error for ParseError {}

/// A file parsed and lowered to the analysis tree.
pub struct ParsedFile<'a> {
    pub program: aast::Program<'a>,
    pub comments: aast::ScouredComments,
    /// The file's line table: positions in the tree are byte ranges, and
    /// this turns them into lines and columns.
    pub lines: LineIndex,
    /// Errors that did not prevent parsing.
    pub errors: Vec<SyntaxError>,
    /// The file has a syntax error (it is among `errors`): `program`
    /// declares what could be salvaged, with function bodies left out, and
    /// is not worth analysing.
    pub has_syntax_error: bool,
}

/// Parses a Hack file and lowers it to the analysis tree ([`aast`]) in one
/// go. `path` is the file's path: its extension decides some of what HHVM
/// accepts (an `.hhi` file is a declaration file and drops function bodies,
/// a `.php` file needs its `<?hh` header, a `.hack` file must not have
/// one), and a few messages name the file.
pub fn parse_file<'a>(
    arena: &'a bumpalo::Bump,
    src: &'a str,
    path: &str,
) -> Result<ParsedFile<'a>, ParseError> {
    thread_local! {
        /// Where syntax trees are built. Only the analysis tree goes into
        /// the caller's arena; the syntax tree is gone once it is lowered.
        static SYNTAX_ARENA: std::cell::RefCell<bumpalo::Bump> =
            std::cell::RefCell::new(bumpalo::Bump::new());
    }
    SYNTAX_ARENA.with(|syntax_arena| match syntax_arena.try_borrow_mut() {
        Ok(mut syntax_arena) => {
            let file = parse_and_lower(&syntax_arena, arena, src, path);
            syntax_arena.reset();
            file
        }
        // Parsing from within a parse on the same thread.
        Err(_) => parse_and_lower(&bumpalo::Bump::new(), arena, src, path),
    })
}

fn parse_and_lower<'a>(
    syntax_arena: &bumpalo::Bump,
    arena: &'a bumpalo::Bump,
    src: &'a str,
    path: &str,
) -> Result<ParsedFile<'a>, ParseError> {
    let kind = FileKind::of(path);
    let is_hhi = kind == FileKind::Hhi;
    let parsed = parse_source(syntax_arena, src, kind, path, None)?;
    let recovered = parsed.recovered.is_some();
    match parsed.lower_or_errors(arena, src, is_hhi) {
        Ok(file) => Ok(file),
        Err((e, errors)) => {
            // The file parses but uses something the analysis tree cannot
            // express. Treat that like a syntax error: salvage what the
            // file declares.
            let error = ParseError {
                offset: e.offset,
                message: e.message,
            };
            if recovered {
                return Err(error);
            }
            let mut file = parse_source(syntax_arena, src, kind, path, Some(error))?
                .lower(arena, src, is_hhi)
                .map_err(|_| error)?;
            if !errors.is_empty() {
                // What HHVM reports for the file says it better.
                file.errors = errors;
            }
            Ok(file)
        }
    }
}

/// The lossless syntax tree of a file.
pub struct LosslessTree {
    pub root: cst::Node,
    /// The syntax errors the parser recovered from, all of them and in
    /// HHVM's order (by position), in which case `root` has the shape
    /// HHVM's recovery gives it (a missing token is
    /// [`cst::Node::Missing`]). Empty for a file that parses. Errors of the
    /// later checks ("`async` functions must return `Awaitable`", ...) are
    /// not syntax errors and are not included.
    pub errors: Vec<SyntaxError>,
}

/// The settings of HHVM's parser that decide what it accepts. The default
/// is what Hakana's analysis parses with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParserOptions {
    /// XHP classes are declared `xhp class foo:bar`, and the names in a
    /// `children` declaration are written the same way, without a colon in
    /// front. When off, `xhp class` is an error and those names are
    /// `:foo:bar` or plain names.
    pub enable_xhp_class_modifier: bool,
    /// `children` declarations are an error.
    pub disable_xhp_children_declarations: bool,
}

impl Default for ParserOptions {
    fn default() -> Self {
        ParserOptions {
            enable_xhp_class_modifier: true,
            disable_xhp_children_declarations: false,
        }
    }
}

/// Parses a Hack file into its lossless syntax tree ([`cst`]): what the
/// linters and the formatter read. `Err` means the parser could not recover
/// from a syntax error at all, so there is no tree.
pub fn parse_cst(
    src: &str,
    is_hhi: bool,
    options: ParserOptions,
) -> Result<LosslessTree, ParseError> {
    let kind = if is_hhi {
        FileKind::Hhi
    } else {
        FileKind::Other
    };
    lossless_tree(src, kind, options)
}

/// Like [`parse_cst`] for the file at `path`, whose extension decides some
/// of what HHVM accepts (see [`parse_named`]).
pub fn parse_cst_named(
    src: &str,
    path: &str,
    options: ParserOptions,
) -> Result<LosslessTree, ParseError> {
    lossless_tree(src, FileKind::of(path), options)
}

fn lossless_tree(
    src: &str,
    kind: FileKind,
    options: ParserOptions,
) -> Result<LosslessTree, ParseError> {
    if src.len() >= u32::MAX as usize {
        return Err(ParseError {
            offset: 0,
            message: "file too large",
        });
    }
    let (root, recovered) = parser::parse_program_cst(src, kind, options)
        .map_err(|(offset, message)| ParseError { offset, message })?;
    Ok(LosslessTree {
        root,
        errors: recovered,
    })
}

/// Parses a Hack file. All nodes are allocated in `arena` and borrow `src`.
/// Nothing is assumed about the file's name: see [`parse_named`].
pub fn parse<'a>(arena: &'a bumpalo::Bump, src: &'a str) -> Result<ParseOutput<'a>, ParseError> {
    parse_source(arena, src, FileKind::Other, "", None)
}

/// Parses the Hack file at `path`, whose extension decides some of what
/// HHVM accepts: an `.hhi` file is read as by [`parse_hhi`], a `.php` file
/// needs its `<?hh` header and a `.hack` file must not have one. The path
/// also appears in the few messages that name the file.
pub fn parse_named<'a>(
    arena: &'a bumpalo::Bump,
    src: &'a str,
    path: &str,
) -> Result<ParseOutput<'a>, ParseError> {
    parse_source(arena, src, FileKind::of(path), path, None)
}

/// Parses a file with an `.hhi` extension. The tree is the same as for
/// [`parse`]; the difference is in [`ParseOutput::errors`], because HHVM
/// relaxes some rules for declaration files (functions need no body, ...)
/// and ignores what is inside function bodies there.
pub fn parse_hhi<'a>(
    arena: &'a bumpalo::Bump,
    src: &'a str,
) -> Result<ParseOutput<'a>, ParseError> {
    parse_source(arena, src, FileKind::Hhi, "", None)
}

/// `failed` is the error of an earlier attempt on this file: it makes this
/// one salvage declarations straight away.
fn parse_source<'a>(
    arena: &'a bumpalo::Bump,
    src: &'a str,
    kind: FileKind,
    path: &str,
    failed: Option<ParseError>,
) -> Result<ParseOutput<'a>, ParseError> {
    // Offsets are stored as `u32`.
    if src.len() >= u32::MAX as usize {
        return Err(ParseError {
            offset: 0,
            message: "file too large",
        });
    }
    let path: &'a str = if path.is_empty() {
        ""
    } else {
        arena.alloc_str(path)
    };
    // The syntax error of the file, if it has one, and what its first
    // parse reported: that error among the rest.
    let mut failure = failed;
    let mut reported = None;
    if failed.is_none() {
        let (result, mut first) = parser::parse_program(arena, src, kind, path, false);
        match result {
            Ok(program) if !first.syntax_error_reported => {
                return Ok(parse_output(program, first, None));
            }
            // The parser recovered from the error the way HHVM does, and
            // HHVM translates the tree that results to its AST: what that
            // turns up is among what it reports.
            Ok(program) => {
                let clean = first.lowering_list.is_empty();
                let found =
                    lower::errors_after_recovery(&program, src, kind == FileKind::Hhi, clean);
                if !found.is_empty() {
                    let first_only =
                        Some(first.first_only_at).filter(|&at| at < first.lowering_list.len());
                    merge_lowering_errors(
                        &mut first.lowering_list,
                        0,
                        found,
                        &first.aborted_units,
                        first_only,
                    );
                    first.first_only_at = usize::MAX;
                }
            }
            Err((offset, message)) => {
                if message == NOT_A_HACK_FILE {
                    return Err(ParseError { offset, message });
                }
                // What was found before the parser gave up stands, and of
                // syntax errors HHVM only reports the first.
                if !first.syntax_error_reported {
                    first.report_syntax(offset, first.error_end, message);
                }
                first.settle_syntax_errors();
                first.order_errors();
            }
        }
        failure = Some(ParseError {
            offset: first.syntax_error.0,
            message: first.syntax_error.1,
        });
        reported = Some(first);
    }
    let Some(error) = failure else { unreachable!() };
    // Salvage the declarations of a file with a syntax error.
    let (Ok(program), salvaged) = parser::parse_program(arena, src, kind, path, true) else {
        return Err(error);
    };
    let mut output = parse_output(program, salvaged, Some(error));
    if let Some(first) = reported {
        output.uses_readonly = first.uses_readonly;
        output.syntax_check_errors = first.errors.len();
        output.errors = first.errors;
        output
            .errors
            .extend(first.tree_errors.into_iter().map(|(_, error)| error));
        output.lowering_errors = first.lowering_list.len();
        output.errors.extend(first.lowering_list);
        output.untranslated = first.aborted_units;
    }
    Ok(output)
}

/// Gathers what a parse produced. With `failure`, the syntax error of the
/// file, the parse was the forgiving one that salvages declarations.
fn parse_output<'a>(
    program: ast::Program<'a>,
    parser: parser::Parser<'a>,
    failure: Option<ParseError>,
) -> ParseOutput<'a> {
    let ranges = &parser.block_ranges;
    let mut range_idx = 0;
    let comments = parser
        .lexer
        .comments
        .iter()
        .map(|c| {
            while range_idx < ranges.len() && ranges[range_idx].1 < c.next_token {
                range_idx += 1;
            }
            let in_block = range_idx < ranges.len() && ranges[range_idx].0 <= c.next_token;
            Comment {
                kind: c.kind,
                start: c.start,
                end: c.end,
                next_token: c.next_token,
                in_block,
                trailing: c.trailing,
                first_leading: c.first_leading,
            }
        })
        .collect();

    // HHVM's order: the errors of its syntax checks, what its checks over
    // the AST find, then the errors of the translation to the AST.
    let mut errors = parser.errors;
    let mut syntax_check_errors = errors.len();
    errors.extend(parser.tree_errors.into_iter().map(|(_, error)| error));
    let mut lowering_errors = parser.lowering_list.len();
    let mut first_only = Some(parser.first_only_at).filter(|&at| at < lowering_errors);
    errors.extend(parser.lowering_list);
    if let Some(error) = failure {
        // What the forgiving parse found on top of that is noise.
        errors = vec![SyntaxError {
            start: error.offset,
            end: error.offset,
            message: error.message.into(),
        }];
        syntax_check_errors = 1;
        lowering_errors = 0;
        first_only = None;
    }
    ParseOutput {
        program,
        comments,
        uses_readonly: parser.uses_readonly,
        errors,
        recovered: failure,
        syntax_check_errors,
        untranslated: parser.aborted_units,
        lowering_errors,
        first_only,
    }
}
