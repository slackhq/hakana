use oxidized::ast::Pos;
use serde::{Deserialize, Serialize};

use hakana_str::{Interner, StrId};

// offset, start line, start column
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct StmtStart {
    pub offset: u32,
    pub line: u32,
    pub column: u16,
    pub add_newline: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct FilePath(pub StrId);

impl FilePath {
    pub fn get_relative_path(&self, interner: &Interner, root_dir: &str) -> String {
        let full_path = interner.lookup(&self.0);
        if full_path.contains(root_dir) {
            full_path[(root_dir.len() + 1)..].to_string()
        } else {
            full_path.to_string()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct HPos {
    pub file_path: FilePath,

    pub start_offset: u32,
    pub end_offset: u32,
    pub start_line: u32,
    pub end_line: u32,
    pub start_column: u16,
    pub end_column: u16,

    pub insertion_start: Option<StmtStart>,
}

impl HPos {
    pub fn new(pos: &Pos, file_path: FilePath, stmt_start: Option<StmtStart>) -> HPos {
        let (start, end) = pos.to_start_and_end_lnum_bol_offset();
        let (start_line, line_start_beginning_offset, start_offset) = start;
        let (end_line, line_end_beginning_offset, end_offset) = end;

        let start_column = start_offset - line_start_beginning_offset + 1;
        let end_column = end_offset - line_end_beginning_offset + 1;

        HPos {
            file_path,
            start_line: start_line as u32,
            end_line: end_line as u32,
            start_offset: start_offset as u32,
            end_offset: end_offset as u32,
            start_column: start_column as u16,
            end_column: end_column as u16,
            insertion_start: stmt_start,
        }
    }
}
