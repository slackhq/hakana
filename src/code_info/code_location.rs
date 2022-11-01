use oxidized::ast::Pos;
use serde::{Deserialize, Serialize};

use crate::codebase_info::symbols::Symbol;

// offset, start line, start column
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct StmtStart(pub usize, pub usize, pub usize);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HPos {
    pub file_path: Symbol,

    pub start_offset: usize,
    pub end_offset: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub start_column: usize,
    pub end_column: usize,

    pub insertion_start: Option<StmtStart>,

    single_line: bool,
}

impl HPos {
    pub fn new(pos: &Pos, file_path: Symbol, stmt_start: Option<StmtStart>) -> HPos {
        let (start, end) = pos.to_start_and_end_lnum_bol_offset();
        let (start_line, line_start_beginning_offset, start_offset) = start;
        let (end_line, line_end_beginning_offset, end_offset) = end;

        let start_column = start_offset - line_start_beginning_offset + 1;
        let end_column = end_offset - line_end_beginning_offset + 1;

        let file_path = file_path.clone();

        return HPos {
            file_path,
            start_line,
            end_line,
            start_offset,
            end_offset,
            start_column,
            end_column,
            single_line: true,
            insertion_start: stmt_start,
        };
    }
}
