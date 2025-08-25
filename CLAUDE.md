# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Development Commands

### Building
- `cargo build --release` - Build release version
- `git submodule init && git submodule update` - Initialize HHVM submodule (required for first build)
- `cd third-party/hhvm && git apply ../../hhvm-patch.diff && cd ../../` - Apply HHVM patches for WASM compilation
- Binary is created at `./target/release/hakana-default`

### Testing
- `cargo run --release --bin=hakana test --reuse-codebase tests` - Run all tests (recommended)
- `cargo run --bin hakana test <path-to-test-dir>` - Run individual test directory
- Test directories are organized under `tests/` with subdirectories for different test types
- **IMPORTANT**: Unused variable analysis only runs for tests in `tests/unused/` directory

### Security Analysis
- `cargo run --bin hakana security-check <path>` - Run security/taint analysis mode
- When security analysis is enabled, no other analysis is performed

### Binaries
- Main binary: `hakana` (src/main.rs)
- Language server: `hakana-language-server` (src/lsp.rs)

## Architecture Overview

Hakana is a typechecker for Hack built in Rust, designed to complement HHVM's built-in typechecker with enhanced type inference and security analysis.

### Hack File Conventions
- **File Extensions**: Use `.hack` for Hack source files
- **Opening Tags**: Hack files in this project should NOT start with `<?hh` - the opening tag is omitted
- **File Format**: Files should start directly with Hack code (classes, functions, namespaces, etc.)

### Core Components

**Workspace Structure**: Multi-crate workspace with specialized modules:
- `src/analyzer/` - Core type analysis engine with expression/statement analyzers
- `src/code_info/` - Type definitions, AST structures, and codebase metadata
- `src/orchestrator/` - High-level analysis coordination and caching
- `src/cli/` - Command-line interface and test runners
- `src/language_server/` - LSP implementation
- `src/code_info_builder/` - AST scanning and initial type inference

**Analysis Architecture**:
- Expression analysis in `src/analyzer/expr/` with specialized analyzers for calls, assignments, binary operations
- Statement analysis in `src/analyzer/stmt/` covering control flow, loops, conditionals
- Reconciler system in `src/analyzer/reconciler/` for type narrowing and assertion handling
- Scope management in `src/analyzer/scope/` for tracking variable types through control flow

**Type System**:
- Union types (`t_union.rs`) and atomic types (`t_atomic.rs`) as core type representations  
- Type combination and expansion in `src/code_info/ttype/`
- Template/generic support in `src/code_info/ttype/template/`
- Type comparison logic in `src/code_info/ttype/comparison/`

#### TAtomic Type Representation
- `TAtomic` enum in `src/code_info/ttype/t_atomic.rs` represents different atomic type variants
- Common variants include `TTypeAlias`, `TGenericParam`, `TNamedObject`, `TInt`, `TString`, etc.
- Type aliases (including newtypes) are represented as `TAtomic::TTypeAlias` with:
  - `name: StrId` - The type alias name (e.g., `StrId::MEMBER_OF` for `HH\MemberOf`)
  - `type_params: Option<Vec<TUnion>>` - Generic type parameters
  - `as_type: Option<Box<TUnion>>` - The underlying type for the alias
  - `newtype: bool` - Whether this is a newtype (distinct type) vs transparent alias

#### Type Intersection and Reconciliation
- Type narrowing (e.g., `is` checks) triggers intersection logic in `src/analyzer/reconciler/assertion_reconciler.rs`
- `intersect_atomic_with_atomic()` function handles intersection of two atomic types using pattern matching
- Each pattern match handles specific type combinations (e.g., `TInt` ∩ `TString` = empty)
- For complex types like `MemberOf`, intersection must:
  1. Extract the inner value type (second type parameter: `type_params[1]`)
  2. Recursively intersect the inner type with the other atomic type via `intersect_union_with_atomic()`
  3. Reconstruct the wrapper type preserving original structure
- **Important**: Bidirectional patterns needed - both `(MemberOf, Other)` and `(Other, MemberOf)` cases
- **Implementation Pattern**:
  ```rust
  (TAtomic::TTypeAlias { name: StrId::MEMBER_OF, type_params: Some(params), .. }, _) => {
      // Intersect inner type (params[1]) with the other type, preserve params[0]
      intersect_union_with_atomic(..., &params[1], other_type, ...)
          .map(|intersected| TAtomic::TTypeAlias {
              name: StrId::MEMBER_OF,
              type_params: Some(vec![params[0].clone(), intersected]),
              // ... preserve other fields
          })
  }
  ```

**Security Analysis**:
- Taint analysis system in `src/code_info/data_flow/` 
- Tracks data flow from sources (user input) to sinks (dangerous operations)
- Uses custom attributes for annotation: `@Sink`, `@Source`, `@Sanitize`, etc.
- Security-specific analysis separate from regular type checking

### Parser Integration
- Uses HHVM's parser via git submodule in `third-party/hhvm/`
- Requires nightly Rust toolchain
- HHVM patches applied for WASM compilation support

### Plugin System
- Hook-based architecture using Rust traits (`CustomHook`, `InternalHook`)
- Hooks available for expressions, statements, arguments, definitions, parameters
- Custom builds can extend functionality via plugins

### Test Organization
Tests are organized in the `tests/` directory with subdirectories for different test types. The test runner logic is in `src/cli/test_runners/test_runner.rs`.

**Standard Test Structure** (most directories):
- `input.hack` - Input source code file (should NOT start with `<?hh` - Hack files in this project omit the opening tag)
- `output.txt` - Expected output (optional, omit if no issues expected)

**Directory-Specific Test Configurations**:
- `tests/inference/` - Type inference tests organized by feature (input.hack + optional output.txt)
- `tests/security/` - Taint analysis and security tests (input.hack + optional output.txt)
- `tests/unused/` - Unused variable and expression analysis tests (input.hack + optional output.txt)
- `tests/nopanic/` - Tests that should not panic (input.hack + output.txt)
- `tests/parsing/` - Parser error tests (input.hack + output.txt)

**Special Test Types with Different Structure**:
- `tests/diff/` - Incremental analysis tests:
  - `a/`, `b/`, `c/`, `d/` subdirectories for different analysis stages
  - Each stage has `input.hack` files
  - `output.txt` contains expected final output
  - `a-before-analysis/` variant for pre-analysis changes
- `tests/fix/` - Code transformation tests:
  - `input.hack` - Original code
  - `output.txt` - Expected transformed code
  - `actual.txt` - Generated during test run
- `tests/add-fixmes/` - Fixme addition tests (same structure as fix/)
- `tests/remove-unused-fixmes/` - Fixme removal tests (same structure as fix/)
- `tests/migrations/` - Code migration tests:
  - `input.hack` - Original code
  - `output.txt` - Expected migrated code
  - `replacements.txt` - Migration replacement rules
- `tests/migration-candidates/` - Migration candidate detection:
  - `input.hack` - Source code
  - `candidates.txt` - Expected migration candidates
- `tests/executable-code-finder/` - Executable code detection:
  - `input.hack` - Source code
  - `output.txt` - JSON output of executable lines

**Configuration Files**:
- `config.json` - Optional per-test configuration overrides (e.g., max_changes_allowed)

### Debugging Type Issues
When encountering type-related test failures, common patterns include:

#### Common Error Types
- `LessSpecificReturnStatement` - Function returns a more general type than declared
  - Often indicates type narrowing/intersection not working properly
  - Check if reconciler patterns handle the specific type combination
- `InvalidReturnStatement` - Returned type doesn't match declaration
- `PossiblyUndefinedVariable` - Variable might not be defined in all code paths

#### Investigation Steps for Type Errors
1. **Identify the failing assertion/check** - What type operation is failing?
2. **Trace the type flow** - How does the type get to the error location?
3. **Check reconciler patterns** - Does `assertion_reconciler.rs` handle the type intersection?
4. **Verify TAtomic structure** - Is the type represented correctly in the type system?
5. **Add debug prints** - Use `eprintln!("{:?}", type_union)` to inspect type structures

#### Type System Debugging Tips
- `HH\MemberOf<Enum, Type>` has `type_params[0] = Enum` and `type_params[1] = Type`
- Newtype aliases have `newtype: true` and should preserve their wrapper structure
- Type intersections should be commutative - handle both `(A, B)` and `(B, A)` patterns
- Empty intersections (impossible types) return `None` from intersection functions

### Data Flow Analysis System
- Located in `src/analyzer/dataflow/` with core graph structures in `src/code_info/data_flow/`
- `unused_variable_analyzer.rs` contains logic for detecting unused variables and expressions
- Data flow graph tracks variable definitions, uses, and control flow boundaries
- Function analysis data (`FunctionAnalysisData`) accumulates analysis state including:
  - `data_flow_graph` - tracks variable usage patterns
  - Issue reporting and type inference state
- `report_unused_expressions()` in `functionlike_analyzer.rs` is the main entry point for unused analysis

#### Data Flow Graph Structure
- `DataFlowGraph` contains:
  - `sources` - HashMap of variable definition nodes (DataFlowNodeId -> DataFlowNode)
  - `sinks` - HashMap of variable usage nodes  
  - `forward_edges` - Maps from source to sink nodes with path information
  - `backward_edges` - Reverse mapping for traversal
- `DataFlowNodeId` variants include:
  - `Var(VarId, FilePath, u32, u32)` - Variable definitions
  - `Param(VarId, FilePath, u32, u32)` - Function parameters
- `VarId` contains a `StrId` that must be resolved using `interner.lookup(&var_id.0)`

#### Common Edge Cases in Unused Analysis
- **Multiple dataflow sources**: Variables redefined in loops create multiple source nodes
  - Example: `$x = 1; foreach(...) { $x = 2; }` creates two sources for variable `$x`
  - Analysis must group sources by variable name to handle this correctly
- **Control flow boundaries**: If/else blocks, loops, and try/catch create scoping challenges
  - Variables defined outside if blocks but only used inside should be flagged
  - But variables with multiple sources (e.g., loop redefinition) should not be flagged
- **Await expression detection**: Variables assigned from await expressions need special handling
  - `context.inside_await` tracks only whether we're inside an await call, which definitionally cannot happen in the assignment itself (since assignments are top-level expressions in Hack)
  - Use counter-based approach: check `analysis_data.await_calls_count` before/after RHS analysis
  - `has_await_call` field tracks if assignment RHS contained await expressions
  - `has_awaitable` field tracks if assignment type is `Awaitable<T>` (for function parameters)
- **Issue type naming**: Issue types may change names between commits (e.g., `VariableDefinedOutsideIfOnlyUsedInside` -> `VariableDefinedOutsideIf`)
  - Always check `src/code_info/issue.rs` for current issue type names
  - Update test output files when issue types change

#### VariableDefinedOutsideIf Analysis Implementation
The `VariableDefinedOutsideIf` and `AsyncVariableDefinedOutsideIf` checks are implemented in `check_variables_scoped_incorrectly()` in `src/analyzer/dataflow/unused_variable_analyzer.rs`.

**Key Guards Against False Positives**:
- **Multiple if block usage**: Variables used in multiple if blocks (e.g., both if and else) are exempted via `is_used_in_multiple_if_blocks()`
- **Loop optimization patterns**: Variables defined before loops but used inside if blocks within those loops are exempted via `is_optimization_pattern()`
- **Foreach iterator variables**: Variables defined in foreach iterator expressions are exempted via `is_position_within_foreach_init_bounds()`

**Boundary Tracking in FunctionAnalysisData**:
- `if_block_boundaries: Vec<(u32, u32)>` - Tracks if/else block boundaries from if_analyzer.rs and else_analyzer.rs
- `loop_boundaries: Vec<(u32, u32)>` - Tracks loop boundaries from loop_analyzer.rs (populated from `BlockContext.loop_bounds`)
- `for_loop_init_boundaries: Vec<(u32, u32)>` - Tracks foreach iterator variable definition boundaries from foreach_analyzer.rs

**Context State for Boundary Tracking**:
- `BlockContext.loop_bounds` - Set by foreach/while/for analyzers for the entire loop construct
- `BlockContext.for_loop_init_bounds` - Set by foreach_analyzer.rs for the iterator variable definition span
- Boundaries are collected during analysis and stored in `FunctionAnalysisData` for later use in unused variable analysis

**Testing Patterns**:
- Test both regular and async variants of the issue
- Test legitimate optimization patterns (variable hoisting before loops)
- Test foreach iterator variables (key and value variables)
- Test if/else usage patterns
- Always update both input.hack and output.txt files when adding test cases

### Hakana Attributes and String Interning

**String Interning for Hakana Attributes**:
When implementing analysis that references Hakana-specific attributes (e.g., `<<Hakana\ExclusiveEnumValues>>`, `<<Hakana\AllowNonExclusiveEnumValues>>`), add them to `src/str/build.rs` in the interned strings list. This:
- Ensures the strings are always available as `StrId` constants
- Allows referencing them as `StrId::EXCLUSIVE_ENUM_VALUES` instead of calling `interner.lookup()`
- Improves performance by avoiding string lookups during analysis
- Makes the code more maintainable and less error-prone

**Implementation Pattern**:
1. Add attribute strings to `src/str/build.rs`
2. Use the generated `StrId::CONSTANT_NAME` in analysis code
3. Compare attributes using `attr.name == StrId::CONSTANT_NAME` instead of string comparisons

**Example**:
```rust
// Instead of:
let attr_name = interner.lookup(&attr.name);
attr_name == "Hakana\\ExclusiveEnumValues"

// Use:
attr.name == StrId::HAKANA_EXCLUSIVE_ENUM_VALUES
```

**Note**: Hakana attribute constants are prefixed with `HAKANA_` in the generated StrId constants.

### Enum Exclusivity Checking System

**Overview**: Hakana provides a system to prevent copy-paste errors when implementing abstract class constants with enum types, ensuring each child class uses unique enum values when required.

**Architecture**:
- `<<Hakana\ExclusiveEnumValues>>` - Applied to enums to mark them as requiring exclusive usage
- `<<Hakana\AllowNonExclusiveEnumValues>>` - Applied to abstract class constants to allow duplicate enum values
- Two issue types: `NoEnumExclusivityAttribute` and `ExclusiveEnumValueReused`

**Implementation Details**:
- Enum exclusivity flag stored in `ConstantInfo.allow_non_exclusive_enum_values`
- Detection logic runs in main analysis phase (not just unused symbol detection)
- Uses pre-interned StrId constants for performance
- Only applies to production code (excludes test classes via `is_production_code`)

**Usage Patterns**:
```hack
// Exclusive enum (requires unique values in child classes)
<<Hakana\ExclusiveEnumValues>>
enum Priority: int { LOW = 1; HIGH = 2; }

abstract class Task {
    abstract const Priority TASK_PRIORITY;  // Will enforce exclusivity
}

// Non-exclusive override (allows duplicate values)
abstract class Document {
    <<Hakana\AllowNonExclusiveEnumValues>>
    abstract const Category DOCUMENT_CATEGORY;  // Allows duplicate values
}
```
