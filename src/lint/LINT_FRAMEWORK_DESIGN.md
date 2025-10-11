# Hakana Lint Framework - Design Document

## Motivation

HHAST (Hack Abstract Syntax Tree) is a popular toolkit for building linters and migrators for Hack code. It provides:
- Full-fidelity AST parsing (preserves all formatting/comments)
- Easy-to-use APIs for code analysis and transformation
- Auto-fix capabilities
- Migration framework for large-scale refactoring

Hakana currently uses HHVM's higher-level "oxidized" AST for type analysis, which discards trivia. This design adds a HHAST-equivalent framework to Hakana that:
1. Uses the full-fidelity parser from HHVM directly (no exec/JSON indirection)
2. Enables porting existing HHAST linters
3. Reuses upstream AST types (no local codegen)
4. Integrates naturally with Hakana's architecture

## Technical Design

### 1. Parser Integration

**Goal**: Call HHVM's full-fidelity parser directly from Rust.

**Implementation**:
- HHVM submodule already contains `positioned_by_ref_parser` in Rust
- This parser returns `PositionedSyntax<'a>` with arena allocation
- Types are in `parser_core_types::syntax_by_ref::*`
- We wrap this in a simple API: `parse_file(arena, source) -> (PositionedSyntax, errors)`

**Benefits**:
- No need to shell out to HHVM or parse JSON
- Efficient arena-based memory management
- Direct access to upstream AST evolution

### 2. AST Representation

**Goal**: Use upstream AST types without local codegen.

**Implementation**:
- Import `positioned_syntax::PositionedSyntax` from HHVM
- Import `syntax_variant_generated::SyntaxVariant` for pattern matching
- Import `positioned_token::PositionedToken` for token access
- All types are parameterized: `Syntax<'a, Token, Value>`

**Structure**:
```rust
pub enum SyntaxVariant<'a, T, V> {
    Token(T),
    Missing,
    SyntaxList(&'a [Syntax<'a, T, V>]),
    FunctionDeclaration(&'a FunctionDeclarationChildren<'a, T, V>),
    // ... ~200+ variants
}

pub struct Syntax<'a, T, V> {
    pub children: SyntaxVariant<'a, T, V>,
    pub value: V,  // Carries position/offset info
}
```

**Benefits**:
- No maintenance burden from local codegen
- Automatic updates when HHVM AST evolves
- Type-safe pattern matching

### 3. Visitor Pattern

**Goal**: Easy tree traversal with selective node handling.

**Implementation**:
```rust
pub trait SyntaxVisitor<'a> {
    fn visit_function_declaration(&mut self, node: &'a FunctionDeclarationChildren) {}
    fn visit_foreach_statement(&mut self, node: &'a ForeachStatementChildren) {}
    // ... one method per node type, all with default no-op implementations
    fn visit_node(&mut self, node: &'a PositionedSyntax<'a>) {}  // Called for every node
}

pub fn walk<'a, V: SyntaxVisitor<'a>>(visitor: &mut V, node: &'a PositionedSyntax<'a>);
```

**Usage**:
```rust
impl<'a> SyntaxVisitor<'a> for MyVisitor<'a> {
    fn visit_foreach_statement(&mut self, node: &'a ForeachStatementChildren) {
        // Only implement the nodes you care about
    }
}
```

**Benefits**:
- Override only what you need (default impls for rest)
- Type-safe access to specific node fields
- Generic walk function handles recursion

### 4. Linter Framework

**Goal**: Trait-based system for implementing linters.

**Implementation**:
```rust
pub trait Linter: Send + Sync {
    fn name(&self) -> &'static str;
    fn lint<'a>(&self, ctx: &LintContext<'a>) -> Vec<LintError>;
    fn supports_auto_fix(&self) -> bool { false }
    fn description(&self) -> &'static str { "" }
}

pub struct LintContext<'a> {
    pub source: &'a SourceText<'a>,
    pub root: &'a PositionedSyntax<'a>,
    pub file_path: &'a Path,
    pub allow_auto_fix: bool,
}

pub struct LintError {
    pub severity: Severity,
    pub message: String,
    pub start_offset: usize,
    pub end_offset: usize,
    pub fix: Option<EditSet>,
    pub linter_name: &'static str,
}
```

**Benefits**:
- Simple trait interface
- Context provides all needed info
- Errors can carry auto-fixes
- Send + Sync enables parallelization

### 5. Edit System

**Goal**: Safe collection and application of text edits.

**Implementation**:
```rust
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

pub struct EditSet {
    edits: Vec<Edit>,
}

impl EditSet {
    pub fn add(&mut self, edit: Edit);
    pub fn apply(&self, source: &str) -> Result<String, String>;
    // Checks for overlaps, sorts by position, applies sequentially
}
```

**Benefits**:
- Overlap detection prevents corruption
- Immutable source text (functional style)
- Can collect edits from multiple passes

### 6. Migrator Framework

**Goal**: Multi-pass code transformations.

**Implementation**:
```rust
pub trait Migrator: Send + Sync {
    fn name(&self) -> &'static str;
    fn migrate<'a>(&self, ctx: &LintContext<'a>) -> Option<EditSet>;
    fn num_passes(&self) -> usize { 1 }
    fn description(&self) -> &'static str { "" }
    fn is_safe(&self) -> bool { true }
}
```

**Multi-pass Example**:
```rust
impl Migrator for ApiMigrator {
    fn num_passes(&self) -> usize { 2 }

    fn migrate<'a>(&self, ctx: &LintContext<'a>) -> Option<EditSet> {
        // Pass 1: Rename function calls
        // Pass 2: Update imports (after seeing new names)
    }
}
```

**Benefits**:
- Multi-pass for dependencies between changes
- Safety flag for dangerous migrations
- Same context as linters

### 7. Runner

**Goal**: Execute linters/migrators on files.

**Implementation**:
```rust
pub struct LintConfig {
    pub allow_auto_fix: bool,
    pub apply_auto_fix: bool,
    pub enabled_linters: Vec<String>,
    pub disabled_linters: Vec<String>,
}

pub fn run_linters(
    file_path: &Path,
    file_contents: &str,
    linters: &[&dyn Linter],
    config: &LintConfig,
) -> Result<LintResult, String>;
```

**Benefits**:
- Configuration for selective execution
- Can collect all errors before applying fixes
- Returns modified source if fixes applied

## Example: No Await In Loop

```rust
struct NoAwaitInLoopLinter;

impl Linter for NoAwaitInLoopLinter {
    fn name(&self) -> &'static str { "no-await-in-loop" }

    fn lint<'a>(&self, ctx: &LintContext<'a>) -> Vec<LintError> {
        let mut visitor = NoAwaitVisitor { ctx, errors: vec![], in_loop: false };
        walk(&mut visitor, ctx.root);
        visitor.errors
    }
}

struct NoAwaitVisitor<'a> {
    ctx: &'a LintContext<'a>,
    errors: Vec<LintError>,
    in_loop: bool,
}

impl<'a> SyntaxVisitor<'a> for NoAwaitVisitor<'a> {
    fn visit_foreach_statement(&mut self, node: &'a ForeachStatementChildren) {
        self.in_loop = true;
        walk(self, &node.body);
        self.in_loop = false;
    }

    fn visit_node(&mut self, node: &'a PositionedSyntax<'a>) {
        if self.in_loop {
            if let Some(token) = node.get_token() {
                if token.kind() == TokenKind::Await {
                    let (start, end) = self.ctx.node_range(node);
                    self.errors.push(LintError::new(
                        Severity::Warning,
                        "Await in loop",
                        start, end,
                        self.name(),
                    ));
                }
            }
        }
    }
}
```

## Comparison with Hakana's Main Analysis

| Aspect | Hakana Analysis | Lint Framework |
|--------|----------------|----------------|
| AST Type | Oxidized (higher-level) | Positioned (full-fidelity) |
| Trivia | Discarded | Preserved |
| Use Case | Type checking | Style/migrations |
| Modifications | No | Yes (auto-fix) |
| Performance | Optimized for inference | Simple traversals |
| Caching | Complex | Per-file |

## Future Enhancements

### Phase 2: CLI Integration
```bash
hakana lint src/          # Run all linters
hakana lint --fix src/    # Apply auto-fixes
hakana migrate api-v2 src/  # Run migrator
```

### Phase 3: Configuration
```json
{
  "linters": {
    "no-await-in-loop": "warn",
    "unused-variables": "error"
  },
  "migrators": {
    "api-v2": { "enabled": false }
  }
}
```

### Phase 4: LSP Integration
- Inline diagnostics in editor
- Quick fixes via code actions
- Real-time linting

### Phase 5: Plugin System
- Load external linters from dynamic libraries
- Similar to Hakana's existing hook system
- Trait objects with dynamic dispatch

## Migration Path from HHAST

For teams with existing HHAST linters:

1. **Port the visitor logic**: HHAST uses classes, we use traits + visitors
2. **Map node types**: Most are 1:1 between HHAST and positioned syntax
3. **Replace fix builders**: HHAST's `fix()` becomes `EditSet`
4. **Test behavior**: Add tests comparing output

Example mapping:
```hack
// HHAST
final class MyLinter extends ASTLinter {
  <<__Override>>
  protected function getLintErrorForNode(
    ForeachStatement $node,
    vec<LintError> $errors,
  ): ?LintError {
    // ...
  }
}
```

```rust
// Hakana Lint
struct MyLinter;
impl Linter for MyLinter {
    fn lint<'a>(&self, ctx: &LintContext<'a>) -> Vec<LintError> {
        let mut visitor = MyVisitor { ctx, errors: vec![] };
        walk(&mut visitor, ctx.root);
        visitor.errors
    }
}

impl<'a> SyntaxVisitor<'a> for MyVisitor<'a> {
    fn visit_foreach_statement(&mut self, node: &'a ForeachStatementChildren) {
        // ...
    }
}
```

## Implementation Status

- [x] Core framework structure
- [x] Parser integration
- [x] Visitor pattern
- [x] Linter trait
- [x] Migrator trait
- [x] Edit system with overlap detection
- [x] Runner with configuration
- [x] Example linter (NoAwaitInLoop)
- [x] Documentation
- [ ] CLI commands
- [ ] Configuration file support
- [ ] More example linters
- [ ] LSP integration
- [ ] Plugin system

## Testing Strategy

1. **Unit tests**: Each component tested in isolation
2. **Integration tests**: Full linter execution on sample files
3. **Comparison tests**: Compare output with HHAST linters (where applicable)
4. **Performance tests**: Ensure reasonable performance on large files

## Conclusion

This design provides a clean, Rust-idiomatic framework for building linters and migrators that:
- Reuses HHVM's full-fidelity parser infrastructure
- Avoids maintenance burden of local codegen
- Enables porting HHAST linters to Rust
- Integrates naturally with Hakana's architecture
- Provides safety and performance benefits of Rust

The trait-based system is flexible and extensible, while the visitor pattern keeps implementation simple and focused.
