use aast_parser::rust_aast_parser_types::Env as AastParserEnv;

use hakana_reflection_info::code_location::HPos;
use hakana_reflection_info::{StrId, ThreadedInterner};
use name_context::NameContext;
use naming_visitor::Scanner;
use ocamlrep::rc::RcOc;
use oxidized::ast_defs::Pos;
use oxidized::prim_defs::Comment;
use oxidized::scoured_comments::ScouredComments;
use oxidized::{aast, aast_visitor::visit};
use parser_core_types::{indexed_source_text::IndexedSourceText, source_text::SourceText};
use relative_path::{Prefix, RelativePath};
use rustc_hash::FxHashMap;
use std::fs::File;
use std::io::Write;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub mod name_context;
mod naming_visitor;

#[derive(Debug)]
pub enum ParserError {
    NotAHackFile,
    SyntaxError { message: String, pos: HPos },
}

pub fn get_aast_for_path_and_contents(
    local_path: String,
    file_contents: String,
    aast_cache_dir: Option<String>,
) -> Result<(aast::Program<(), ()>, ScouredComments, String), ParserError> {
    let path_hash = xxhash_rust::xxh3::xxh3_64(local_path.as_bytes());

    let cache_path = if let Some(cache_dir) = aast_cache_dir {
        if !Path::new(&cache_dir).is_dir() {
            if fs::create_dir(&cache_dir).is_err() {
                panic!("could not create aast cache directory");
            }
        }

        Some(format!("{}/{:x}", cache_dir, path_hash))
    } else {
        None
    };

    let rc_path = RcOc::new(RelativePath::make(Prefix::Root, PathBuf::from(&local_path)));

    let text = SourceText::make(rc_path.clone(), file_contents.as_bytes());
    let indexed_source_text = IndexedSourceText::new(text.clone());

    let mut parser_env = AastParserEnv::default();
    parser_env.keep_errors = true;
    parser_env.parser_options.po_disable_hh_ignore_error = 0;
    parser_env.include_line_comments = true;
    parser_env.scour_comments = true;
    parser_env.parser_options.po_enable_xhp_class_modifier = true;

    let mut parser_result =
        match aast_parser::AastParser::from_text(&parser_env, &indexed_source_text) {
            Ok(parser_result) => parser_result,
            Err(err) => {
                return Err(match err {
                    aast_parser::Error::ParserFatal(err, pos) => ParserError::SyntaxError {
                        message: err.message.to_string(),
                        pos: HPos::new(&pos, StrId(0), None),
                    },
                    _ => ParserError::NotAHackFile,
                })
            }
        };

    if !parser_result.syntax_errors.is_empty() {
        let first_error = &parser_result.syntax_errors[0];

        let lines = file_contents[0..first_error.start_offset]
            .split("\n")
            .collect::<Vec<_>>();
        let column = lines.last().unwrap().len();
        let line_count = lines.len();

        return Err(ParserError::SyntaxError {
            message: first_error.message.to_string(),
            pos: HPos {
                file_path: StrId(0),
                start_offset: first_error.start_offset,
                end_offset: first_error.end_offset,
                start_line: line_count,
                end_line: line_count,
                start_column: column,
                end_column: column,
                insertion_start: None,
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
                *pos = Pos::from_lnum_bol_offset(rc_path.clone(), offsets.0, offsets.1);
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
                *pos = Pos::from_lnum_bol_offset(rc_path.clone(), offsets.0, offsets.1);
            }
        }
    }

    // reorder so single line and multiline comments are intermingled
    parser_result
        .scoured_comments
        .comments
        .sort_by(|(a, _), (b, _)| a.start_offset().cmp(&b.start_offset()));

    if let Some(cache_path) = cache_path {
        let mut file = File::create(&cache_path).unwrap();
        let serialized_aast =
            bincode::serialize(&(&aast, &parser_result.scoured_comments)).unwrap();
        file.write_all(&serialized_aast)
            .unwrap_or_else(|_| panic!("Could not write file {}", &cache_path));
    }

    Ok((aast, parser_result.scoured_comments, file_contents))
}

pub struct Uses {
    pub symbol_uses: FxHashMap<StrId, Vec<(StrId, StrId)>>,
    pub symbol_member_uses: FxHashMap<(StrId, StrId), Vec<(StrId, StrId)>>,
}

pub fn scope_names(
    program: &aast::Program<(), ()>,
    interner: &mut ThreadedInterner,
    mut name_context: NameContext,
) -> (FxHashMap<usize, StrId>, Uses) {
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
