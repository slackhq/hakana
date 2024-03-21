use aast_parser::rust_aast_parser_types::Env as AastParserEnv;

use hakana_reflection_info::code_location::{FilePath, HPos};
use hakana_reflection_info::file_info::ParserError;
use hakana_str::{StrId, ThreadedInterner};
use name_context::NameContext;
use naming_visitor::Scanner;
use oxidized::ast_defs::Pos;
use oxidized::prim_defs::Comment;
use oxidized::scoured_comments::ScouredComments;
use oxidized::{aast, aast_visitor::visit};
use parser_core_types::{indexed_source_text::IndexedSourceText, source_text::SourceText};
use relative_path::{Prefix, RelativePath};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::PathBuf;
use std::sync::Arc;

pub mod name_context;
mod naming_visitor;

pub fn get_aast_for_path_and_contents(
    file_path: FilePath,
    file_path_str: &str,
    file_contents: String,
) -> Result<(aast::Program<(), ()>, ScouredComments, String), ParserError> {
    let relative_path = Arc::new(RelativePath::make(
        Prefix::Root,
        PathBuf::from(&file_path_str),
    ));

    let text = SourceText::make(relative_path.clone(), file_contents.as_bytes());
    let indexed_source_text = IndexedSourceText::new(text.clone());

    let mut parser_env = AastParserEnv::default();
    parser_env.parser_options.po_disable_hh_ignore_error = 0;
    parser_env.include_line_comments = true;
    parser_env.scour_comments = true;
    parser_env.parser_options.po_enable_xhp_class_modifier = true;

    let mut parser_result = match aast_parser::AastParser::from_text(
        &parser_env,
        &indexed_source_text,
        FxHashSet::default(),
    ) {
        Ok(parser_result) => parser_result,
        Err(err) => {
            return Err(match err {
                aast_parser::Error::ParserFatal(err, pos) => ParserError::SyntaxError {
                    message: err.message.to_string(),
                    pos: HPos::new(&pos, FilePath(StrId::EMPTY)),
                },
                _ => ParserError::NotAHackFile,
            })
        }
    };

    if !parser_result.syntax_errors.is_empty() {
        let first_error = &parser_result.syntax_errors[0];

        let lines = file_contents[0..first_error.start_offset]
            .split('\n')
            .collect::<Vec<_>>();
        let column = lines.last().unwrap().len();
        let line_count = lines.len();

        return Err(ParserError::SyntaxError {
            message: first_error.message.to_string(),
            pos: HPos {
                file_path,
                start_offset: first_error.start_offset as u32,
                end_offset: first_error.end_offset as u32,
                start_line: line_count as u32,
                end_line: line_count as u32,
                start_column: (column as u16) + 1,
                end_column: (column as u16) + 1,
            },
        });
    }

    let aast = parser_result.aast;

    // rewrite positional data for comments because it comes out wrong in the AST
    for (pos, comment) in parser_result.scoured_comments.comments.iter_mut() {
        match comment {
            Comment::CmtLine(_) => {
                let mut offsets = pos.to_start_and_end_lnum_bol_offset();
                offsets.0 .2 -= 2;
                *pos = Pos::from_lnum_bol_offset(relative_path.clone(), offsets.0, offsets.1);
            }
            Comment::CmtBlock(text) => {
                let mut offsets = pos.to_start_and_end_lnum_bol_offset();
                let newline_count = text.as_bytes().iter().filter(|&&c| c == b'\n').count();
                let comment_length = text.len();

                offsets.0 .0 -= newline_count;
                offsets.0 .2 -= comment_length + 2;
                if newline_count > 0 {
                    // we lose the true bol here for the comment, which is a shame
                    offsets.0 .1 = offsets.0 .2;
                }
                offsets.1 .2 += 1;
                *pos = Pos::from_lnum_bol_offset(relative_path.clone(), offsets.0, offsets.1);
            }
        }
    }

    // reorder so single line and multiline comments are intermingled
    parser_result
        .scoured_comments
        .comments
        .sort_by(|(a, _), (b, _)| a.start_offset().cmp(&b.start_offset()));

    Ok((aast, parser_result.scoured_comments, file_contents))
}

pub struct Uses {
    pub symbol_uses: FxHashMap<StrId, Vec<(StrId, StrId)>>,
    pub symbol_member_uses: FxHashMap<(StrId, StrId), Vec<(StrId, StrId)>>,
}

pub fn scope_names<'ast>(
    program: &'ast aast::Program<(), ()>,
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
