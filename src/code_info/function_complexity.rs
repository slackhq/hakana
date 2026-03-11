use hakana_str::{Interner, StrId};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{function_context::FunctionLikeIdentifier, functionlike_info::FunctionLikeInfo};

#[derive(Serialize, Debug, Clone)]
pub struct FunctionComplexity {
    pub file_path: StrId,
    pub line: u32,
    pub name: FunctionLikeIdentifier,
    pub complexity: u32,
}

impl FunctionComplexity {
    pub fn from_functionlike(
        functionlike_id: &FunctionLikeIdentifier,
        functionlike_info: &FunctionLikeInfo,
        complexity: u32,
    ) -> Self {
        FunctionComplexity {
            file_path: functionlike_info.def_location.file_path.0,
            line: functionlike_info.def_location.start_line,
            name: functionlike_id.clone(),
            complexity,
        }
    }

    pub fn to_string(&self, interner: &Interner) -> String {
        format!(
            "{}:{} - {} (complexity: {})",
            interner.lookup(&self.file_path),
            self.line,
            self.name.to_string(interner),
            self.complexity
        )
    }

    pub fn to_json(&self, interner: &Interner) -> Value {
        json!({
            "name": self.name.to_string(interner),
            "complexity": self.complexity,
            "file_path": interner.lookup(&self.file_path),
            "line": self.line,
        })
    }

    pub fn cmp(&self, interner: &Interner, other: &Self) -> std::cmp::Ordering {
        self.complexity
            .cmp(&other.complexity)
            .then(
                self.name
                    .to_string(interner)
                    .cmp(&other.name.to_string(interner)),
            )
            .then(
                interner
                    .lookup(&self.file_path)
                    .cmp(interner.lookup(&other.file_path)),
            )
            .then(self.line.cmp(&other.line))
    }
}
