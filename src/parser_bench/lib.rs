//! Tools for keeping Hakana's parser honest against HHVM's.
//!
//! [`compare`] parses one file with both parsers and reports whether the
//! results agree. The `parse-bench` binary runs it (and timing / memory
//! measurements) over arbitrary file lists; `tests/differential.rs` runs it
//! over the test corpus checked into this repository.

use std::path::PathBuf;
use std::sync::Arc;

use aast_parser::rust_aast_parser_types::Env as AastParserEnv;
use oxidized::aast;
use oxidized::aast_visitor::{AstParams, NodeMut, VisitorMut, visit_mut};
use oxidized::ast_defs::Pos;
use oxidized::prim_defs::Comment;
use oxidized::scoured_comments::ScouredComments;
use parser_core_types::{indexed_source_text::IndexedSourceText, source_text::SourceText};
use relative_path::{Prefix, RelativePath};

pub mod hakana_trivia;
pub mod to_oxidized;

// HHVM's readonly pass, compiled in as-is (the `aast_parser` crate does not
// export it): trees built from the native parser need the rewrite it makes
// to compare equal to HHVM's, and its errors are the reference for the
// native port of the pass (`hakana_parser::readonly_check`).
#[allow(dead_code, unused_imports, clippy::all)]
#[path = "../../third-party/hhvm/hphp/hack/src/parser/readonly_check.rs"]
mod readonly_check;

pub type Program = aast::Program<(), ()>;

/// An error reported by one of the parsers, as byte offsets plus message.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReportedError {
    pub start: usize,
    pub end: usize,
    pub message: String,
}

fn native_error(e: &hakana_parser::SyntaxError) -> ReportedError {
    ReportedError {
        start: e.start as usize,
        end: e.end as usize,
        message: e.message.to_string(),
    }
}

fn thread_arena<R>(f: impl FnOnce(&bumpalo::Bump) -> R) -> R {
    thread_local! {
        static ARENA: std::cell::RefCell<bumpalo::Bump> =
            std::cell::RefCell::new(bumpalo::Bump::new());
    }
    ARENA.with(|arena| {
        let mut arena = arena.borrow_mut();
        arena.reset();
        f(&arena)
    })
}

/// What the native parser makes of a file.
pub enum Native {
    /// Parsed; `errors` are the non-fatal errors it reported.
    Parsed {
        program: Program,
        comments: ScouredComments,
        errors: Vec<ReportedError>,
    },
    /// The file has a syntax error (or uses something unsupported), so
    /// there is no tree to compare: the errors reported for it.
    Failed(Vec<ReportedError>),
}

/// Parses with Hakana's parser and converts the result to HHVM's AST so the
/// two can be compared.
pub fn native(path: &str, contents: &str) -> Native {
    thread_arena(|arena| {
        let failed = |offset: u32, message: &str| {
            Native::Failed(vec![ReportedError {
                start: offset as usize,
                end: offset as usize,
                message: message.to_string(),
            }])
        };
        let parsed = match hakana_parser::parse_named(arena, contents, path) {
            Ok(parsed) => parsed,
            Err(e) => return failed(e.offset, e.message),
        };
        // A file with a syntax error only yields what could be salvaged
        // from it, which is not comparable with anything.
        if parsed.recovered.is_some() {
            return Native::Failed(parsed.errors.iter().map(native_error).collect());
        }
        let relative_path = Arc::new(RelativePath::make(Prefix::Root, PathBuf::from(path)));
        let uses_readonly = parsed.uses_readonly;
        let file = match parsed.lower_or_errors(arena, contents, path.ends_with(".hhi")) {
            Ok(file) => file,
            // What HHVM reports for the file, if anything, is what a file
            // that cannot be lowered is reported with.
            Err((_, errors)) if !errors.is_empty() => {
                return Native::Failed(errors.iter().map(native_error).collect());
            }
            Err((e, _)) => return failed(e.offset, e.message),
        };
        let mut program = to_oxidized::program(&file.program, &file.lines, relative_path.clone());
        let comments = to_oxidized::comments(&file.comments, &file.lines, relative_path);
        let errors = file.errors.iter().map(native_error).collect();
        if uses_readonly {
            // Only for the way HHVM's pass rewrites the tree; the errors of
            // the readonly analysis are the native pass's, among `errors`.
            readonly_check::check_program(&mut program, true);
        }
        Native::Parsed {
            program,
            comments,
            errors,
        }
    })
}

// Hakana has always asked HHVM for `parent` as an identifier rather than
// `CIparent`; the bridge produces that form directly.
struct ParentClassIds;

impl<'ast> VisitorMut<'ast> for ParentClassIds {
    type Params = AstParams<(), std::convert::Infallible>;

    fn object(&mut self) -> &mut dyn VisitorMut<'ast, Params = Self::Params> {
        self
    }

    fn visit_class_id(
        &mut self,
        context: &mut (),
        id: &'ast mut aast::ClassId<(), ()>,
    ) -> Result<(), std::convert::Infallible> {
        if matches!(id.2, aast::ClassId_::CIparent) {
            id.2 = aast::ClassId_::CIexpr(aast::Expr(
                (),
                id.1.clone(),
                aast::Expr_::Id(Box::new(oxidized::ast_defs::Id(
                    id.1.clone(),
                    "parent".to_owned(),
                ))),
            ));
        }
        id.recurse(context, self)
    }
}

/// Parses with HHVM's parser, configured the way Hakana has always used it,
/// and reports comments the way Hakana attaches them (see `hakana_trivia`).
/// `Err` means HHVM could not produce a tree at all.
pub fn hhvm(
    path: &str,
    contents: &str,
) -> Result<(Program, ScouredComments, Vec<ReportedError>), String> {
    hhvm_with(path, contents, true)
}

/// [`hhvm`] with nothing added: the tree, errors and comments exactly as
/// HHVM's own pipeline produces them. This is what the benchmarks time and
/// measure.
pub fn hhvm_unmodified(
    path: &str,
    contents: &str,
) -> Result<(Program, ScouredComments, Vec<ReportedError>), String> {
    hhvm_with(path, contents, false)
}

fn hhvm_with(
    path: &str,
    contents: &str,
    hakana_comments: bool,
) -> Result<(Program, ScouredComments, Vec<ReportedError>), String> {
    let relative_path = Arc::new(RelativePath::make(Prefix::Root, PathBuf::from(path)));
    let text = SourceText::make(relative_path.clone(), contents.as_bytes());
    let indexed_source_text = IndexedSourceText::new(text.clone());

    let mut parser_env = AastParserEnv {
        include_line_comments: true,
        scour_comments: true,
        ..AastParserEnv::default()
    };
    parser_env.parser_options.enable_xhp_class_modifier = true;
    parser_env.parser_options.allow_unstable_features = true;

    let parser_result = match aast_parser::AastParser::from_text(
        &parser_env,
        &indexed_source_text,
        std::collections::HashSet::default(),
    ) {
        Ok(parser_result) => parser_result,
        Err(aast_parser::Error::ParserFatal(err, _)) => return Err(err.message.to_string()),
        Err(_) => return Err("not a Hack file".to_string()),
    };

    let mut errors = vec![];
    for e in parser_result.syntax_errors {
        errors.push(ReportedError {
            start: e.start_offset,
            end: e.end_offset,
            message: e.message.to_string(),
        });
    }
    for (pos, message) in parser_result.lowerer_parsing_errors {
        errors.push(ReportedError {
            start: pos.start_offset(),
            end: pos.end_offset(),
            message,
        });
    }

    let mut program = parser_result.aast;
    visit_mut(&mut ParentClassIds, &mut (), &mut program).unwrap();
    // Comments are only ever compared for files without errors, and in a
    // broken file HHVM's trivia is not something to scan again.
    if !hakana_comments || !errors.is_empty() {
        return Ok((program, parser_result.scoured_comments, errors));
    }
    // Comments are reported the way Hakana attaches them (see
    // `hakana_trivia`), which takes HHVM's lossless tree.
    let arena = bumpalo::Bump::new();
    let (root, _, _) = positioned_by_ref_parser::parse_script(
        &arena,
        &text,
        parser_core_types::parser_env::ParserEnv {
            enable_xhp_class_modifier: true,
            ..Default::default()
        },
    );
    let mut comments = hakana_trivia::scour(&root, &indexed_source_text, &arena, &relative_path);
    fixup_comments(&mut comments, &relative_path);
    Ok((program, comments, errors))
}

/// Hakana's historical correction of HHVM's comment positions.
fn fixup_comments(scoured_comments: &mut ScouredComments, relative_path: &Arc<RelativePath>) {
    for (pos, comment) in scoured_comments.comments.iter_mut() {
        match comment {
            Comment::CmtLine(_) => {
                let mut offsets = pos.to_start_and_end_lnum_bol_offset();
                offsets.0.2 -= 2;
                *pos = Pos::from_lnum_bol_offset(relative_path.clone(), offsets.0, offsets.1);
            }
            Comment::CmtBlock(text) => {
                let mut offsets = pos.to_start_and_end_lnum_bol_offset();
                let newline_count = text.as_bytes().iter().filter(|&&c| c == b'\n').count();
                let comment_length = text.len();

                offsets.0.0 -= newline_count;
                offsets.0.2 -= comment_length + 2;
                if newline_count > 0 {
                    offsets.0.1 = offsets.0.2;
                }
                offsets.1.2 += 1;
                *pos = Pos::from_lnum_bol_offset(relative_path.clone(), offsets.0, offsets.1);
            }
        }
    }
    scoured_comments
        .comments
        .sort_by(|(a, _), (b, _)| a.start_offset().cmp(&b.start_offset()));
}

/// A lazily produced debug dump of one side of a mismatch.
pub type Dump = Box<dyn FnOnce() -> String>;

pub enum Outcome {
    /// Both parsers produced the same AST, comments and fixmes.
    Same,
    /// Neither parser accepts the file.
    BothReject,
    /// The native parser could not parse a file HHVM parses cleanly.
    NativeDeclined,
    /// The native parser accepted a file that HHVM cannot parse at all.
    HhvmRejects,
    /// The native parser accepted a file for which HHVM reports errors
    /// (typically one of its semantic checks on otherwise valid syntax).
    HhvmReportsErrors(String),
    /// The native parser reports errors for a file HHVM considers clean.
    NativeReportsErrors(String),
    /// Both parsers accepted the file but produced different results.
    Mismatch {
        what: String,
        native: Dump,
        hhvm: Dump,
    },
}

pub fn compare(path: &str, contents: String) -> Outcome {
    let native = native(path, &contents);
    // HHVM's parser panics on a few pathological inputs.
    let hhvm = std::panic::catch_unwind(|| hhvm(path, &contents))
        .unwrap_or_else(|_| Err("HHVM's parser panicked".to_string()));
    match (native, hhvm) {
        (Native::Failed(_), Err(_)) => Outcome::BothReject,
        (Native::Failed(_), Ok(h)) => {
            if h.2.is_empty() {
                Outcome::NativeDeclined
            } else {
                Outcome::BothReject
            }
        }
        (Native::Parsed { .. }, Err(_)) => Outcome::HhvmRejects,
        (
            Native::Parsed {
                program,
                comments,
                errors,
            },
            Ok((hhvm_program, hhvm_comments, hhvm_errors)),
        ) => {
            let aast_ok = program == hhvm_program;
            let comments_ok = comments.comments == hhvm_comments.comments;
            let fixmes_ok = comments.fixmes == hhvm_comments.fixmes
                && comments.ignores == hhvm_comments.ignores
                && comments.misuses == hhvm_comments.misuses;
            if !(aast_ok && comments_ok && fixmes_ok) {
                // A tree mismatch only counts when both parsers consider the
                // file valid; HHVM's error recovery produces arbitrary trees.
                if hhvm_errors.is_empty() || !errors.is_empty() {
                    if !hhvm_errors.is_empty() {
                        return Outcome::BothReject;
                    }
                    let mut what = vec![];
                    if !aast_ok {
                        what.push("ast");
                    }
                    if !comments_ok {
                        what.push("comments");
                    }
                    if !fixmes_ok {
                        what.push("fixmes");
                    }
                    let (native, hhvm): (Dump, Dump) = if !aast_ok {
                        (
                            Box::new(move || format!("{:#?}", program)),
                            Box::new(move || format!("{:#?}", hhvm_program)),
                        )
                    } else if !comments_ok {
                        (
                            Box::new(move || format!("{:#?}", comments.comments)),
                            Box::new(move || format!("{:#?}", hhvm_comments.comments)),
                        )
                    } else {
                        (
                            Box::new(move || {
                                format!(
                                    "{:#?}\n{:#?}\n{:#?}",
                                    comments.fixmes, comments.ignores, comments.misuses
                                )
                            }),
                            Box::new(move || {
                                format!(
                                    "{:#?}\n{:#?}\n{:#?}",
                                    hhvm_comments.fixmes,
                                    hhvm_comments.ignores,
                                    hhvm_comments.misuses
                                )
                            }),
                        )
                    };
                    return Outcome::Mismatch {
                        what: what.join(", "),
                        native,
                        hhvm,
                    };
                }
            }
            match (errors.is_empty(), hhvm_errors.first()) {
                (true, None) => Outcome::Same,
                (true, Some(first)) => Outcome::HhvmReportsErrors(first.message.clone()),
                (false, None) => Outcome::NativeReportsErrors(errors[0].message.clone()),
                (false, Some(_)) => Outcome::BothReject,
            }
        }
    }
}

/// How the error reports of the two parsers relate for one file.
pub enum ErrorOutcome {
    /// Neither reports anything.
    BothClean,
    /// Both report exactly the same errors (message and offsets).
    SameErrors,
    /// Both report the same errors, but not in the same order (or one
    /// repeats an error the other reports once).
    DifferentOrder {
        native: Vec<ReportedError>,
        hhvm: Vec<ReportedError>,
    },
    /// Both report errors, but not the same ones.
    DifferentErrors {
        native: Vec<ReportedError>,
        hhvm: Vec<ReportedError>,
    },
    /// HHVM reports errors the native parser misses entirely.
    Missed(Vec<ReportedError>),
    /// The native parser reports errors (or fails) on a file HHVM accepts.
    FalsePositive(Vec<ReportedError>),
}

/// Compares only the errors the two parsers report for a file, order
/// included: Hakana shows them in the order they come. A file the native
/// parser cannot parse counts as one error at the failure offset.
pub fn compare_errors(path: &str, contents: &str) -> ErrorOutcome {
    let mut native_errors = match native(path, contents) {
        Native::Parsed { errors, .. } => errors,
        Native::Failed(errors) => errors,
    };
    let mut hhvm_errors = match std::panic::catch_unwind(|| hhvm(path, contents)) {
        Ok(Ok((_, _, errors))) => errors,
        Ok(Err(message)) => vec![ReportedError {
            start: 0,
            end: 0,
            message,
        }],
        Err(_) => vec![ReportedError {
            start: 0,
            end: 0,
            message: "HHVM's parser panicked".to_string(),
        }],
    };
    let in_order = native_errors == hhvm_errors;
    let (native_in_order, hhvm_in_order) = (native_errors.clone(), hhvm_errors.clone());
    native_errors.sort();
    native_errors.dedup();
    hhvm_errors.sort();
    hhvm_errors.dedup();
    match (native_errors.is_empty(), hhvm_errors.is_empty()) {
        (true, true) => ErrorOutcome::BothClean,
        (true, false) => ErrorOutcome::Missed(hhvm_errors),
        (false, true) => ErrorOutcome::FalsePositive(native_errors),
        (false, false) => {
            if in_order {
                ErrorOutcome::SameErrors
            } else if native_errors == hhvm_errors {
                ErrorOutcome::DifferentOrder {
                    native: native_in_order,
                    hhvm: hhvm_in_order,
                }
            } else {
                ErrorOutcome::DifferentErrors {
                    native: native_errors,
                    hhvm: hhvm_errors,
                }
            }
        }
    }
}

/// What the readonly analysis makes of one file's tree.
pub struct ReadonlyOutcome {
    /// The file uses `readonly` where it makes HHVM (and Hakana) run the
    /// analysis.
    pub uses_readonly: bool,
    /// How many errors the analysis reports.
    pub errors: usize,
    /// How the native analysis disagrees with HHVM's, if it does.
    pub difference: Option<String>,
}

/// Runs the native readonly analysis and HHVM's over the tree the native
/// parser builds for a file, whether or not the file uses `readonly`, and
/// compares what they report: the same errors, in the same order, both when
/// checking for the typechecker and for the compiler. `None` if the native
/// parser has no tree for the file.
pub fn compare_readonly(path: &str, contents: &str) -> Option<ReadonlyOutcome> {
    thread_arena(|arena| {
        let is_hhi = path.ends_with(".hhi");
        let parsed = if is_hhi {
            hakana_parser::parse_hhi(arena, contents)
        } else {
            hakana_parser::parse(arena, contents)
        }
        .ok()?;
        if parsed.recovered.is_some() {
            return None;
        }
        let lowered =
            hakana_parser::lower::lower(arena, &parsed.program, &parsed.comments, contents, is_hhi)
                .ok()?;
        let relative_path = Arc::new(RelativePath::make(Prefix::Root, PathBuf::from(path)));
        let mut outcome = ReadonlyOutcome {
            uses_readonly: parsed.uses_readonly,
            errors: 0,
            difference: None,
        };
        // The native analysis tells expressions apart by their address.
        if let Some(pos) = shared_expression(&lowered.program) {
            outcome.difference = Some(format!(
                "the expression at line {} is in two places of the native tree",
                pos.line(&lowered.lines)
            ));
            return Some(outcome);
        }
        for is_typechecker in [true, false] {
            let native: Vec<ReportedError> =
                hakana_parser::readonly_check::check_program(&lowered.program, is_typechecker)
                    .iter()
                    .map(native_error)
                    .collect();
            // HHVM's pass rewrites the tree it checks, so each run gets its own.
            let mut program =
                to_oxidized::program(&lowered.program, &lowered.lines, relative_path.clone());
            let hhvm: Vec<ReportedError> =
                readonly_check::check_program(&mut program, is_typechecker)
                    .into_iter()
                    .map(|e| ReportedError {
                        start: e.start_offset,
                        end: e.end_offset,
                        message: e.message.to_string(),
                    })
                    .collect();
            if is_typechecker {
                outcome.errors = hhvm.len();
            }
            if native != hhvm {
                let show = |e: Option<&ReportedError>| match e {
                    Some(e) => format!("[{}-{}] {}", e.start, e.end, e.message),
                    None => "nothing".to_string(),
                };
                let at = native
                    .iter()
                    .zip(&hhvm)
                    .position(|(n, h)| n != h)
                    .unwrap_or(native.len().min(hhvm.len()));
                outcome.difference = Some(format!(
                    "error {} of {} (native) / {} (hhvm){}: native {} | hhvm {}",
                    at + 1,
                    native.len(),
                    hhvm.len(),
                    if is_typechecker {
                        ""
                    } else {
                        ", for the compiler"
                    },
                    show(native.get(at)),
                    show(hhvm.get(at)),
                ));
                break;
            }
        }
        Some(outcome)
    })
}

/// The position of an expression node that is reachable along two paths in
/// the tree, if there is one. That is expected in one case, which the native
/// readonly analysis allows for: a property made from a constructor parameter
/// has the parameter's attributes, so what a method contains may have been
/// seen before the method.
fn shared_expression(
    program: &hakana_parser::aast::Program<'_>,
) -> Option<hakana_parser::aast::Pos> {
    use hakana_parser::aast;
    use hakana_parser::aast_visitor::{self, Node};

    #[derive(Default)]
    struct Seen(rustc_hash::FxHashSet<*const ()>);

    impl<'a> aast_visitor::Visitor<'a> for Seen {
        type Params = aast_visitor::AstParams<(), aast::Pos>;

        fn object(&mut self) -> &mut dyn aast_visitor::Visitor<'a, Params = Self::Params> {
            self
        }

        fn visit_method_(&mut self, c: &mut (), m: &'a aast::Method_<'a>) -> Result<(), aast::Pos> {
            self.0.clear();
            m.recurse(c, self)
        }

        fn visit_expr(&mut self, c: &mut (), p: &'a aast::Expr<'a>) -> Result<(), aast::Pos> {
            if !self.0.insert(std::ptr::from_ref(p).cast()) {
                return Err(p.1);
            }
            p.recurse(c, self)
        }
    }

    aast_visitor::visit(&mut Seen::default(), &mut (), program).err()
}

/// Variants of a file with `readonly` added in the places a simple textual
/// substitution can reach: on the right of assignments, on arguments,
/// parameters, returned values, return types and methods. Few files use
/// `readonly`, and those that do use it correctly; the variants (those that
/// still parse) put the analysis to work on everything else. The first
/// variants each make one kind of change, the last one makes them all.
pub fn readonly_variants(contents: &str) -> Vec<String> {
    const INJECTIONS: [(&str, &str); 14] = [
        (" = $", " = readonly $"),
        (" = new ", " = readonly new "),
        ("return $", "return readonly $"),
        ("throw $", "throw readonly $"),
        ("yield $", "yield readonly $"),
        ("($", "(readonly $"),
        (", $", ", readonly $"),
        ("={$", "={readonly $"),
        ("(int $", "(readonly int $"),
        ("(string $", "(readonly string $"),
        (", int $", ", readonly int $"),
        (", string $", ", readonly string $"),
        ("): ", "): readonly "),
        ("public function ", "public readonly function "),
    ];
    let mut variants = vec![];
    let mut all = contents.to_string();
    for (from, to) in INJECTIONS {
        if contents.contains(from) {
            variants.push(contents.replace(from, to));
            all = all.replace(from, to);
        }
    }
    if variants.len() > 1 {
        variants.push(all);
    }
    variants
}

/// HHVM's lossless syntax tree for a file, in the form Hakana's parser
/// produces its own ([`hakana_parser::cst`]), and every syntax error HHVM's
/// parser reports while building it. (Of those only the first makes it into
/// what [`hhvm`] returns; the others show how the parser recovered.)
pub fn hhvm_cst(path: &str, contents: &str) -> (hakana_parser::cst::Node, Vec<ReportedError>) {
    hhvm_cst_with(path, contents, hakana_parser::ParserOptions::default())
}

/// [`hhvm_cst`] with HHVM's parser set up the way `options` say.
pub fn hhvm_cst_with(
    path: &str,
    contents: &str,
    options: hakana_parser::ParserOptions,
) -> (hakana_parser::cst::Node, Vec<ReportedError>) {
    use hakana_parser::cst;
    use parser_core_types::lexable_token::LexableToken;
    use parser_core_types::syntax_by_ref::positioned_syntax::PositionedSyntax;
    use parser_core_types::syntax_kind::SyntaxKind;

    fn convert(n: &PositionedSyntax<'_>) -> cst::Node {
        if let Some(t) = n.get_token() {
            let start = t.start_offset();
            return cst::Node::Token(cst::Token {
                kind: cst::TokenKind::ALL[t.kind() as u8 as usize],
                offset: (start - t.leading_width()) as u32,
                leading_width: t.leading_width() as u32,
                width: t.width() as u32,
                trailing_width: t.trailing_width() as u32,
            });
        }
        match n.kind() {
            SyntaxKind::Missing => cst::Node::Missing,
            SyntaxKind::SyntaxList => cst::Node::List(n.iter_children().map(convert).collect()),
            kind => {
                let name = format!("{kind:?}");
                let kind = cst::SyntaxKind::from_name(&name)
                    .unwrap_or_else(|| panic!("unknown syntax kind {name}; regenerate kinds.rs"));
                cst::Node::Syntax(kind, n.iter_children().map(convert).collect())
            }
        }
    }

    let arena = bumpalo::Bump::new();
    let rel = Arc::new(RelativePath::make(Prefix::Root, PathBuf::from(path)));
    let source = SourceText::make(rel, contents.as_bytes());
    let env = parser_core_types::parser_env::ParserEnv {
        enable_xhp_class_modifier: options.enable_xhp_class_modifier,
        disable_xhp_children_declarations: options.disable_xhp_children_declarations,
        ..Default::default()
    };
    let (root, mut errors, _state) = positioned_by_ref_parser::parse_script(&arena, &source, env);
    // The order HHVM puts them in: by position, those found first in
    // front, without repetitions.
    errors.reverse();
    errors.sort_by_key(|e| (e.start_offset, e.end_offset));
    let mut errors: Vec<ReportedError> = errors
        .into_iter()
        .map(|e| ReportedError {
            start: e.start_offset,
            end: e.end_offset,
            message: e.message.to_string(),
        })
        .collect();
    errors.dedup();
    let mut tree = convert(&root);
    hakana_trivia::repartition(&mut tree, contents.as_bytes());
    (tree, errors)
}

/// How the lossless tree of Hakana's parser compares with HHVM's for a file.
pub enum CstOutcome {
    Same,
    /// HHVM's parser recovers from this file's syntax errors and still
    /// builds a tree; the native parser gave up, so it has none.
    NoRecovery(String),
    /// The native parser could not produce a tree for a file HHVM parses
    /// without syntax errors.
    NativeFailed(String),
    /// Where the trees first differ.
    Different(String),
}

pub fn compare_cst(path: &str, contents: &str) -> CstOutcome {
    compare_cst_with(path, contents, hakana_parser::ParserOptions::default())
}

/// [`compare_cst`] with both parsers set up the way `options` say.
pub fn compare_cst_with(
    path: &str,
    contents: &str,
    options: hakana_parser::ParserOptions,
) -> CstOutcome {
    let (hhvm, hhvm_errors) = hhvm_cst_with(path, contents, options);
    match hakana_parser::parse_cst_named(contents, path, options) {
        Err(e) => {
            let line = contents[..(e.offset as usize).min(contents.len())]
                .matches('\n')
                .count()
                + 1;
            let why = format!("line {line}: {}", e.message);
            if !hhvm_errors.is_empty() {
                CstOutcome::NoRecovery(why)
            } else {
                CstOutcome::NativeFailed(why)
            }
        }
        Ok(native) => {
            let native_errors: Vec<ReportedError> =
                native.errors.iter().map(native_error).collect();
            let native = native.root;
            // With CST_TOKENS=1 only the token sequences are compared: kinds,
            // extents and trivia, whatever nodes they sit in.
            if std::env::var_os("CST_TOKENS").is_some() {
                fn tokens(n: &hakana_parser::cst::Node, out: &mut Vec<hakana_parser::cst::Token>) {
                    match n {
                        hakana_parser::cst::Node::Token(t) => out.push(*t),
                        _ => n.children().iter().for_each(|c| tokens(c, out)),
                    }
                }
                let (mut a, mut b) = (vec![], vec![]);
                tokens(&native, &mut a);
                tokens(&hhvm, &mut b);
                return match a.iter().zip(&b).position(|(x, y)| x != y) {
                    Some(i) => CstOutcome::Different(format!(
                        "token {i}: native {:?} vs hhvm {:?}",
                        a[i], b[i]
                    )),
                    None if a.len() != b.len() => {
                        CstOutcome::Different(format!("{} tokens vs {}", a.len(), b.len()))
                    }
                    None => CstOutcome::Same,
                };
            }
            if let Some(difference) = hakana_parser::cst::first_difference(&native, &hhvm) {
                return CstOutcome::Different(difference);
            }
            // The trees agree; so should the errors recovered from.
            let show = |e: Option<&ReportedError>| match e {
                Some(e) => format!("[{}-{}] {}", e.start, e.end, e.message),
                None => "nothing".to_string(),
            };
            let at = native_errors
                .iter()
                .zip(&hhvm_errors)
                .position(|(n, h)| n != h)
                .unwrap_or(native_errors.len().min(hhvm_errors.len()));
            if at < native_errors.len().max(hhvm_errors.len()) {
                return CstOutcome::Different(format!(
                    "syntax error {} of {} (native) / {} (hhvm): native {} | hhvm {}",
                    at + 1,
                    native_errors.len(),
                    hhvm_errors.len(),
                    show(native_errors.get(at)),
                    show(hhvm_errors.get(at)),
                ));
            }
            CstOutcome::Same
        }
    }
}
