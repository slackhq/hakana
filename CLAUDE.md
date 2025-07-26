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
- `tests/inference/` - Type inference tests organized by feature
- `tests/security/` - Taint analysis and security tests  
- `tests/diff/` - Incremental analysis tests
- `tests/fix/` - Code transformation tests
- `tests/unused/` - Unused variable and expression analysis tests
- Each test has input.hack and output.txt files

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
