//! MCP tool definitions.

use serde::Serialize;
use serde_json::{json, Value};

/// A tool definition for MCP
#[derive(Debug, Serialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

impl Tool {
    /// Definition for the find_symbol_usages tool
    pub fn find_symbol_usages_definition() -> Self {
        Tool {
            name: "find_symbol_usages".to_string(),
            description: "Find all usages of a symbol in the codebase. \
                Supports functions, classes, methods, properties, constants, \
                class constants, and type aliases. Returns file path, line, column \
                for each usage.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "symbol_name": {
                        "type": "string",
                        "description": "The fully-qualified name of the symbol to find usages for. \
                            Examples:\n\
                            - Function: 'MyNamespace\\myFunction'\n\
                            - Class: 'MyNamespace\\MyClass'\n\
                            - Method: 'MyNamespace\\MyClass::myMethod'\n\
                            - Property: 'MyNamespace\\MyClass::$propertyName'\n\
                            - Class constant: 'MyNamespace\\MyClass::CONSTANT_NAME'\n\
                            - Type alias: 'MyNamespace\\MyTypeAlias'"
                    }
                },
                "required": ["symbol_name"]
            }),
        }
    }

    /// Definition for the goto_definition tool
    pub fn goto_definition_definition() -> Self {
        Tool {
            name: "goto_definition".to_string(),
            description: "Go to the definition of a symbol at a specific location in a file. \
                Given a file path, line, and column, returns the location where the symbol \
                at that position is defined. Useful for navigating from usages to definitions \
                of functions, classes, methods, properties, and other symbols.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "The relative path to the file (relative to project root). \
                            Example: 'src/MyClass.hack'"
                    },
                    "line": {
                        "type": "integer",
                        "description": "The 1-indexed line number where the symbol is located"
                    },
                    "column": {
                        "type": "integer",
                        "description": "The 1-indexed column number where the symbol is located"
                    }
                },
                "required": ["file_path", "line", "column"]
            }),
        }
    }
}
