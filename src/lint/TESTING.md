# Testing Linters in Hakana

This document explains how to test linters in the Hakana lint framework.

## Test Types

### 1. Rust Unit Tests (Primary)

The main test suite uses Rust's built-in testing framework. Each linter includes comprehensive unit tests.

**Running all lint tests:**
```bash
cargo test --package hakana-lint
```

**Running specific linter tests:**
```bash
cargo test --package hakana-lint no_whitespace
cargo test --package hakana-lint use_statement
cargo test --package hakana-lint no_await
```

**Running with output:**
```bash
cargo test --package hakana-lint -- --nocapture
```

**Running in release mode (faster):**
```bash
cargo test --package hakana-lint --release
```

### 2. HHAST Integration Test Files

Linters ported from HHAST use the integration test framework in `tests/hhast_tests/`:

```
tests/hhast_tests/
├── NoEmptyStatementsLinter/
│   ├── empty_statements.php.in              # Input code with issues
│   ├── empty_statements.php.expect          # Expected error JSON (optional)
│   ├── empty_statements.php.autofix.expect  # Expected auto-fixed code
│   ├── type_error_thrown_on_autofix.php.in
│   └── type_error_thrown_on_autofix.php.autofix.expect
├── MustUseBracesForControlFlowLinter/
│   ├── if_without_braces.php.in
│   ├── if_without_braces.php.autofix.expect
│   └── ...
└── UseStatementWithoutKindLinter/
    ├── missing_kind.php.in
    ├── missing_kind.php.autofix.expect
    └── ...
```

**File naming conventions:**
- `.php.in` or `.hack.in` - Input file with code to be linted
- `.php.expect` or `.hack.expect` - Expected error output in JSON format (optional)
- `.php.autofix.expect` or `.hack.autofix.expect` - Expected code after auto-fix is applied

## Running Integration Tests

**Method 1: HHAST Integration Tests (For ported linters)**

Run HHAST-style integration tests through Hakana's test runner:

```bash
# Run all HHAST integration tests
cargo run --release --bin=hakana test tests/hhast_tests/

# Run tests for a specific linter
cargo run --release --bin=hakana test tests/hhast_tests/NoEmptyStatementsLinter

# Run a specific test case
cargo run --release --bin=hakana test tests/hhast_tests/NoEmptyStatementsLinter/empty_statements.php.in
```

The test runner automatically:
- Runs linters on `.php.in` and `.hack.in` files
- Compares errors against `.expect` files (if present)
- Tests auto-fixes against `.autofix.expect` files (if present)
- Reports diffs for any mismatches

**Method 2: Using Rust unit tests**

The Rust unit tests parse actual Hack files and verify behavior:

```bash
cd src/lint/examples
cargo test
```

**Method 3: Manual verification**

Run a linter on test files manually:

```rust
use hakana_lint::{run_linters, LintConfig};
use hakana_lint::examples::NoWhitespaceAtEndOfLineLinter;

let linter = NoWhitespaceAtEndOfLineLinter::new();
let config = LintConfig::default();

let result = run_linters(
    Path::new("tests/linters/NoWhitespaceAtEndOfLine/basic/input.hack"),
    &file_contents,
    &vec![&linter],
    &config,
)?;

for error in result.errors {
    println!("{}", error);
}
```

## Test Coverage

### Current Test Statistics

| Linter | Unit Tests | HHAST Integration | Notes |
|--------|-----------|-------------------|-------|
| NoWhitespaceAtEndOfLine | 4 | ✓ | Auto-fix supported |
| UseStatementWithoutKindLinter | 5 | ✓ | Auto-fix supported |
| NoAwaitInLoopLinter | 1 | - | No auto-fix |
| NoEmptyStatementsLinter | 4 | ✓ (2 test files) | Auto-fix with trivia handling |
| MustUseBracesForControlFlowLinter | 4 | ✓ | Auto-fix supported |
| DontDiscardNewExpressionsLinter | 3 | ✓ | No auto-fix |
| MustUseOverrideAttributeLinter | 2 | - | Requires semantic analysis |
| **Total** | **23** | **5 linters** | - |

All tests passing ✅

## Writing New Tests

### For Rust Unit Tests

Add tests to the linter's module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_linter_detects_issue() {
        let code = "// problematic code here";

        let arena = bumpalo::Bump::new();
        let rel_path = Arc::new(RelativePath::make(
            Prefix::Root,
            PathBuf::from("test.hack")
        ));
        let source = SourceText::make(rel_path, code.as_bytes());
        let (root, _) = crate::parse_file(&arena, &source);

        let ctx = LintContext::new(&source, &root, Path::new("test.hack"), false);
        let linter = MyLinter::new();
        let errors = linter.lint(&ctx);

        assert!(!errors.is_empty(), "Should detect issue");
        assert!(errors[0].message.contains("expected text"));
    }

    #[test]
    fn test_my_linter_auto_fix() {
        let code = "// code with issue";
        let expected = "// fixed code";

        // ... setup ...

        let ctx = LintContext::new(&source, &root, Path::new("test.hack"), true);
        let linter = MyLinter::new();
        let errors = linter.lint(&ctx);

        assert!(errors[0].fix.is_some(), "Should provide auto-fix");

        if let Some(ref fix) = errors[0].fix {
            let fixed = fix.apply(code).unwrap();
            assert_eq!(fixed, expected);
        }
    }
}
```

### For HHAST Integration Tests

Create a new directory in `tests/hhast_tests/` matching your linter name:

```bash
mkdir -p tests/hhast_tests/MyLinterName
```

Add test files following the HHAST convention:

**my_test_case.php.in:** (or `.hack.in`)
```hack
<?hh

function foo(): void {
    // Code that should trigger the linter
    $x = 1;
    ; // empty statement
}
```

**my_test_case.php.expect:** (optional - JSON error format)
```json
[
  {
    "description": "This statement is empty",
    "severity": "warning"
  }
]
```

**my_test_case.php.autofix.expect:** (if auto-fix supported)
```hack
<?hh

function foo(): void {
    // Code that should trigger the linter
    $x = 1;
     // empty statement removed, whitespace preserved
}
```

**Testing patterns:**
- Use `.php.in` for HHAST compatibility or `.hack.in` for clarity
- Omit `.expect` file if no errors are expected (test should pass)
- Omit `.autofix.expect` if auto-fix is not supported
- Multiple test cases can share the same directory

**Running your test:**
```bash
cargo run --release --bin=hakana test tests/hhast_tests/MyLinterName
```

## Test Best Practices

### 1. Test Coverage Checklist

For each linter, ensure you have tests for:
- [ ] Basic detection (finds the issue)
- [ ] No false positives (doesn't flag correct code)
- [ ] Edge cases (empty files, complex nesting, etc.)
- [ ] Auto-fix (if supported) - verify fix is correct
- [ ] Multiple issues in one file
- [ ] Different variations of the issue

### 2. Test Naming Conventions

```rust
test_detects_<issue>        // Basic detection
test_accepts_<valid_code>   // No false positive
test_auto_fix               // Auto-fix functionality
test_<edge_case>            // Specific edge case
```

### 3. Test Assertions

Be specific in assertions:

```rust
// Good:
assert_eq!(errors.len(), 2, "Should find exactly 2 issues");
assert!(errors[0].message.contains("trailing whitespace"));
assert_eq!(errors[0].severity, Severity::Warning);

// Less good:
assert!(!errors.is_empty());
```

### 4. Test Independence

Each test should be independent:

```rust
#[test]
fn test_one() {
    let code = "...";
    // Complete test setup and assertions
}

#[test]
fn test_two() {
    let code = "...";  // Fresh setup, no shared state
    // Complete test setup and assertions
}
```

## Debugging Tests

### Running a single test:
```bash
cargo test --package hakana-lint test_name
```

### With output:
```bash
cargo test --package hakana-lint test_name -- --nocapture
```

### With backtrace:
```bash
RUST_BACKTRACE=1 cargo test --package hakana-lint test_name
```

### Print debug info in tests:
```rust
#[test]
fn test_debug() {
    eprintln!("Debug: node = {:?}", node);
    eprintln!("Debug: text = {}", ctx.node_text(node));
    // ... rest of test
}
```

## Continuous Integration

For CI/CD pipelines:

```bash
# Run all lint tests
cargo test --package hakana-lint --release

# With coverage (requires cargo-tarpaulin)
cargo tarpaulin --package hakana-lint

# With specific test pattern
cargo test --package hakana-lint 'examples::*'
```

## Performance Testing

For performance-sensitive linters:

```rust
#[test]
fn bench_linter_performance() {
    use std::time::Instant;

    let code = // large code sample
    // ... setup ...

    let start = Instant::now();
    let errors = linter.lint(&ctx);
    let duration = start.elapsed();

    println!("Linting took: {:?}", duration);
    assert!(duration.as_millis() < 100, "Should be fast");
}
```

## Test Examples

### Example 1: Line-Based Linter Test

```rust
#[test]
fn test_trailing_whitespace() {
    let code = "function foo() {   \n    return;\n}";
    //                         ^^^ 3 trailing spaces

    let arena = bumpalo::Bump::new();
    let rel_path = Arc::new(RelativePath::make(Prefix::Root, PathBuf::from("test.hack")));
    let source = SourceText::make(rel_path, code.as_bytes());
    let (root, _) = crate::parse_file(&arena, &source);

    let ctx = LintContext::new(&source, &root, Path::new("test.hack"), false);
    let linter = NoWhitespaceAtEndOfLineLinter::new();
    let errors = linter.lint(&ctx);

    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("3 trailing whitespace"));
}
```

### Example 2: AST-Based Linter Test

```rust
#[test]
fn test_use_without_kind() {
    let code = "namespace Foo;\nuse Bar\\Baz;\n";

    let arena = bumpalo::Bump::new();
    let rel_path = Arc::new(RelativePath::make(Prefix::Root, PathBuf::from("test.hack")));
    let source = SourceText::make(rel_path, code.as_bytes());
    let (root, _) = crate::parse_file(&arena, &source);

    let ctx = LintContext::new(&source, &root, Path::new("test.hack"), false);
    let linter = UseStatementWithoutKindLinter::new();
    let errors = linter.lint(&ctx);

    assert!(!errors.is_empty());
    assert!(errors[0].message.contains("without explicit kind"));
}
```

### Example 3: Auto-Fix Test

```rust
#[test]
fn test_auto_fix() {
    let code = "use Foo\\Bar;\n";
    let expected_contains = "use type";

    let arena = bumpalo::Bump::new();
    let rel_path = Arc::new(RelativePath::make(Prefix::Root, PathBuf::from("test.hack")));
    let source = SourceText::make(rel_path, code.as_bytes());
    let (root, _) = crate::parse_file(&arena, &source);

    let ctx = LintContext::new(&source, &root, Path::new("test.hack"), true);
    let linter = UseStatementWithoutKindLinter::new();
    let errors = linter.lint(&ctx);

    assert!(errors[0].fix.is_some());

    let fixed = errors[0].fix.as_ref().unwrap().apply(code).unwrap();
    assert!(fixed.contains(expected_contains), "Fixed code: {}", fixed);
}
```

## Test Report

Generate a test report:

```bash
cargo test --package hakana-lint -- --format=json > test-report.json
```

Or with human-readable output:

```bash
cargo test --package hakana-lint 2>&1 | tee test-report.txt
```

## Future Integration

Planned future integration with Hakana's main test runner:

- [ ] Add `--lint` flag to `hakana test` command
- [ ] Support lint-specific test format in `tests/linters/`
- [ ] Integration with `hakana.json` config for test settings
- [ ] Parallel test execution for linters
- [ ] Coverage reporting integration

For now, use `cargo test --package hakana-lint` as the primary test method.
