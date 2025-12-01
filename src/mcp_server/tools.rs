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
}
