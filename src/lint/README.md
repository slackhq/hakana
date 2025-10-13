# Hakana Lint Framework

A HHAST-equivalent linting and migration framework for Hakana, built on top of HHVM's full-fidelity parser.

## Overview

This framework enables building linters and migrators that work with the full-fidelity AST from HHVM's parser. Unlike Hakana's main analysis engine which uses the higher-level "oxidized" AST, this framework operates on the complete syntax tree that preserves all trivia (whitespace, comments, formatting) and enables precise code transformations.

## Key Features

- **Full-fidelity AST**: Access to all tokens, trivia, and formatting information
- **Visitor pattern**: Easy tree traversal with pattern matching on node types
- **Auto-fix support**: Collect edits and apply them safely with overlap detection
- **Migrator framework**: Multi-pass code transformations for large-scale refactoring
- **Trait-based**: Extensible Rust trait system for implementing custom linters
- **CLI integration**: `hakana lint` command with auto-fix support
- **HHAST configuration**: Compatible with HHAST's `.hhast-lint.json` config files
- **Suppression comments**: Full support for HHAST suppression syntax
- **Parallel execution**: Multi-threaded linting for fast performance on large codebases

## Architecture

### Core Components

1. **Parser Integration** (`lib.rs`)
   - Direct calls to HHVM's `positioned_by_ref_parser`
   - Returns `PositionedSyntax` trees with full trivia

2. **Context** (`context.rs`)
   - Provides access to source text, syntax tree, and file path
   - Helper methods for extracting text and positions
   - Token range helpers for working with trivia when creating auto-fixes

3. **Visitor** (`visitor.rs`)
   - Trait-based visitor pattern for AST traversal
   - Override only the node types you care about
   - Generic `walk()` function handles recursion

4. **Linters** (`linter.rs`)
   - `Linter` trait for implementing analysis
   - `LinterRegistry` for managing available linters
   - Support for auto-fixing

5. **Migrators** (`migrator.rs`)
   - `Migrator` trait for code transformations
   - Multi-pass support for complex migrations
   - Safety flags for dangerous operations

6. **Edits** (`edit.rs`)
   - `Edit` represents a single text replacement
   - `EditSet` collects and applies edits safely
   - Overlap detection prevents invalid transformations

7. **Errors** (`error.rs`)
   - `LintError` with severity levels
   - Optional auto-fix attachment
   - Source location tracking

8. **Runner** (`runner.rs`)
   - Execute linters on files
   - Apply auto-fixes
   - Configuration management
   - HHAST suppression comment parsing

9. **HHAST Configuration** (`hhast_config.rs`)
   - Parse `.hhast-lint.json` configuration files
   - Support for disabled linters and file exclusions
   - Compatible with HHAST's config format

## Usage

### Command Line Interface

The linting framework is integrated into Hakana's CLI:

```bash
# Lint files in current directory
hakana lint

# Lint specific files or directories
hakana lint src/ tests/

# Apply auto-fixes
hakana lint --apply-fixes

# Show what would be fixed without applying
hakana lint --show-fixes

# Use specific number of threads (default: number of CPU cores)
hakana lint --threads 4

# Specify config file location (default: .hhast-lint.json)
hakana lint --config path/to/config.json
```

### Configuration File

Create a `.hhast-lint.json` file in your project root:

```json
{
  "disabledLinters": [
    "NoAwaitInLoopLinter"
  ],
  "disabledAutoFixes": [
    "UseStatementWithoutKindLinter"
  ],
  "overrides": [
    {
      "patterns": ["tests/**"],
      "disabledLinters": ["DontDiscardNewExpressionsLinter"]
    }
  ]
}
```

Supported configuration options:
- `disabledLinters`: List of linter names to disable globally
- `disabledAutoFixes`: Linters whose auto-fixes should not be applied
- `overrides`: Path-specific configuration using glob patterns

### Suppression Comments

Suppress linter errors using HHAST-compatible comments:

```hack
// Suppress all linters for the next line
// HHAST_FIXME
$foo = bar();

// Suppress a specific linter for the next line
// HHAST_FIXME[NoEmptyStatements]
;

// Alternative syntax (functionally equivalent to FIXME)
// HHAST_IGNORE_ERROR[NoEmptyStatements]
;

// Suppress a linter for the entire file
// HHAST_IGNORE_ALL[NoEmptyStatements]

// Also works with block comments
/* HHAST_FIXME[DontDiscardNewExpressions] */
new Exception();
```

Suppression formats:
- `HHAST_IGNORE_ALL[LinterName]` - Suppresses the linter for the entire file
- `HHAST_FIXME[LinterName]` - Suppresses the linter for the next line
- `HHAST_IGNORE_ERROR[LinterName]` - Same as FIXME (next line only)
- `HHAST_IGNORE_ALL` / `HHAST_FIXME` / `HHAST_IGNORE_ERROR` (without linter name) - Suppresses all linters for the next line

The linter name can be specified as:
- Full HHAST name: `Facebook\HHAST\NoEmptyStatementsLinter`
- Class name: `NoEmptyStatementsLinter`
- Short name: `NoEmptyStatements`
- Hakana name: `no-empty-statements`

## Programmatic Usage

### Implementing a Linter

```rust
use hakana_lint::{Linter, LintContext, LintError, Severity, SyntaxVisitor};

// Define your linter
struct NoAwaitInLoopLinter;

impl Linter for NoAwaitInLoopLinter {
    fn name(&self) -> &'static str {
        "no-await-in-loop"
    }

    fn description(&self) -> &'static str {
        "Detects await expressions inside loops"
    }

    fn lint<'a>(&self, ctx: &LintContext<'a>) -> Vec<LintError> {
        let mut visitor = MyVisitor { ctx, errors: vec![] };
        hakana_lint::visitor::walk(&mut visitor, ctx.root);
        visitor.errors
    }
}

// Implement a visitor to walk the tree
struct MyVisitor<'a> {
    ctx: &'a LintContext<'a>,
    errors: Vec<LintError>,
}

impl<'a> SyntaxVisitor<'a> for MyVisitor<'a> {
    fn visit_foreach_statement(&mut self, node: &'a ForeachStatementChildren<...>) {
        // Check for await expressions in the body
        // Add errors as needed
    }
}
```

### Working with Tokens and Auto-Fixes

When creating auto-fixes, use the `LintContext` helper methods to properly handle trivia (whitespace, comments):

```rust
use hakana_lint::{Edit, EditSet};
use parser_core_types::lexable_token::LexableToken;

if let Some(token) = node.semicolon.get_token() {
    let mut fix = EditSet::new();

    // Get just the token range (excluding leading/trailing trivia)
    let (token_start, token_end) = ctx.token_range(token);
    fix.add(Edit::delete(token_start, token_end));

    // Or include leading trivia if needed
    let (with_leading_start, with_leading_end) = ctx.token_range_with_leading(token);

    error = error.with_fix(fix);
}
```

Three range methods are available:
- `ctx.token_range(token)` - Just the token itself (e.g., `;` without surrounding whitespace)
- `ctx.token_range_with_leading(token)` - Token plus leading trivia
- `ctx.node_range(node)` - Full node including all trivia (for error reporting)

See `examples/no_empty_statements.rs` for a complete example of handling trivia correctly.

### Running Linters

```rust
use hakana_lint::{run_linters, LintConfig};
use std::path::Path;

let linters: Vec<&dyn Linter> = vec![&NoAwaitInLoopLinter];
let config = LintConfig {
    allow_auto_fix: true,
    apply_auto_fix: false,
    ..Default::default()
};

let result = run_linters(
    Path::new("file.hack"),
    &file_contents,
    &linters,
    &config,
)?;

for error in result.errors {
    println!("{}", error);
}
```

### Implementing a Migrator

```rust
use hakana_lint::{Migrator, LintContext, EditSet};

struct ApiMigrator;

impl Migrator for ApiMigrator {
    fn name(&self) -> &'static str {
        "api-v2-migration"
    }

    fn description(&self) -> &'static str {
        "Migrates from API v1 to v2"
    }

    fn migrate<'a>(&self, ctx: &LintContext<'a>) -> Option<EditSet> {
        let mut edits = EditSet::new();

        // Walk the tree and collect edits
        // ...

        if edits.is_empty() {
            None
        } else {
            Some(edits)
        }
    }

    fn num_passes(&self) -> usize {
        2  // Run twice over the codebase
    }
}
```

## Comparison with HHAST

| Feature | HHAST | Hakana Lint |
|---------|-------|-------------|
| Language | Hack | Rust |
| AST Source | Full-fidelity parser | Same (via FFI) |
| Extensibility | Class inheritance | Trait system |
| Node Types | Local codegen | Upstream types |
| Auto-fix | Supported | Supported |
| Migrations | Supported | Supported |
| Performance | Interpreted | Compiled |

## Example Linters

The framework includes example linters ported from HHAST in `examples/`:

### NoAwaitInLoopLinter
- **Port of**: HHAST's `NoAwaitInLoopLinter`
- **Description**: Detects `await` expressions inside loops
- **Purpose**: Prevents common performance issues by identifying sequential async operations
- **Suggestion**: Use concurrent operations instead (e.g., `Vec\map_async`)
- **Auto-fix**: No (requires manual refactoring)

### NoWhitespaceAtEndOfLineLinter
- **Port of**: HHAST's `NoWhitespaceAtEndOfLineLinter`
- **Description**: Detects trailing whitespace at the end of lines
- **Purpose**: Maintains consistent code formatting
- **Auto-fix**: Yes - automatically removes trailing spaces and tabs

### UseStatementWithoutKindLinter
- **Port of**: HHAST's `UseStatementWithoutKindLinter`
- **Description**: Ensures `use` statements have explicit kind keywords
- **Purpose**: Improves code clarity by requiring `use type`, `use namespace`, `use function`, or `use const`
- **Example**: Flags `use Foo\Bar;` and suggests `use type Foo\Bar;`
- **Auto-fix**: Yes - adds `type` keyword by default

### NoEmptyStatementsLinter
- **Port of**: HHAST's `NoEmptyStatementsLinter`
- **Description**: Detects empty statements (semicolons with no effect) and expressions without side effects
- **Purpose**: Identifies unnecessary semicolons and expressions that don't affect execution
- **Examples**:
  - Standalone semicolons: `  ;` on their own line
  - Control flow with empty bodies: `if ($x) ;`
  - Expressions without side effects: `$a + $b;` (result not used)
- **Auto-fix**:
  - Yes - removes standalone semicolons while preserving surrounding whitespace
  - Yes - replaces control flow empty bodies with `{}` (e.g., `if ($x) ;` → `if ($x) {}`)

### DontDiscardNewExpressionsLinter
- **Port of**: HHAST's `DontDiscardNewExpressionsLinter`
- **Description**: Detects when objects are created with `new` but not assigned or used
- **Purpose**: Identifies likely bugs where constructors are called for side effects
- **Special handling**: Provides specific guidance for Exception types (suggests using `throw`)
- **Auto-fix**: No (requires manual refactoring)

### MustUseOverrideAttributeLinter
- **Port of**: HHAST's `MustUseOverrideAttributeLinter`
- **Description**: Suggests adding `<<__Override>>` attribute to methods in classes that extend other classes
- **Purpose**: Helps catch errors where methods unintentionally override parent methods
- **Note**: This is a simplified version that suggests the attribute for all public methods in extending classes; full implementation requires semantic analysis
- **Auto-fix**: No (requires semantic analysis)

### MustUseBracesForControlFlowLinter
- **Port of**: HHAST's `MustUseBracesForControlFlowLinter`
- **Description**: Requires braces for if, while, for, foreach, and else statements
- **Purpose**: Prevents bugs caused by missing braces when adding statements to control flow blocks
- **Example**: Flags `if ($x) echo 'hello';` and suggests wrapping in braces
- **Auto-fix**: Yes - wraps the statement body in braces

All linters include comprehensive test coverage demonstrating their behavior.

## Integration Status

The lint framework is fully integrated into Hakana's CLI. The following features are complete:

✅ **Completed**:
- CLI command: `hakana lint` with full argument support
- Configuration file support (`.hhast-lint.json`)
- HHAST-compatible suppression comments
- Parallel execution for large codebases
- Auto-fix support with `--apply-fixes` flag
- Multiple built-in linters ported from HHAST
- Path-based configuration overrides
- File exclusion patterns

🚧 **In Progress / Future Enhancements**:
- [ ] CLI command: `hakana migrate` for running migrators
- [ ] LSP integration for inline diagnostics and quick fixes
- [ ] Additional HHAST linters (ongoing porting effort)
- [ ] Caching for incremental linting
- [ ] Plugin system for external linters
- [ ] Performance profiling and optimization
- [ ] Integration with Hakana's existing issue reporting system

## Testing

### Unit Tests

```bash
# Run lint framework tests
cargo test --package hakana-lint

# Test a specific linter
cargo test --package hakana-lint no_await_in_loop
```

### HHAST Integration Tests

Linters ported from HHAST can use the integration test framework in `tests/hhast_tests/`:

```
tests/hhast_tests/
  NoEmptyStatementsLinter/
    empty_statements.php.in              # Input code with issues
    empty_statements.php.expect          # Expected error JSON (optional)
    empty_statements.php.autofix.expect  # Expected auto-fixed code (optional)
    type_error_thrown_on_autofix.php.in
    type_error_thrown_on_autofix.php.autofix.expect
```

Run integration tests:
```bash
# Run all HHAST integration tests
cargo run --release --bin=hakana test tests/hhast_tests/

# Run tests for a specific linter
cargo run --release --bin=hakana test tests/hhast_tests/NoEmptyStatementsLinter
```

The test runner automatically:
- Runs linters on `.php.in` and `.hack.in` files
- Compares errors against `.expect` files (if present)
- Tests auto-fixes against `.autofix.expect` files (if present)
- Reports diffs for any mismatches

## License

Same as Hakana (MIT)
