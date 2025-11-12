# HHAST Linter Migration Guide

This document shows how HHAST linters were ported to the Hakana lint framework, serving as a guide for migrating additional linters.

## Successfully Migrated Linters

### 1. NoWhitespaceAtEndOfLineLinter ✅

**HHAST Source**: `src/Linters/NoWhitespaceAtEndOfLineLinter.hack`

**Migration Strategy**: Line-based analysis

**Key Differences**:
- **HHAST**: Uses `LineLinter` base class, iterates over lines via framework
- **Hakana**: Manually splits source text by newlines, processes each line
- **Auto-fix**: Both support it identically

**Implementation Approach**:
```rust
// Iterate through source text line by line
for (line_num, line) in source.split(|&b| b == b'\n').enumerate() {
    // Find trailing whitespace by iterating from end
    let trimmed_len = line.iter()
        .rev()
        .take_while(|&&b| b == b' ' || b == b'\t')
        .count();

    // If found, create error with auto-fix
    if trimmed_len > 0 {
        let mut fix = EditSet::new();
        fix.add(Edit::delete(ws_start, ws_end));
        error = error.with_fix(fix);
    }
}
```

**Test Coverage**:
- ✅ Detects trailing spaces
- ✅ Detects trailing tabs
- ✅ No false positives on clean lines
- ✅ Auto-fix removes all trailing whitespace

---

### 2. UseStatementWithoutKindLinter ✅

**HHAST Source**: `src/Linters/UseStatementWIthoutKindLinter.hack`

**Migration Strategy**: AST visitor pattern

**Key Differences**:
- **HHAST**: Extends `ASTLinter`, overrides `getLintErrorForNode()`
- **Hakana**: Implements `SyntaxVisitor`, overrides `visit_namespace_use_declaration()`
- **Both**: Provide auto-fix by adding kind keyword

**Implementation Approach**:
```rust
impl<'a> SyntaxVisitor<'a> for UseStatementVisitor<'a> {
    fn visit_namespace_use_declaration(&mut self, node: &'a NamespaceUseDeclarationChildren) {
        // Check if kind field is Missing
        let has_kind = has_kind_keyword(&node.kind);

        if !has_kind {
            // Find insertion point (before clauses)
            let clauses_start = node.clauses.offset().unwrap_or(start);

            // Insert "type " before the imported names
            fix.add(Edit::insert(clauses_start, "type "));
        }
    }
}
```

**Simplifications Made**:
- **HHAST**: Analyzes usage context to determine if import is type/namespace/function
- **Hakana**: Defaults to `type` (most common case)
- **Rationale**: Context analysis requires full codebase scanning; can be added later

**Test Coverage**:
- ✅ Detects use without kind
- ✅ Accepts `use type`
- ✅ Accepts `use namespace`
- ✅ Accepts `use function`
- ✅ Auto-fix adds `type` keyword correctly

---

### 3. NoAwaitInLoopLinter ✅

**HHAST Source**: HHAST's `NoAwaitInLoopLinter`

**Migration Strategy**: Context-tracking visitor

**Key Differences**:
- **HHAST**: Single-pass AST traversal
- **Hakana**: Uses visitor with depth counter for nested loops

**Implementation Approach**:
```rust
struct NoAwaitVisitor<'a> {
    in_loop_depth: usize,
    errors: RefCell<Vec<LintError>>,
}

impl<'a> SyntaxVisitor<'a> for NoAwaitVisitor<'a> {
    // Track loop entry
    fn visit_foreach_statement(&mut self, _node: &'a ForeachStatementChildren) {
        self.in_loop_depth += 1;
    }

    // Check all nodes
    fn visit_node(&mut self, node: &'a PositionedSyntax<'a>) -> bool {
        if self.in_loop_depth > 0 {
            if let Some(token) = node.get_token() {
                if token.kind() == TokenKind::Await {
                    self.errors.borrow_mut().push(/* error */);
                }
            }
        }

        // After processing children, decrement depth
        match &node.children {
            SyntaxVariant::ForeachStatement(_) => {
                // Walk children...
                self.in_loop_depth -= 1;
            }
            _ => {}
        }
    }
}
```

**Test Coverage**:
- ✅ Detects await in foreach
- ✅ Detects await in while
- ✅ Detects await in for
- ✅ Handles nested loops correctly

---

## Migration Patterns

### Pattern 1: Line-Based Linters

For linters that analyze individual lines (like NoWhitespaceAtEndOfLineLinter):

```rust
impl Linter for MyLineLinter {
    fn lint<'a>(&self, ctx: &LintContext<'a>) -> Vec<LintError> {
        let source = ctx.source.text();
        let mut errors = Vec::new();
        let mut offset = 0;

        for (line_num, line) in source.split(|&b| b == b'\n').enumerate() {
            // Analyze line
            let line_str = std::str::from_utf8(line).unwrap_or("");

            // Check for issues
            if has_issue(line_str) {
                errors.push(LintError::new(
                    Severity::Warning,
                    "Issue detected",
                    offset + issue_start,
                    offset + issue_end,
                    self.name(),
                ));
            }

            offset += line.len() + 1; // +1 for newline
        }

        errors
    }
}
```

### Pattern 2: AST Node-Based Linters

For linters that check specific AST node types:

```rust
impl<'a> SyntaxVisitor<'a> for MyVisitor<'a> {
    fn visit_function_declaration(&mut self, node: &'a FunctionDeclarationChildren) {
        // Check specific node type
        // Access typed fields: node.attribute_spec, node.declaration_header, etc.
    }

    fn visit_classish_declaration(&mut self, node: &'a ClassishDeclarationChildren) {
        // Another node type
    }
}
```

### Pattern 3: Context-Tracking Linters

For linters that need to track where they are in the tree:

```rust
struct MyVisitor<'a> {
    in_async_function: bool,
    class_depth: usize,
    current_visibility: Option<TokenKind>,
}

impl<'a> SyntaxVisitor<'a> for MyVisitor<'a> {
    fn visit_function_declaration(&mut self, node: &'a FunctionDeclarationChildren) {
        let was_async = self.in_async_function;
        self.in_async_function = is_async(&node.declaration_header);

        // Process children with updated context...

        self.in_async_function = was_async; // Restore
    }
}
```

## Common Challenges & Solutions

### Challenge 1: Finding Correct Insertion Points

**Problem**: AST offsets don't always match expected text positions due to whitespace.

**Solution**: Use `node.offset()` and `node.end_offset()` from `SyntaxTrait` to get precise positions.

```rust
use parser_core_types::syntax_trait::SyntaxTrait;

let insert_pos = node.offset().unwrap_or(fallback);
fix.add(Edit::insert(insert_pos, "text to insert"));
```

### Challenge 2: Handling Missing Nodes

**Problem**: Optional syntax elements are represented as `SyntaxVariant::Missing`.

**Solution**: Pattern match on variants:

```rust
match &node.children {
    SyntaxVariant::Missing => {
        // Node is absent
    }
    SyntaxVariant::Token(token) => {
        // Node is a token
    }
    _ => {
        // Node has children
    }
}
```

### Challenge 3: Token Kind Checking

**Problem**: Need to check if a token is a specific keyword.

**Solution**: Use `LexableToken` trait:

```rust
use parser_core_types::lexable_token::LexableToken;
use parser_core_types::token_kind::TokenKind;

if let Some(token) = node.get_token() {
    match token.kind() {
        TokenKind::Async => { /* found async */ }
        TokenKind::Public => { /* found public */ }
        _ => {}
    }
}
```

### Challenge 4: Traversing Children

**Problem**: Need to manually walk children after processing a node.

**Solution**: Use `walk()` function:

```rust
// In visit_node() implementation
for child in node.iter_children() {
    crate::visitor::walk(self, child);
}
```

## Test Equivalence

Each migrated linter should have test coverage matching HHAST:

| Test Type | HHAST | Hakana | Status |
|-----------|-------|--------|--------|
| Basic detection | `.hack.expect` file | `#[test] fn test_detects_*()` | ✅ |
| No false positives | `.hack` file with no errors | `#[test] fn test_accepts_*()` | ✅ |
| Auto-fix | `.hack.autofix.expect` | `#[test] fn test_auto_fix()` | ✅ |
| Multiple issues | Tests with multiple errors | Multiple assertions in test | ✅ |

## Performance Considerations

### HHAST (Hack)
- Interpreted language
- Dynamic typing at runtime
- Framework overhead

### Hakana (Rust)
- Compiled to native code
- Zero-cost abstractions
- Static typing

**Expected Performance**: 5-10x faster for typical linters due to:
- No interpreter overhead
- Efficient arena allocation
- Inline optimizations
- SIMD where applicable

## Migration Checklist

When porting a new HHAST linter:

- [ ] Read the HHAST source to understand the check
- [ ] Identify the pattern (line-based, AST-based, context-tracking)
- [ ] Create file in `src/lint/examples/`
- [ ] Implement the `Linter` trait
- [ ] If AST-based, implement `SyntaxVisitor` with specific visit methods
- [ ] Add auto-fix support if applicable
- [ ] Write comprehensive tests:
  - [ ] Basic detection test
  - [ ] Negative test (no false positives)
  - [ ] Auto-fix test (if supported)
  - [ ] Edge cases
- [ ] Register in `examples/mod.rs`
- [ ] Update README.md with linter description
- [ ] Verify all tests pass: `cargo test --package hakana-lint`

## Resources

- **HHAST Linters**: https://github.com/hhvm/hhast/tree/main/src/Linters
- **Syntax Variants**: `third-party/hhvm/hphp/hack/src/parser/syntax_by_ref/syntax_variant_generated.rs`
- **Token Kinds**: `third-party/hhvm/hphp/hack/src/parser/token_kind.rs`
- **Framework Docs**: `src/lint/README.md` and `src/lint/QUICKSTART.md`
