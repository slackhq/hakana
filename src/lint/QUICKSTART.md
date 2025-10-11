# Hakana Lint Framework - Quick Start Guide

Get started building linters and migrators in 5 minutes.

## Installation

The framework is already installed as part of the Hakana workspace. Import it from other crates:

```rust
use hakana_lint::{Linter, LintContext, LintError, Severity, SyntaxVisitor};
```

## Your First Linter

Let's build a simple linter that detects TODO comments in code.

### Step 1: Create the Linter Struct

```rust
pub struct TodoCommentLinter;
```

### Step 2: Implement the Linter Trait

```rust
impl Linter for TodoCommentLinter {
    fn name(&self) -> &'static str {
        "todo-comment"
    }

    fn description(&self) -> &'static str {
        "Detects TODO comments in code"
    }

    fn lint<'a>(&self, ctx: &LintContext<'a>) -> Vec<LintError> {
        // TODO: Implement the visitor
        vec![]
    }
}
```

### Step 3: Create a Visitor

The visitor walks the syntax tree and collects errors:

```rust
struct TodoVisitor<'a> {
    ctx: &'a LintContext<'a>,
    errors: Vec<LintError>,
}

impl<'a> SyntaxVisitor<'a> for TodoVisitor<'a> {
    fn visit_node(&mut self, node: &'a PositionedSyntax<'a>) {
        // Check for TODO in comments/text
        let text = self.ctx.node_text(node);
        if text.contains("TODO") {
            let (start, end) = self.ctx.node_range(node);
            self.errors.push(LintError::new(
                Severity::Warning,
                "Found TODO comment",
                start,
                end,
                "todo-comment",
            ));
        }
    }
}
```

### Step 4: Wire It Together

```rust
impl Linter for TodoCommentLinter {
    // ... name() and description() ...

    fn lint<'a>(&self, ctx: &LintContext<'a>) -> Vec<LintError> {
        let mut visitor = TodoVisitor {
            ctx,
            errors: Vec::new(),
        };

        // Walk the tree
        hakana_lint::visitor::walk(&mut visitor, ctx.root);

        visitor.errors
    }
}
```

### Step 5: Test It

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detects_todo() {
        let code = "// TODO: Fix this\nfunction foo() {}";

        // Parse the code
        let arena = bumpalo::Bump::new();
        let rel_path = Arc::new(RelativePath::make(
            Prefix::Root,
            PathBuf::from("test.hack"),
        ));
        let source = SourceText::make(rel_path, code.as_bytes());
        let (root, _) = hakana_lint::parse_file(&arena, &source);

        // Run the linter
        let ctx = LintContext::new(&source, &root, Path::new("test.hack"), false);
        let linter = TodoCommentLinter;
        let errors = linter.lint(&ctx);

        // Check results
        assert!(!errors.is_empty());
        assert!(errors[0].message.contains("TODO"));
    }
}
```

## Advanced: Auto-Fix

Let's add an auto-fix that removes TODO comments:

```rust
use hakana_lint::{Edit, EditSet};

fn lint<'a>(&self, ctx: &LintContext<'a>) -> Vec<LintError> {
    let mut visitor = TodoVisitor { ctx, errors: Vec::new() };
    hakana_lint::visitor::walk(&mut visitor, ctx.root);

    // Add fixes to each error
    visitor.errors.into_iter().map(|mut error| {
        if ctx.allow_auto_fix {
            let mut fix = EditSet::new();
            // Replace TODO comment with empty string
            fix.add(Edit::new(error.start_offset, error.end_offset, ""));
            error.fix = Some(fix);
        }
        error
    }).collect()
}

fn supports_auto_fix(&self) -> bool {
    true
}
```

## Pattern: Detecting Specific Node Types

Instead of checking every node, you can override specific visitor methods:

```rust
impl<'a> SyntaxVisitor<'a> for MyVisitor<'a> {
    // Only called for function declarations
    fn visit_function_declaration(
        &mut self,
        node: &'a FunctionDeclarationChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
        // node has typed fields: attribute_spec, declaration_header, body
        let header = &node.declaration_header;
        // ... inspect function ...
    }

    // Only called for foreach statements
    fn visit_foreach_statement(
        &mut self,
        node: &'a ForeachStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
        // node has typed fields: collection, key, value, body
        // ... inspect loop ...
    }
}
```

## Pattern: Tracking Context

Use struct fields to track state as you walk the tree:

```rust
struct MyVisitor<'a> {
    ctx: &'a LintContext<'a>,
    errors: Vec<LintError>,
    in_function: bool,        // Am I inside a function?
    loop_depth: usize,        // How deeply nested in loops?
    class_names: Vec<String>, // Stack of class names
}

impl<'a> SyntaxVisitor<'a> for MyVisitor<'a> {
    fn visit_function_declaration(&mut self, node: &'a FunctionDeclarationChildren) {
        self.in_function = true;
        // Process children...
        self.in_function = false;
    }

    fn visit_foreach_statement(&mut self, node: &'a ForeachStatementChildren) {
        self.loop_depth += 1;
        // Process children...
        self.loop_depth -= 1;
    }
}
```

## Pattern: Working with Tokens

To check specific keywords or operators:

```rust
use parser_core_types::lexable_token::LexableToken;
use parser_core_types::token_kind::TokenKind;

fn visit_node(&mut self, node: &'a PositionedSyntax<'a>) {
    if let Some(token) = node.get_token() {
        match token.kind() {
            TokenKind::Async => { /* found async keyword */ }
            TokenKind::Await => { /* found await */ }
            TokenKind::Public => { /* found public */ }
            _ => {}
        }
    }
}
```

## Pattern: Token Ranges and Trivia

When creating auto-fixes that modify tokens, use the context helpers to handle trivia (whitespace, comments) correctly:

```rust
use hakana_lint::{Edit, EditSet};

if let Some(token) = node.semicolon.get_token() {
    let mut fix = EditSet::new();

    // Get JUST the token (no leading/trailing whitespace)
    // Example: "  ; // comment" → returns range of just ";"
    let (start, end) = self.ctx.token_range(token);
    fix.add(Edit::delete(start, end));  // Preserves "  " and " // comment"

    // Get token WITH leading whitespace
    // Example: "  ; // comment" → returns range of "  ;"
    let (start, end) = self.ctx.token_range_with_leading(token);
    fix.add(Edit::delete(start, end));  // Preserves " // comment"

    // Get FULL range including all trivia (for error messages)
    let (start, end) = self.ctx.node_range(&node.semicolon);
    // Example: "  ; // comment" → returns range of entire string

    error = error.with_fix(fix);
}
```

**Why this matters**: The parser includes whitespace and comments as "trivia" around tokens. Most auto-fixes should preserve formatting by only modifying the token itself, not surrounding whitespace.

See `examples/no_empty_statements.rs` for a complete example.

## Running Your Linter

```rust
use hakana_lint::{run_linters, LintConfig};

let linters: Vec<&dyn Linter> = vec![&TodoCommentLinter];

let config = LintConfig {
    allow_auto_fix: true,
    apply_auto_fix: false,  // Just report, don't apply
    enabled_linters: vec![],  // Empty = all enabled
    disabled_linters: vec![],
};

let result = run_linters(
    Path::new("src/file.hack"),
    &file_contents,
    &linters,
    &config,
)?;

// Print errors
for error in &result.errors {
    println!("{}", error);
}

// Optionally apply fixes
if result.fixes_applied {
    if let Some(new_source) = result.modified_source {
        fs::write(result.file_path, new_source)?;
    }
}
```

## Common Node Types

```rust
// Top-level
visit_script()
visit_namespace_declaration()
visit_namespace_use_declaration()

// Classes and interfaces
visit_classish_declaration()
visit_methodish_declaration()
visit_property_declaration()

// Functions
visit_function_declaration()
visit_anonymous_function()
visit_lambda_expression()

// Statements
visit_if_statement()
visit_foreach_statement()
visit_for_statement()
visit_while_statement()
visit_return_statement()
visit_throw_statement()
visit_try_statement()

// Expressions
visit_function_call_expression()
visit_binary_expression()
visit_member_selection_expression()
visit_variable_expression()
```

## Debugging Tips

1. **Print the syntax tree**:
   ```rust
   eprintln!("Node: {:?}", node.children);
   ```

2. **Print node text**:
   ```rust
   eprintln!("Text: {}", ctx.node_text(node));
   ```

3. **Check node position**:
   ```rust
   let (start, end) = ctx.node_range(node);
   eprintln!("Position: {}..{}", start, end);
   ```

4. **Use visit_node() to see all nodes**:
   ```rust
   fn visit_node(&mut self, node: &'a PositionedSyntax<'a>) {
       eprintln!("Visiting: {:?}", node.children);
   }
   ```

## Next Steps

- Read the [full README](README.md) for more details
- Check out the [design document](../../LINT_FRAMEWORK_DESIGN.md)
- Look at example linters in `examples/`
- Browse HHAST linters for inspiration: https://github.com/hhvm/hhast

## Getting Help

- Check the syntax variant definitions: `third-party/hhvm/hphp/hack/src/parser/syntax_by_ref/syntax_variant_generated.rs`
- Look at token kinds: `third-party/hhvm/hphp/hack/src/parser/token_kind.rs`
- Run tests to see working examples: `cargo test --package hakana-lint`

Happy linting!
