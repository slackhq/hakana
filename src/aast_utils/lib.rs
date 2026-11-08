use hakana_code_info::code_location::{FilePath, HPos};
use hakana_code_info::file_info::ParserError;
use hakana_parser::aast;
use hakana_parser::aast::ScouredComments;
use hakana_parser::aast_visitor::visit;
use hakana_str::{StrId, ThreadedInterner};
use name_context::NameContext;
use naming_visitor::Scanner;
use rustc_hash::FxHashMap;

pub use bumpalo::Bump;
pub use hakana_parser::LineIndex;

thread_local! {
    static ARENAS: std::cell::RefCell<Vec<Bump>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// An arena for one file's source and tree, taken from a per-thread pool.
///
/// Files are parsed by the thousand, and a fresh `Bump` for each means a large
/// allocation and release per file; reusing them keeps that memory mapped.
pub struct FileArena(std::mem::ManuallyDrop<Bump>);

impl FileArena {
    /// Arenas that grew beyond this are released rather than kept.
    const MAX_POOLED_BYTES: usize = 32 * 1024 * 1024;

    pub fn new() -> Self {
        let arena = ARENAS
            .with(|arenas| arenas.borrow_mut().pop())
            .unwrap_or_default();
        FileArena(std::mem::ManuallyDrop::new(arena))
    }
}

impl Default for FileArena {
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for FileArena {
    type Target = Bump;

    #[inline]
    fn deref(&self) -> &Bump {
        &self.0
    }
}

impl Drop for FileArena {
    fn drop(&mut self) {
        // SAFETY: the arena is not used again after being taken here.
        let mut arena = unsafe { std::mem::ManuallyDrop::take(&mut self.0) };
        if arena.allocated_bytes() > Self::MAX_POOLED_BYTES {
            return;
        }
        arena.reset();
        // Ignore the pool if the thread is shutting down.
        let _ = ARENAS.try_with(|arenas| {
            let mut arenas = arenas.borrow_mut();
            if arenas.len() < 4 {
                arenas.push(arena);
            }
        });
    }
}

pub mod name_context;
mod naming_visitor;

/// A parsed file: the tree (allocated in the caller's arena and borrowing the
/// source text), its comments, the errors found, and whether one of them is a
/// syntax error that left only the file's declarations (without function
/// bodies) in the tree.
pub struct AastParseResult<'a> {
    pub program: aast::Program<'a>,
    pub comments: ScouredComments,
    /// The file's line table: positions in the tree are byte ranges.
    pub lines: hakana_parser::LineIndex,
    pub parser_errors: Vec<ParserError>,
    /// One of the errors is a syntax error, and `program` only declares
    /// what could be salvaged (without function bodies).
    pub has_syntax_error: bool,
}

/// Parses a Hack file with Hakana's parser.
///
/// The tree lives in `arena` and borrows `file_contents`, so both have to
/// outlive it. Errors come back alongside the tree, including a syntax error:
/// the tree then holds the declarations that could be salvaged. `Err` is for
/// files that are not Hack at all.
pub fn get_aast_for_path_and_contents<'a>(
    arena: &'a Bump,
    file_path: FilePath,
    file_path_str: &str,
    file_contents: &'a str,
) -> Result<AastParseResult<'a>, ParserError> {
    match hakana_parser::parse_file(arena, file_contents, file_path_str) {
        Ok(parsed) => {
            let errors = parsed
                .errors
                .iter()
                .map(|e| ParserError::SyntaxError {
                    message: e.message.to_string(),
                    pos: error_pos(file_path, file_contents, e.start, e.end),
                })
                .collect();
            Ok(AastParseResult {
                program: parsed.program,
                comments: parsed.comments,
                lines: parsed.lines,
                parser_errors: errors,
                has_syntax_error: parsed.has_syntax_error,
            })
        }
        Err(e) if e.message == hakana_parser::NOT_A_HACK_FILE => Err(ParserError::NotAHackFile),
        Err(e) => Err(ParserError::SyntaxError {
            message: e.message.to_string(),
            pos: error_pos(FilePath(StrId::EMPTY), file_contents, e.offset, e.offset),
        }),
    }
}

/// The position of a syntax error, from its byte offsets.
fn error_pos(file_path: FilePath, file_contents: &str, start: u32, end: u32) -> HPos {
    let bytes = file_contents.as_bytes();
    let start = (start as usize).min(bytes.len());
    let end = (end as usize).clamp(start, bytes.len());
    let locate = |offset: usize| {
        let before = &bytes[..offset];
        let line = before.iter().filter(|&&b| b == b'\n').count() + 1;
        let bol = before
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1);
        (line as u32, (offset - bol) as u16 + 1)
    };
    let (start_line, start_column) = locate(start);
    let (end_line, end_column) = locate(end);
    HPos {
        file_path,
        start_offset: start as u32,
        end_offset: end as u32,
        start_line,
        end_line,
        start_column,
        end_column,
    }
}

pub struct Uses {
    pub symbol_uses: FxHashMap<StrId, Vec<(StrId, StrId)>>,
    pub symbol_member_uses: FxHashMap<(StrId, StrId), Vec<(StrId, StrId)>>,
}

pub fn scope_names<'ast>(
    program: &'ast aast::Program<'ast>,
    interner: &mut ThreadedInterner,
    mut name_context: NameContext<'ast>,
) -> (FxHashMap<u32, StrId>, Uses) {
    let mut scanner = Scanner {
        interner,
        resolved_names: FxHashMap::default(),
        symbol_uses: FxHashMap::default(),
        symbol_member_uses: FxHashMap::default(),
        file_uses: vec![],
    };

    visit(&mut scanner, &mut name_context, program).unwrap();
    (
        scanner.resolved_names,
        Uses {
            symbol_uses: scanner.symbol_uses,
            symbol_member_uses: scanner.symbol_member_uses,
        },
    )
}
